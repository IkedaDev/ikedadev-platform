use crate::domain::repositories::EventRepository;
use ikeda_core::error::AppError;
use std::sync::Arc;
use uuid::Uuid;

/// Application use case: soft-deletes an event by ID.
pub struct DeleteEventUseCase {
    event_repo: Arc<dyn EventRepository>,
}

impl DeleteEventUseCase {
    /// Constructs a new `DeleteEventUseCase`.
    pub fn new(event_repo: Arc<dyn EventRepository>) -> Self {
        Self { event_repo }
    }

    /// Executes event soft-deletion.
    pub async fn execute(&self, tenant_id: Uuid, event_id: Uuid) -> Result<(), AppError> {
        let deleted = self.event_repo.soft_delete(tenant_id, event_id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Event with ID '{event_id}' not found"
            )));
        }
        Ok(())
    }
}
