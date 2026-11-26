use async_trait::async_trait;
use ikeda_core::error::AppError;
use ikeda_core::tenant::{ApiKeyValidator, AuthenticatedKey};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct AuthenticateApiKeyRow {
    tenant_id: Uuid,
    scopes: Vec<String>,
}

/// SQLx-backed `ApiKeyValidator` invoking `authenticate_api_key($1)`.
pub struct SqlxApiKeyValidator {
    pool: PgPool,
}

impl SqlxApiKeyValidator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyValidator for SqlxApiKeyValidator {
    async fn validate_key_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        let row = sqlx::query_as::<_, AuthenticateApiKeyRow>(
            r#"
            SELECT tenant_id, scopes
            FROM authenticate_api_key($1)
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to authenticate API key: {e}")))?;

        Ok(row.map(|r| AuthenticatedKey {
            tenant_id: r.tenant_id,
            scopes: r.scopes,
        }))
    }
}
