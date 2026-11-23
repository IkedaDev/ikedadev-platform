use crate::domain::models::Tenant;
use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Port defining persistence operations for platform `Tenant` entities.
#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Persists a new tenant.
    async fn create(&self, tenant: &Tenant) -> Result<Tenant, AppError>;

    /// Finds an active tenant by ID.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Tenant>, AppError>;

    /// Finds an active tenant by unique slug.
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Tenant>, AppError>;

    /// Updates an existing tenant's name, status, or config.
    async fn update(&self, tenant: &Tenant) -> Result<Tenant, AppError>;
}
