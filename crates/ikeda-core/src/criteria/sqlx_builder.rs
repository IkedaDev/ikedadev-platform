use crate::criteria::{
    FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, OrderDirection,
};
use crate::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// Maximum nesting depth allowed for recursive `FilterGroup` trees to prevent DoS.
pub const MAX_FILTER_DEPTH: usize = 10;

/// Maximum length allowed for JSONB `metadata.<key>` identifiers.
pub const MAX_METADATA_KEY_LEN: usize = 64;

/// PostgreSQL column type used to safely validate and bind `Criteria` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Int64,
    Int32,
    Bool,
    Uuid,
    Timestamp,
}

/// Safe SQL column expression resolved from an API field whitelist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedField {
    pub sql_expr: String,
    pub kind: ColumnKind,
}

impl ResolvedField {
    /// Creates a new whitelisted SQL field descriptor.
    #[must_use]
    pub fn new(sql_expr: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            sql_expr: sql_expr.into(),
            kind,
        }
    }
}

/// Resolves a `metadata.<key>` field into a safe PostgreSQL JSONB text extraction expression.
///
/// Returns `None` if `field` does not start with `"metadata."`.
/// Returns `Some(Err(AppError::BadRequest))` if the key is empty, too long, or contains
/// characters outside `[a-zA-Z0-9_-]`.
#[must_use]
pub fn resolve_metadata_field(field: &str) -> Option<Result<ResolvedField, AppError>> {
    let meta_key = field.trim().strip_prefix("metadata.")?;
    if !meta_key.is_empty()
        && meta_key.len() <= MAX_METADATA_KEY_LEN
        && meta_key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(Ok(ResolvedField::new(
            format!("metadata->>'{meta_key}'"),
            ColumnKind::Text,
        )));
    }
    Some(Err(AppError::BadRequest(format!(
        "Invalid metadata key in filter: '{meta_key}'"
    ))))
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active SQLx `QueryBuilder`.
///
/// Every field name is resolved through the caller-supplied whitelist `resolver` to prevent
/// SQL injection in column names and JSONB paths. All values are bound as query parameters.
pub fn append_filter_group<'a, F>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
    resolver: &F,
) -> Result<(), AppError>
where
    F: Fn(&str) -> Result<ResolvedField, AppError>,
{
    append_filter_group_with_depth(qb, group, resolver, 0)
}

fn append_filter_group_with_depth<'a, F>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
    resolver: &F,
    depth: usize,
) -> Result<(), AppError>
where
    F: Fn(&str) -> Result<ResolvedField, AppError>,
{
    if depth > MAX_FILTER_DEPTH {
        return Err(AppError::BadRequest(format!(
            "Filter nesting exceeds maximum allowed depth of {MAX_FILTER_DEPTH}"
        )));
    }

    let (conjunction, rules) = match group {
        FilterGroup::And(and) => (" AND ", and),
        FilterGroup::Or(or) => (" OR ", or),
    };

    if rules.is_empty() {
        qb.push("TRUE");
        return Ok(());
    }

    qb.push("(");
    for (idx, rule) in rules.iter().enumerate() {
        if idx > 0 {
            qb.push(conjunction);
        }
        match rule {
            FilterRule::Condition(cond) => append_condition(qb, cond, resolver)?,
            FilterRule::Group(nested) => {
                append_filter_group_with_depth(qb, nested, resolver, depth + 1)?;
            }
        }
    }
    qb.push(")");
    Ok(())
}

