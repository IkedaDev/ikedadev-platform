use crate::domain::models::{ContentCategory, ContentTag};
use crate::domain::ports::{
    CreateCategoryData, CreateTagData, TaxonomyRepository, UpdateCategoryData,
};
use crate::infrastructure::database::{begin_tenant_tx, map_sqlx_error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct CategoryRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    slug: String,
    description: Option<String>,
    display_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<CategoryRow> for ContentCategory {
    fn from(r: CategoryRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            slug: r.slug,
            description: r.description,
            display_order: r.display_order,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct TagRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    slug: String,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<TagRow> for ContentTag {
    fn from(r: TagRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            slug: r.slug,
            created_at: r.created_at,
            deleted_at: r.deleted_at,
        }
    }
}

pub struct SqlxTaxonomyRepository {
    pool: PgPool,
}

impl SqlxTaxonomyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TaxonomyRepository for SqlxTaxonomyRepository {
    async fn create_category(&self, data: CreateCategoryData) -> Result<ContentCategory, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            INSERT INTO content_categories (tenant_id, name, slug, description, display_order)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, tenant_id, name, slug, description, display_order, created_at, updated_at, deleted_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(&data.name)
        .bind(&data.slug)
        .bind(&data.description)
        .bind(data.display_order)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to create content category"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit create_category: {e}"))
        })?;
        Ok(ContentCategory::from(row))
    }

    async fn list_categories(&self, tenant_id: Uuid) -> Result<Vec<ContentCategory>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let rows = sqlx::query_as::<_, CategoryRow>(
            r#"
            SELECT id, tenant_id, name, slug, description, display_order, created_at, updated_at, deleted_at
            FROM content_categories
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY display_order ASC, name ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list content categories"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit list_categories: {e}"))
        })?;
        Ok(rows.into_iter().map(ContentCategory::from).collect())
    }

    async fn update_category(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateCategoryData,
    ) -> Result<ContentCategory, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let existing = sqlx::query_as::<_, CategoryRow>(
            r#"
            SELECT id, tenant_id, name, slug, description, display_order, created_at, updated_at, deleted_at
            FROM content_categories
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to load content category"))?
        .ok_or_else(|| AppError::NotFound(format!("Category '{id}' not found")))?;

        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            UPDATE content_categories
            SET name = $3, slug = $4, description = $5, display_order = $6, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING id, tenant_id, name, slug, description, display_order, created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(data.name.unwrap_or(existing.name))
        .bind(data.slug.unwrap_or(existing.slug))
        .bind(data.description.unwrap_or(existing.description))
        .bind(data.display_order.unwrap_or(existing.display_order))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update content category"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit update_category: {e}"))
        })?;
        Ok(ContentCategory::from(row))
    }

    async fn soft_delete_category(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let res = sqlx::query(
            r#"
            UPDATE content_categories
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete category"))?;

        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Category '{id}' not found")));
        }

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit delete_category: {e}"))
        })?;
        Ok(())
    }

    async fn create_tag(&self, data: CreateTagData) -> Result<ContentTag, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let row = sqlx::query_as::<_, TagRow>(
            r#"
            INSERT INTO content_tags (tenant_id, name, slug)
            VALUES ($1, $2, $3)
            RETURNING id, tenant_id, name, slug, created_at, deleted_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(&data.name)
        .bind(&data.slug)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to create content tag"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit create_tag: {e}")))?;
        Ok(ContentTag::from(row))
    }

    async fn list_tags(&self, tenant_id: Uuid) -> Result<Vec<ContentTag>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let rows = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT id, tenant_id, name, slug, created_at, deleted_at
            FROM content_tags
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY name ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list content tags"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit list_tags: {e}")))?;
        Ok(rows.into_iter().map(ContentTag::from).collect())
    }

    async fn soft_delete_tag(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let res = sqlx::query(
            r#"
            UPDATE content_tags
            SET deleted_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete tag"))?;

        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Tag '{id}' not found")));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit delete_tag: {e}")))?;
        Ok(())
    }
}
