use crate::domain::repositories::ClientRepository;
use ikeda_core::error::AppError;
use std::sync::Arc;
use uuid::Uuid;

/// Application use case: soft-deletes a client by ID.
pub struct DeleteClientUseCase {
    client_repo: Arc<dyn ClientRepository>,
}

impl DeleteClientUseCase {
    /// Constructs a new `DeleteClientUseCase`.
    pub fn new(client_repo: Arc<dyn ClientRepository>) -> Self {
        Self { client_repo }
    }

    /// Executes client soft deletion.
    pub async fn execute(&self, tenant_id: Uuid, client_id: Uuid) -> Result<(), AppError> {
        let deleted = self.client_repo.soft_delete(tenant_id, client_id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Client with ID '{client_id}' not found"
            )));
        }
        Ok(())
    }
}
