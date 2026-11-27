use ikeda_core::criteria::{
    append_filter_group as core_append_filter_group, append_order_by as core_append_order_by,
    resolve_metadata_field, ColumnKind, FilterGroup, OrderBy, ResolvedField,
};
use ikeda_core::error::AppError;
use sqlx::{Postgres, QueryBuilder};

/// Strict whitelist of filterable/sortable columns on `articles` (plus `metadata.<key>`).
fn resolve_article_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "category_id" => Ok(ResolvedField::new("category_id", ColumnKind::Uuid)),
        "slug" => Ok(ResolvedField::new("slug", ColumnKind::Text)),
        "title" => Ok(ResolvedField::new("title", ColumnKind::Text)),
        "status" => Ok(ResolvedField::new("status", ColumnKind::Text)),
        "published_at" => Ok(ResolvedField::new("published_at", ColumnKind::Timestamp)),
        "scheduled_for" => Ok(ResolvedField::new("scheduled_for", ColumnKind::Timestamp)),
        "author_id" => Ok(ResolvedField::new("author_id", ColumnKind::Uuid)),
        "word_count" => Ok(ResolvedField::new("word_count", ColumnKind::Int32)),
        "reading_time_minutes" => Ok(ResolvedField::new(
            "reading_time_minutes",
            ColumnKind::Int32,
        )),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported filter/sort field on articles: '{other}'"
            )))
        }
    }
}

pub fn append_filter_group<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    group: &'a FilterGroup,
) -> Result<(), AppError> {
    core_append_filter_group(qb, group, &resolve_article_field)
}

pub fn append_order_by(
    qb: &mut QueryBuilder<'_, Postgres>,
    order_by: &[OrderBy],
) -> Result<(), AppError> {
    core_append_order_by(qb, order_by, "updated_at DESC", &resolve_article_field)
}
