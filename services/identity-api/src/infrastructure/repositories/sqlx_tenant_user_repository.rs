use crate::domain::models::TenantUser;
use crate::domain::repositories::TenantUserRepository;
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct TenantUserRow {
    id: Uuid,
    tenant_id: Uuid,
    keycloak_user_id: Uuid,
    email: String,
    full_name: String,
    roles: Vec<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<TenantUserRow> for TenantUser {
    fn from(r: TenantUserRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            keycloak_user_id: r.keycloak_user_id,
            email: r.email,
            full_name: r.full_name,
            roles: r.roles,
            is_active: r.is_active,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

/// PostgreSQL + RLS implementation of `TenantUserRepository`.
pub struct SqlxTenantUserRepository {
    pool: PgPool,
}

impl SqlxTenantUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantUserRepository for SqlxTenantUserRepository {
    async fn create(&self, tenant_id: Uuid, user: &TenantUser) -> Result<TenantUser, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, TenantUserRow>(
            r#"
            INSERT INTO tenant_users (
                id, tenant_id, keycloak_user_id, email, full_name, roles,
                is_active, created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, tenant_id, keycloak_user_id, email, full_name, roles,
                      is_active, created_at, updated_at, deleted_at
            "#,
        )
        .bind(user.id)
        .bind(tenant_id)
        .bind(user.keycloak_user_id)
        .bind(&user.email)
        .bind(&user.full_name)
        .bind(&user.roles)
        .bind(user.is_active)
        .bind(user.created_at)
        .bind(user.updated_at)
        .bind(user.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict(format!(
                        "A user with email '{}' already exists in this tenant",
                        user.email
                    ));
                }
            }
            AppError::DatabaseError(format!("Failed to insert tenant user: {e}"))
        })?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenantUser>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, TenantUserRow>(
            r#"
            SELECT id, tenant_id, keycloak_user_id, email, full_name, roles,
                   is_active, created_at, updated_at, deleted_at
            FROM tenant_users
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find tenant user: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let rows = sqlx::query_as::<_, TenantUserRow>(
            r#"
            SELECT id, tenant_id, keycloak_user_id, email, full_name, roles,
                   is_active, created_at, updated_at, deleted_at
            FROM tenant_users
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to list tenant users: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let result = sqlx::query(
            r#"
            UPDATE tenant_users
            SET is_active = false, deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete tenant user: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }
}
