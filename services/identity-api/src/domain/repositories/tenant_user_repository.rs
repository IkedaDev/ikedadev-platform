use crate::domain::models::TenantUser;
use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Port defining RLS-scoped persistence operations for `TenantUser` entities.
#[async_trait]
pub trait TenantUserRepository: Send + Sync {
    /// Persists a newly provisioned user under the tenant.
    async fn create(&self, tenant_id: Uuid, user: &TenantUser) -> Result<TenantUser, AppError>;

    /// Finds an active user by internal ID within the tenant.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenantUser>, AppError>;

    /// Lists all active users belonging to the tenant.
    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError>;

    /// Soft-deletes a user (`deleted_at = NOW()`, `is_active = false`).
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;
}
