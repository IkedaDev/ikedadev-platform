use crate::domain::models::Client;
use async_trait::async_trait;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Paginated search result returned by `ClientRepository::search`.
#[derive(Debug, Clone)]
pub struct ClientSearchPage {
    /// Clients matching the current page.
    pub items: Vec<Client>,
    /// Total matching clients across all pages.
    pub total_items: u64,
}

/// Port defining persistence operations for `Client` entities.
#[async_trait]
pub trait ClientRepository: Send + Sync {
    /// Persists a new client within the tenant context.
    async fn create(&self, tenant_id: Uuid, client: &Client) -> Result<Client, AppError>;

    /// Finds an active client by ID within the tenant.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Client>, AppError>;

    /// Finds an active client by email within the tenant.
    async fn find_by_email(&self, tenant_id: Uuid, email: &str)
        -> Result<Option<Client>, AppError>;

    /// Finds an active client by tax ID within the tenant.
    async fn find_by_tax_id(
        &self,
        tenant_id: Uuid,
        tax_id: &str,
    ) -> Result<Option<Client>, AppError>;

    /// Updates an existing active client.
    async fn update(&self, tenant_id: Uuid, client: &Client) -> Result<Client, AppError>;

    /// Soft-deletes an active client by ID (`deleted_at = NOW()`).
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;

    /// Searches active clients using recursive `Criteria` (filters, ordering, pagination).
    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<ClientSearchPage, AppError>;
}
