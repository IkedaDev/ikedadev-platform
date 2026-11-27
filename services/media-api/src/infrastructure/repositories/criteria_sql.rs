use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    resolve_metadata_field, ColumnKind, FilterGroup, OrderBy, ResolvedField,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

/// Strict whitelist of filterable/sortable columns on `media_assets` (plus `metadata.<key>`).
fn resolve_media_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "context" => Ok(ResolvedField::new("context", ColumnKind::Text)),
        "status" => Ok(ResolvedField::new("status", ColumnKind::Text)),
        "storage_provider" => Ok(ResolvedField::new("storage_provider", ColumnKind::Text)),
        "file_name" => Ok(ResolvedField::new("file_name", ColumnKind::Text)),
        "mime_type" => Ok(ResolvedField::new("mime_type", ColumnKind::Text)),
        "size_bytes" => Ok(ResolvedField::new("size_bytes", ColumnKind::Int64)),
        "alt_text" => Ok(ResolvedField::new("alt_text", ColumnKind::Text)),
        "uploaded_by" => Ok(ResolvedField::new("uploaded_by", ColumnKind::Uuid)),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on media_assets: '{other}'"
            )))
        }
    }
}

pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_media_field)
}

pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(qb, order_by, "created_at DESC", &resolve_media_field)
}
