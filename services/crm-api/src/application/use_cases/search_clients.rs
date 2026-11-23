use crate::domain::models::Client;
use crate::domain::repositories::ClientRepository;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::PaginatedApiResponse;
use std::sync::Arc;
use uuid::Uuid;

/// Application use case: paginated dynamic search on CRM clients using recursive `Criteria`.
pub struct SearchClientsUseCase {
    client_repo: Arc<dyn ClientRepository>,
}

impl SearchClientsUseCase {
    /// Constructs a new `SearchClientsUseCase`.
    pub fn new(client_repo: Arc<dyn ClientRepository>) -> Self {
        Self { client_repo }
    }

    /// Executes criteria search and wraps results in a standard `PaginatedApiResponse`.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        criteria: Criteria,
    ) -> Result<PaginatedApiResponse<Client>, AppError> {
        let pagination = criteria.pagination.unwrap_or_default();
        let page = pagination.page.max(1);
        let limit = pagination.sql_limit();

        let result = self.client_repo.search(tenant_id, &criteria).await?;

        Ok(PaginatedApiResponse::new(
            result.items,
            page,
            limit,
            result.total_items,
        ))
    }
}
