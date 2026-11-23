use crate::domain::models::Tenant;
use crate::domain::repositories::TenantRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct TenantRow {
    id: Uuid,
    slug: String,
    name: String,
    status: String,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<TenantRow> for Tenant {
    fn from(r: TenantRow) -> Self {
        Self {
            id: r.id,
            slug: r.slug,
            name: r.name,
            status: r.status,
            config: r.config,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

/// PostgreSQL implementation of `TenantRepository` (platform-level registry).
pub struct SqlxTenantRepository {
    pool: PgPool,
}

impl SqlxTenantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantRepository for SqlxTenantRepository {
    async fn create(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
        let row = sqlx::query_as::<_, TenantRow>(
            r#"
            INSERT INTO tenants (id, slug, name, status, config, created_at, updated_at, deleted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, slug, name, status, config, created_at, updated_at, deleted_at
            "#,
        )
        .bind(tenant.id)
        .bind(&tenant.slug)
        .bind(&tenant.name)
        .bind(&tenant.status)
        .bind(&tenant.config)
        .bind(tenant.created_at)
        .bind(tenant.updated_at)
        .bind(tenant.deleted_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict(format!(
                        "Tenant with slug '{}' already exists",
                        tenant.slug
                    ));
                }
            }
            AppError::DatabaseError(format!("Failed to insert tenant: {e}"))
        })?;

        Ok(row.into())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Tenant>, AppError> {
        let row = sqlx::query_as::<_, TenantRow>(
            r#"
            SELECT id, slug, name, status, config, created_at, updated_at, deleted_at
            FROM tenants
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find tenant by ID: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Tenant>, AppError> {
        let row = sqlx::query_as::<_, TenantRow>(
            r#"
            SELECT id, slug, name, status, config, created_at, updated_at, deleted_at
            FROM tenants
            WHERE slug = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find tenant by slug: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn update(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
        let row = sqlx::query_as::<_, TenantRow>(
            r#"
            UPDATE tenants
            SET name = $1, status = $2, config = $3, updated_at = NOW()
            WHERE id = $4 AND deleted_at IS NULL
            RETURNING id, slug, name, status, config, created_at, updated_at, deleted_at
            "#,
        )
        .bind(&tenant.name)
        .bind(&tenant.status)
        .bind(&tenant.config)
        .bind(tenant.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to update tenant: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("Tenant '{}' not found", tenant.id)))?;

        Ok(row.into())
    }
}
