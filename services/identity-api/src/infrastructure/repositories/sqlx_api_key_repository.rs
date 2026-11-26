use crate::domain::models::{ApiKeyRecord, ApiKeySummary};
use crate::domain::repositories::ApiKeyRepository;
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct ApiKeySummaryRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    key_prefix: String,
    scopes: Vec<String>,
    is_active: bool,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl From<ApiKeySummaryRow> for ApiKeySummary {
    fn from(r: ApiKeySummaryRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            key_prefix: r.key_prefix,
            scopes: r.scopes,
            is_active: r.is_active,
            expires_at: r.expires_at,
            last_used_at: r.last_used_at,
            created_at: r.created_at,
            revoked_at: r.revoked_at,
        }
    }
}

/// PostgreSQL + RLS implementation of `ApiKeyRepository`.
pub struct SqlxApiKeyRepository {
    pool: PgPool,
}

impl SqlxApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for SqlxApiKeyRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        record: &ApiKeyRecord,
    ) -> Result<ApiKeySummary, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ApiKeySummaryRow>(
            r#"
            INSERT INTO api_keys (
                id, tenant_id, name, key_prefix, key_hash, scopes,
                is_active, expires_at, last_used_at, created_at, revoked_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, tenant_id, name, key_prefix, scopes, is_active,
                      expires_at, last_used_at, created_at, revoked_at
            "#,
        )
        .bind(record.id)
        .bind(tenant_id)
        .bind(&record.name)
        .bind(&record.key_prefix)
        .bind(&record.key_hash)
        .bind(&record.scopes)
        .bind(record.is_active)
        .bind(record.expires_at)
        .bind(record.last_used_at)
        .bind(record.created_at)
        .bind(record.revoked_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to insert API key: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let rows = sqlx::query_as::<_, ApiKeySummaryRow>(
            r#"
            SELECT id, tenant_id, name, key_prefix, scopes, is_active,
                   expires_at, last_used_at, created_at, revoked_at
            FROM api_keys
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to list API keys: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn revoke(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET is_active = false, revoked_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to revoke API key: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }
}