fn escape_like_pattern(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn append_condition<'a, F>(
    qb: &mut QueryBuilder<'a, Postgres>,
    cond: &'a FilterCondition,
    resolver: &F,
) -> Result<(), AppError>
where
    F: Fn(&str) -> Result<ResolvedField, AppError>,
{
    let resolved = resolver(&cond.field)?;

    match cond.operator {
        FilterOperator::IsNull => {
            qb.push(resolved.sql_expr);
            qb.push(" IS NULL");
            return Ok(());
        }
        FilterOperator::IsNotNull => {
            qb.push(resolved.sql_expr);
            qb.push(" IS NOT NULL");
            return Ok(());
        }
        FilterOperator::Contains | FilterOperator::StartsWith | FilterOperator::EndsWith => {
            if resolved.kind != ColumnKind::Text {
                return Err(AppError::BadRequest(format!(
                    "Operator {:?} is only supported on text/metadata fields (field '{}')",
                    cond.operator, cond.field
                )));
            }
            let raw = value_as_string(&cond.value, &cond.field)?;
            let escaped = escape_like_pattern(&raw);
            let pattern = match cond.operator {
                FilterOperator::Contains => format!("%{escaped}%"),
                FilterOperator::StartsWith => format!("{escaped}%"),
                FilterOperator::EndsWith => format!("%{escaped}"),
                _ => escaped,
            };
            qb.push(resolved.sql_expr);
            qb.push(" ILIKE ");
            qb.push_bind(pattern);
            qb.push(r" ESCAPE '\'");
            return Ok(());
        }
        FilterOperator::In | FilterOperator::NotIn => {
            let arr = cond.value.as_array().ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Operator {:?} requires an array value for field '{}'",
                    cond.operator, cond.field
                ))
            })?;
            if arr.is_empty() {
                qb.push(if cond.operator == FilterOperator::In {
                    "FALSE"
                } else {
                    "TRUE"
                });
                return Ok(());
            }
            qb.push(resolved.sql_expr);
            qb.push(if cond.operator == FilterOperator::In {
                " IN ("
            } else {
                " NOT IN ("
            });
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    qb.push(", ");
                }
                bind_typed_value(qb, resolved.kind, item, &cond.field)?;
            }
            qb.push(")");
            return Ok(());
        }
        FilterOperator::Eq
        | FilterOperator::Neq
        | FilterOperator::Gt
        | FilterOperator::Gte
        | FilterOperator::Lt
        | FilterOperator::Lte => {
            let op_sql = match cond.operator {
                FilterOperator::Eq => " = ",
                FilterOperator::Neq => " != ",
                FilterOperator::Gt => " > ",
                FilterOperator::Gte => " >= ",
                FilterOperator::Lt => " < ",
                FilterOperator::Lte => " <= ",
                _ => " = ",
            };
            qb.push(resolved.sql_expr);
            qb.push(op_sql);
            bind_typed_value(qb, resolved.kind, &cond.value, &cond.field)?;
        }
    }

    Ok(())
}

fn bind_typed_value<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    kind: ColumnKind,
    val: &'a serde_json::Value,
    field: &str,
) -> Result<(), AppError> {
    match kind {
        ColumnKind::Text => {
            let s = value_as_string(val, field)?;
            qb.push_bind(s);
        }
        ColumnKind::Int64 => {
            let n = val.as_i64().ok_or_else(|| {
                AppError::BadRequest(format!("Field '{field}' expects an integer value"))
            })?;
            qb.push_bind(n);
        }
        ColumnKind::Int32 => {
            let n = val
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| {
                    AppError::BadRequest(format!("Field '{field}' expects a 32-bit integer value"))
                })?;
            qb.push_bind(n);
        }
        ColumnKind::Bool => {
            let b = val.as_bool().ok_or_else(|| {
                AppError::BadRequest(format!("Field '{field}' expects a boolean value"))
            })?;
            qb.push_bind(b);
        }
        ColumnKind::Uuid => {
            let s = val.as_str().ok_or_else(|| {
                AppError::BadRequest(format!("Field '{field}' expects a UUID string"))
            })?;
            let id = Uuid::parse_str(s.trim()).map_err(|_| {
                AppError::BadRequest(format!("Field '{field}' contains invalid UUID: '{s}'"))
            })?;
            qb.push_bind(id);
        }
        ColumnKind::Timestamp => {
            let s = val.as_str().ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Field '{field}' expects an ISO-8601 timestamp string"
                ))
            })?;
            let dt = DateTime::parse_from_rfc3339(s.trim())
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| {
                    AppError::BadRequest(format!(
                        "Field '{field}' contains invalid RFC3339 timestamp: '{s}'"
                    ))
                })?;
            qb.push_bind(dt);
        }
    }
    Ok(())
}

