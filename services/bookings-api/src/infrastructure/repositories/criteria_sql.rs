use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    resolve_metadata_field, ColumnKind, FilterGroup, OrderBy, ResolvedField,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

/// Validates and maps a `bookings` API field name (whitelisted column or `metadata.<key>`)
/// to a safe SQL expression.
fn resolve_booking_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "booking_code" => Ok(ResolvedField::new("booking_code", ColumnKind::Text)),
        "client_id" => Ok(ResolvedField::new("client_id", ColumnKind::Uuid)),
        "calendar_id" => Ok(ResolvedField::new("calendar_id", ColumnKind::Uuid)),
        "event_id" => Ok(ResolvedField::new("event_id", ColumnKind::Uuid)),
        "starts_at" => Ok(ResolvedField::new("starts_at", ColumnKind::Timestamp)),
        "ends_at" => Ok(ResolvedField::new("ends_at", ColumnKind::Timestamp)),
        "timezone" => Ok(ResolvedField::new("timezone", ColumnKind::Text)),
        "status" => Ok(ResolvedField::new("status", ColumnKind::Text)),
        "subtotal" => Ok(ResolvedField::new("subtotal", ColumnKind::Int64)),
        "tax_total" => Ok(ResolvedField::new("tax_total", ColumnKind::Int64)),
        "total_price" => Ok(ResolvedField::new("total_price", ColumnKind::Int64)),
        "total_duration_minutes" => Ok(ResolvedField::new(
            "total_duration_minutes",
            ColumnKind::Int32,
        )),
        "currency" => Ok(ResolvedField::new("currency", ColumnKind::Text)),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on bookings: '{other}'"
            )))
        }
    }
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active `QueryBuilder` using the `bookings` whitelist.
pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_booking_field)
}

/// Appends validated `ORDER BY` clauses to the query builder (defaults to `starts_at DESC`).
pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(qb, order_by, "starts_at DESC", &resolve_booking_field)
}
