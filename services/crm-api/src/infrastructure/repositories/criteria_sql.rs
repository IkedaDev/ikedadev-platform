use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    resolve_metadata_field, ColumnKind, FilterGroup, OrderBy, ResolvedField,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

/// Validates and maps a CRM `clients` API field name (whitelisted column or `metadata.<key>`)
/// to a safe SQL expression.
fn resolve_client_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "full_name" => Ok(ResolvedField::new("full_name", ColumnKind::Text)),
        "email" => Ok(ResolvedField::new("email", ColumnKind::Text)),
        "phone" => Ok(ResolvedField::new("phone", ColumnKind::Text)),
        "tax_id" => Ok(ResolvedField::new("tax_id", ColumnKind::Text)),
        "address" => Ok(ResolvedField::new("address", ColumnKind::Text)),
        "notes" => Ok(ResolvedField::new("notes", ColumnKind::Text)),
        "keycloak_user_id" => Ok(ResolvedField::new("keycloak_user_id", ColumnKind::Uuid)),
        "is_active" => Ok(ResolvedField::new("is_active", ColumnKind::Bool)),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on clients: '{other}'"
            )))
        }
    }
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active `QueryBuilder` using the `clients` whitelist.
pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_client_field)
}

/// Appends validated `ORDER BY` clauses to the query builder (defaults to `created_at DESC`).
pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(qb, order_by, "created_at DESC", &resolve_client_field)
}
