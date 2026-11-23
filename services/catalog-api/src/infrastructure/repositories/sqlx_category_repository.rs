use crate::domain::models::Category;
use crate::domain::repositories::CategoryRepository;
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct CategoryRow {
    id: Uuid,
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    slug: String,
    description: Option<String>,
    display_order: i32,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<CategoryRow> for Category {
    fn from(r: CategoryRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            parent_id: r.parent_id,
            name: r.name,
            slug: r.slug,
            description: r.description,
            display_order: r.display_order,
            is_active: r.is_active,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

/// PostgreSQL + RLS implementation of `CategoryRepository`.
pub struct SqlxCategoryRepository {
    pool: PgPool,
}

impl SqlxCategoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_category_write_err(ctx: &str, err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = err {
        if db_err.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "A category with this slug already exists in this tenant".to_string(),
            );
        }
    }
    AppError::DatabaseError(format!("{ctx}: {err}"))
}

#[async_trait]
impl CategoryRepository for SqlxCategoryRepository {
    async fn create(&self, tenant_id: Uuid, category: &Category) -> Result<Category, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            INSERT INTO categories (
                id, tenant_id, parent_id, name, slug, description,
                display_order, is_active, created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, tenant_id, parent_id, name, slug, description,
                      display_order, is_active, created_at, updated_at, deleted_at
            "#,
        )
        .bind(category.id)
        .bind(tenant_id)
        .bind(category.parent_id)
        .bind(&category.name)
        .bind(&category.slug)
        .bind(&category.description)
        .bind(category.display_order)
        .bind(category.is_active)
        .bind(category.created_at)
        .bind(category.updated_at)
        .bind(category.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_category_write_err("Failed to insert category", e))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Category>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            SELECT id, tenant_id, parent_id, name, slug, description,
                   display_order, is_active, created_at, updated_at, deleted_at
            FROM categories
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find category: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_slug(
        &self,
        tenant_id: Uuid,
        slug: &str,
    ) -> Result<Option<Category>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            SELECT id, tenant_id, parent_id, name, slug, description,
                   display_order, is_active, created_at, updated_at, deleted_at
            FROM categories
            WHERE slug = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(slug)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find category by slug: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Category>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let rows = sqlx::query_as::<_, CategoryRow>(
            r#"
            SELECT id, tenant_id, parent_id, name, slug, description,
                   display_order, is_active, created_at, updated_at, deleted_at
            FROM categories
            WHERE deleted_at IS NULL
            ORDER BY display_order ASC, name ASC
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to list categories: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn update(&self, tenant_id: Uuid, category: &Category) -> Result<Category, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CategoryRow>(
            r#"
            UPDATE categories
            SET parent_id = $1, name = $2, slug = $3, description = $4,
                display_order = $5, is_active = $6, updated_at = NOW()
            WHERE id = $7 AND deleted_at IS NULL
            RETURNING id, tenant_id, parent_id, name, slug, description,
                      display_order, is_active, created_at, updated_at, deleted_at
            "#,
        )
        .bind(category.parent_id)
        .bind(&category.name)
        .bind(&category.slug)
        .bind(&category.description)
        .bind(category.display_order)
        .bind(category.is_active)
        .bind(category.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_category_write_err("Failed to update category", e))?
        .ok_or_else(|| AppError::NotFound(format!("Category '{}' not found", category.id)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let res = sqlx::query(
            r#"
            UPDATE categories
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete category: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(res.rows_affected() > 0)
    }
}
