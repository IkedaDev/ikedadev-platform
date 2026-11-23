use crate::domain::models::{ApiKeyRecord, ApiKeySummary, CreatedApiKey};
use crate::domain::repositories::ApiKeyRepository;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use ikeda_core::tenant::hash_api_key;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for minting a new per-tenant M2M API key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateApiKeyCommand {
    /// Human-readable label (e.g. `"Anami BFF Production"`).
    pub name: String,
    /// Granted scopes (e.g. `["events:write", "crm:read"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Optional expiration timestamp in UTC.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Use case: generates a new cryptographically random API key, persists only its SHA-256 hash,
/// and returns the plaintext secret once.
pub struct CreateApiKeyUseCase {
    api_key_repo: Arc<dyn ApiKeyRepository>,
}

impl CreateApiKeyUseCase {
    pub fn new(api_key_repo: Arc<dyn ApiKeyRepository>) -> Self {
        Self { api_key_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: CreateApiKeyCommand,
    ) -> Result<CreatedApiKey, AppError> {
        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "API key 'name' cannot be empty".to_string(),
            ));
        }

        if let Some(exp) = cmd.expires_at {
            if exp <= Utc::now() {
                return Err(AppError::BadRequest(
                    "API key 'expires_at' must be in the future".to_string(),
                ));
            }
        }

        // Generate 256 bits of random entropy via two UUIDv4 values
        let raw_key = format!(
            "ik_live_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let key_prefix: String = raw_key.chars().take(16).collect();
        let key_hash = hash_api_key(&raw_key);

        let record = ApiKeyRecord {
            id: Uuid::new_v4(),
            tenant_id,
            name,
            key_prefix,
            key_hash,
            scopes: cmd.scopes,
            is_active: true,
            expires_at: cmd.expires_at,
            last_used_at: None,
            created_at: Utc::now(),
            revoked_at: None,
        };

        let summary = self.api_key_repo.create(tenant_id, &record).await?;
        Ok(CreatedApiKey {
            key: summary,
            raw_key,
        })
    }
}

/// Use case: lists all API keys for a tenant (prefix and metadata only).
pub struct ListApiKeysUseCase {
    api_key_repo: Arc<dyn ApiKeyRepository>,
}

impl ListApiKeysUseCase {
    pub fn new(api_key_repo: Arc<dyn ApiKeyRepository>) -> Self {
        Self { api_key_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError> {
        self.api_key_repo.find_by_tenant(tenant_id).await
    }
}

/// Use case: revokes an API key by ID within the tenant.
pub struct RevokeApiKeyUseCase {
    api_key_repo: Arc<dyn ApiKeyRepository>,
}

impl RevokeApiKeyUseCase {
    pub fn new(api_key_repo: Arc<dyn ApiKeyRepository>) -> Self {
        Self { api_key_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, key_id: Uuid) -> Result<(), AppError> {
        let revoked = self.api_key_repo.revoke(tenant_id, key_id).await?;
        if !revoked {
            return Err(AppError::NotFound(format!(
                "API key with ID '{key_id}' not found or already revoked"
            )));
        }
        Ok(())
    }
}
