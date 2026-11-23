use crate::domain::repositories::CalendarRepository;
use ikeda_core::error::AppError;
use std::sync::Arc;
use uuid::Uuid;

/// Application use case: soft-deletes a calendar by ID.
pub struct DeleteCalendarUseCase {
    calendar_repo: Arc<dyn CalendarRepository>,
}

impl DeleteCalendarUseCase {
    /// Constructs a new `DeleteCalendarUseCase`.
    pub fn new(calendar_repo: Arc<dyn CalendarRepository>) -> Self {
        Self { calendar_repo }
    }

    /// Executes calendar soft-deletion.
    pub async fn execute(&self, tenant_id: Uuid, calendar_id: Uuid) -> Result<(), AppError> {
        let deleted = self
            .calendar_repo
            .soft_delete(tenant_id, calendar_id)
            .await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Calendar with ID '{calendar_id}' not found"
            )));
        }
        Ok(())
    }
}
