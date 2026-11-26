use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    ColumnKind, FilterGroup, OrderBy, ResolvedField, MAX_METADATA_KEY_LEN,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

const MAX_JSONB_PATH_SEGMENTS: usize = 5;

/// Resolves a dotted JSONB path (`properties.<k1>.<k2>` or `context.<k1>`) into a safe
/// PostgreSQL JSONB text extraction expression (`properties->'k1'->>'k2'`).
fn resolve_jsonb_path_field(field: &str, column: &str) -> Option<Result<ResolvedField, AppError>> {
    let prefix = format!("{column}.");
    let raw_path = field.trim().strip_prefix(&prefix)?;

    if raw_path.is_empty() {
        return Some(Err(AppError::BadRequest(format!(
            "Invalid {column} JSONB path in filter: '{field}'"
        ))));
    }

    let segments: Vec<&str> = raw_path.split('.').collect();
    if segments.len() > MAX_JSONB_PATH_SEGMENTS {
        return Some(Err(AppError::BadRequest(format!(
            "JSONB path '{field}' exceeds maximum depth of {MAX_JSONB_PATH_SEGMENTS}"
        ))));
    }

    for seg in &segments {
        if seg.is_empty()
            || seg.len() > MAX_METADATA_KEY_LEN
            || !seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some(Err(AppError::BadRequest(format!(
                "Invalid {column} JSONB segment '{seg}' in filter: '{field}'"
            ))));
        }
    }

    let mut expr = String::from(column);
    let last_idx = segments.len().saturating_sub(1);
    for (idx, seg) in segments.iter().enumerate() {
        if idx == last_idx {
            expr.push_str("->>'");
        } else {
            expr.push_str("->'");
        }
        expr.push_str(seg);
        expr.push('\'');
    }

    Some(Ok(ResolvedField::new(expr, ColumnKind::Text)))
}

/// Validates and maps a `telemetry_events` API field name (whitelisted column or
/// `properties.<path>` / `context.<path>`) to a safe SQL expression.
fn resolve_telemetry_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "kind" => Ok(ResolvedField::new("kind", ColumnKind::Text)),
        "event_name" => Ok(ResolvedField::new("event_name", ColumnKind::Text)),
        "source" => Ok(ResolvedField::new("source", ColumnKind::Text)),
        "client_id" => Ok(ResolvedField::new("client_id", ColumnKind::Uuid)),
        "actor_id" => Ok(ResolvedField::new("actor_id", ColumnKind::Uuid)),
        "anonymous_id" => Ok(ResolvedField::new("anonymous_id", ColumnKind::Text)),
        "session_id" => Ok(ResolvedField::new("session_id", ColumnKind::Text)),
        "resource_type" => Ok(ResolvedField::new("resource_type", ColumnKind::Text)),
        "resource_id" => Ok(ResolvedField::new("resource_id", ColumnKind::Uuid)),
        "request_id" => Ok(ResolvedField::new("request_id", ColumnKind::Text)),
        "occurred_at" => Ok(ResolvedField::new("occurred_at", ColumnKind::Timestamp)),
        "received_at" => Ok(ResolvedField::new("received_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_jsonb_path_field(other, "properties") {
                return res;
            }
            if let Some(res) = resolve_jsonb_path_field(other, "context") {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on telemetry_events: '{other}'"
            )))
        }
    }
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active `QueryBuilder` using the
/// `telemetry_events` whitelist.
pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_telemetry_field)
}

/// Appends validated `ORDER BY` clauses to the query builder (defaults to `occurred_at DESC, received_at DESC`).
pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(
        qb,
        order_by,
        "occurred_at DESC, received_at DESC",
        &resolve_telemetry_field,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_whitelisted_and_jsonb_fields() {
        assert_eq!(
            resolve_telemetry_field("event_name").unwrap(),
            ResolvedField::new("event_name", ColumnKind::Text)
        );
        assert_eq!(
            resolve_telemetry_field("properties.path").unwrap(),
            ResolvedField::new("properties->>'path'", ColumnKind::Text)
        );
        assert_eq!(
            resolve_telemetry_field("properties.utm.source").unwrap(),
            ResolvedField::new("properties->'utm'->>'source'", ColumnKind::Text)
        );
        assert_eq!(
            resolve_telemetry_field("context.user_agent").unwrap(),
            ResolvedField::new("context->>'user_agent'", ColumnKind::Text)
        );
    }

    #[test]
    fn test_rejects_sql_injection_in_properties_path() {
        assert!(resolve_telemetry_field("properties.foo' OR '1'='1").is_err());
        assert!(resolve_telemetry_field("properties.").is_err());
        assert!(resolve_telemetry_field("properties.a..b").is_err());
        assert!(resolve_telemetry_field("properties.a.b.c.d.e.f").is_err());
        assert!(resolve_telemetry_field("tenant_id").is_err());
        assert!(resolve_telemetry_field("event_name; DROP TABLE telemetry_events--").is_err());
    }
}
