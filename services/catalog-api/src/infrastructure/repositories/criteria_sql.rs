use chrono::{DateTime, Utc};
use ikeda_core::criteria::{
    FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, OrderDirection,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnKind {
    Text,
    Int64,
    Int32,
    Bool,
    Uuid,
    Timestamp,
}

#[derive(Debug, Clone)]
struct ResolvedField {
    sql_expr: String,
    kind: ColumnKind,
}

fn resolve_field(field: &str) -> Result<ResolvedField, AppError> {
    let trimmed = field.trim();
    match trimmed {
        "id" => Ok(ResolvedField {
            sql_expr: "id".to_string(),
            kind: ColumnKind::Uuid,
        }),
        "category_id" => Ok(ResolvedField {
            sql_expr: "category_id".to_string(),
            kind: ColumnKind::Uuid,
        }),
        "item_type" => Ok(ResolvedField {
            sql_expr: "item_type".to_string(),
            kind: ColumnKind::Text,
        }),
        "sku" => Ok(ResolvedField {
            sql_expr: "sku".to_string(),
            kind: ColumnKind::Text,
        }),
        "name" => Ok(ResolvedField {
            sql_expr: "name".to_string(),
            kind: ColumnKind::Text,
        }),
        "description" => Ok(ResolvedField {
            sql_expr: "description".to_string(),
            kind: ColumnKind::Text,
        }),
        "base_price" => Ok(ResolvedField {
            sql_expr: "base_price".to_string(),
            kind: ColumnKind::Int64,
        }),
        "currency" => Ok(ResolvedField {
            sql_expr: "currency".to_string(),
            kind: ColumnKind::Text,
        }),
        "tax_rate_bps" => Ok(ResolvedField {
            sql_expr: "tax_rate_bps".to_string(),
            kind: ColumnKind::Int32,
        }),
        "tax_included" => Ok(ResolvedField {
            sql_expr: "tax_included".to_string(),
            kind: ColumnKind::Bool,
        }),
        "duration_minutes" => Ok(ResolvedField {
            sql_expr: "duration_minutes".to_string(),
            kind: ColumnKind::Int32,
        }),
        "is_active" => Ok(ResolvedField {
            sql_expr: "is_active".to_string(),
            kind: ColumnKind::Bool,
        }),
        "created_at" => Ok(ResolvedField {
            sql_expr: "created_at".to_string(),
            kind: ColumnKind::Timestamp,
        }),
        "updated_at" => Ok(ResolvedField {
            sql_expr: "updated_at".to_string(),
            kind: ColumnKind::Timestamp,
        }),
        other => {
            if let Some(meta_key) = other.strip_prefix("metadata.") {
                if !meta_key.is_empty()
                    && meta_key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Ok(ResolvedField {
                        sql_expr: format!("metadata->>'{meta_key}'"),
                        kind: ColumnKind::Text,
                    });
                }
                return Err(AppError::BadRequest(format!(
                    "Invalid metadata key in filter: '{meta_key}'"
                )));
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on catalog_items: '{other}'"
            )))
        }
    }
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active `QueryBuilder`.
pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
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
            FilterRule::Condition(cond) => append_condition(qb, cond)?,
            FilterRule::Group(nested) => append_filter_group(qb, nested)?,
        }
    }
    qb.push(")");
    Ok(())
}

fn append_condition<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    cond: &'a FilterCondition,
) -> Result<(), AppError> {
    let resolved = resolve_field(&cond.field)?;

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
            let pattern = match cond.operator {
                FilterOperator::Contains => format!("%{raw}%"),
                FilterOperator::StartsWith => format!("{raw}%"),
                FilterOperator::EndsWith => format!("%{raw}"),
                _ => raw,
            };
            qb.push(resolved.sql_expr);
            qb.push(" ILIKE ");
            qb.push_bind(pattern);
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

/// Appends validated `ORDER BY` clauses to the query builder (defaults to `created_at DESC`).
pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    if order_by.is_empty() {
        qb.push(" ORDER BY created_at DESC");
        return Ok(());
    }

    qb.push(" ORDER BY ");
    for (idx, ord) in order_by.iter().enumerate() {
        if idx > 0 {
            qb.push(", ");
        }
        let resolved = resolve_field(&ord.field)?;
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
