use crate::domain::models::{EventException, TimeRange};
use crate::domain::repositories::EventRepository;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request command for creating an occurrence exception on a recurring event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateEventExceptionCommand {
    /// The original occurrence start timestamp in UTC to cancel or reschedule.
    pub original_starts_at: DateTime<Utc>,
    /// Whether this specific occurrence is cancelled completely.
    pub is_cancelled: bool,
    /// If rescheduled, the new occurrence start timestamp in UTC.
    pub rescheduled_starts_at: Option<DateTime<Utc>>,
    /// If rescheduled, the new occurrence end timestamp in UTC.
    pub rescheduled_ends_at: Option<DateTime<Utc>>,
}

/// Application use case: creates an exception overriding a specific occurrence of a recurring event.
pub struct CreateEventExceptionUseCase {
    event_repo: Arc<dyn EventRepository>,
}

impl CreateEventExceptionUseCase {
    /// Constructs a new `CreateEventExceptionUseCase`.
    pub fn new(event_repo: Arc<dyn EventRepository>) -> Self {
        Self { event_repo }
    }

    /// Validates recurrence and persists the event exception.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
        cmd: CreateEventExceptionCommand,
    ) -> Result<EventException, AppError> {
        let event = self
            .event_repo
            .find_by_id(tenant_id, event_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event with ID '{event_id}' not found")))?;

        if !event.is_recurring() {
            return Err(AppError::BadRequest(
                "Cannot create occurrence exception for a non-recurring event".to_string(),
            ));
        }

        let exception = if cmd.is_cancelled {
            EventException::cancel(tenant_id, event_id, cmd.original_starts_at)
        } else {
            let starts_at = cmd.rescheduled_starts_at.ok_or_else(|| {
                AppError::BadRequest(
                    "rescheduled_starts_at is required when is_cancelled is false".to_string(),
                )
            })?;
            let ends_at = cmd.rescheduled_ends_at.ok_or_else(|| {
                AppError::BadRequest(
                    "rescheduled_ends_at is required when is_cancelled is false".to_string(),
                )
            })?;
            let range = TimeRange::new(starts_at, ends_at)?;
            EventException::reschedule(tenant_id, event_id, cmd.original_starts_at, range)
        };

        self.event_repo.add_exception(tenant_id, &exception).await
    }
}
