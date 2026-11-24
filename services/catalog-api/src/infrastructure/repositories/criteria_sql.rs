use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    resolve_metadata_field, ColumnKind, FilterGroup, OrderBy, ResolvedField,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

/// Validates and maps a `catalog_items` API field name (whitelisted column or `metadata.<key>`)
/// to a safe SQL expression.
fn resolve_catalog_item_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "category_id" => Ok(ResolvedField::new("category_id", ColumnKind::Uuid)),
        "item_type" => Ok(ResolvedField::new("item_type", ColumnKind::Text)),
        "sku" => Ok(ResolvedField::new("sku", ColumnKind::Text)),
        "name" => Ok(ResolvedField::new("name", ColumnKind::Text)),
        "description" => Ok(ResolvedField::new("description", ColumnKind::Text)),
        "base_price" => Ok(ResolvedField::new("base_price", ColumnKind::Int64)),
        "currency" => Ok(ResolvedField::new("currency", ColumnKind::Text)),
        "tax_rate_bps" => Ok(ResolvedField::new("tax_rate_bps", ColumnKind::Int32)),
        "tax_included" => Ok(ResolvedField::new("tax_included", ColumnKind::Bool)),
        "duration_minutes" => Ok(ResolvedField::new("duration_minutes", ColumnKind::Int32)),
        "is_active" => Ok(ResolvedField::new("is_active", ColumnKind::Bool)),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on catalog_items: '{other}'"
            )))
        }
    }
}

/// Appends a recursive `FilterGroup` (`AND`/`OR`) to an active `QueryBuilder` using the `catalog_items` whitelist.
pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_catalog_item_field)
}

/// Appends validated `ORDER BY` clauses to the query builder (defaults to `created_at DESC`).
pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(qb, order_by, "created_at DESC", &resolve_catalog_item_field)
}