fn value_as_string(val: &serde_json::Value, field: &str) -> Result<String, AppError> {
    match val {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(AppError::BadRequest(format!(
            "Field '{field}' expects a scalar string/number/boolean value"
        ))),
    }
}

/// Appends validated `ORDER BY` clauses to the query builder, falling back to `default_order`
/// (e.g. `"created_at DESC"`) when `order_by` is empty.
pub fn append_order_by<F>(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
    default_order: &str,
    resolver: &F,
) -> Result<(), AppError>
where
    F: Fn(&str) -> Result<ResolvedField, AppError>,
{
    if order_by.is_empty() {
        qb.push(" ORDER BY ");
        qb.push(default_order);
        return Ok(());
    }

    qb.push(" ORDER BY ");
    for (idx, ord) in order_by.iter().enumerate() {
        if idx > 0 {
            qb.push(", ");
        }
        let resolved = resolver(&ord.field)?;
        let dir = match ord.direction {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        };
        qb.push(resolved.sql_expr);
        qb.push(" ");
        qb.push(dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_resolver(field: &str) -> Result<ResolvedField, AppError> {
        match field.trim() {
            "name" => Ok(ResolvedField::new("name", ColumnKind::Text)),
            "price" => Ok(ResolvedField::new("price", ColumnKind::Int64)),
            "is_active" => Ok(ResolvedField::new("is_active", ColumnKind::Bool)),
            other => {
                if let Some(res) = resolve_metadata_field(other) {
                    return res;
                }
                Err(AppError::BadRequest(format!(
                    "Unsupported field: '{other}'"
                )))
            }
        }
    }

    #[test]
    fn test_rejects_sql_injection_in_column_and_metadata() {
        assert!(test_resolver("name; DROP TABLE clients--").is_err());
        assert!(test_resolver("metadata.foo' OR '1'='1").is_err());
        assert!(test_resolver("metadata.").is_err());
        assert!(test_resolver(&format!("metadata.{}", "a".repeat(65))).is_err());
        assert_eq!(
            test_resolver("metadata.vip_tier-1").unwrap(),
            ResolvedField::new("metadata->>'vip_tier-1'", ColumnKind::Text)
        );
    }

    #[test]
    fn test_escapes_ilike_wildcards() {
        let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("SELECT 1 WHERE ");
        let group = FilterGroup::And(vec![FilterRule::Condition(FilterCondition {
            field: "name".to_string(),
            operator: FilterOperator::Contains,
            value: json!("100%_test\\x"),
        })]);

        append_filter_group(&mut qb, &group, &test_resolver).unwrap();
        assert_eq!(qb.sql(), r"SELECT 1 WHERE (name ILIKE $1 ESCAPE '\')");
        assert_eq!(escape_like_pattern("100%_test\\x"), r"100\%\_test\\x");
    }

    #[test]
    fn test_rejects_excessive_filter_nesting() {
        let mut group = FilterGroup::And(vec![FilterRule::Condition(FilterCondition {
            field: "is_active".to_string(),
            operator: FilterOperator::Eq,
            value: json!(true),
        })]);
        for _ in 0..=MAX_FILTER_DEPTH {
            group = FilterGroup::And(vec![FilterRule::Group(Box::new(group))]);
        }

        let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("SELECT 1 WHERE ");
        let res = append_filter_group(&mut qb, &group, &test_resolver);
        assert!(matches!(res, Err(AppError::BadRequest(_))));
    }
}
