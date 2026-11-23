use crate::domain::models::Client;
use crate::domain::repositories::ClientRepository;
use ikeda_core::error::AppError;
use std::sync::Arc;
use uuid::Uuid;

/// Application use case: retrieves an active client by ID within the tenant.
pub struct GetClientUseCase {
    client_repo: Arc<dyn ClientRepository>,
}

impl GetClientUseCase {
    /// Constructs a new `GetClientUseCase`.
    pub fn new(client_repo: Arc<dyn ClientRepository>) -> Self {
        Self { client_repo }
    }

    /// Executes client lookup.
    pub async fn execute(&self, tenant_id: Uuid, client_id: Uuid) -> Result<Client, AppError> {
        self.client_repo
            .find_by_id(tenant_id, client_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Client with ID '{client_id}' not found")))
    }
}
