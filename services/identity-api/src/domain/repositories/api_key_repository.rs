use crate::domain::models::{ApiKeyRecord, ApiKeySummary};
use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Port defining RLS-scoped persistence operations for per-tenant `api_keys`.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Persists a newly hashed API key record.
    async fn create(
        &self,
        tenant_id: Uuid,
        record: &ApiKeyRecord,
    ) -> Result<ApiKeySummary, AppError>;

    /// Lists all API keys belonging to the tenant (returning only safe metadata & prefix).
    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError>;

    /// Revokes an API key by ID (`is_active = false, revoked_at = NOW()`).
    async fn revoke(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;
}
