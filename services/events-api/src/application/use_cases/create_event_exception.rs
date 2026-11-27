use crate::domain::models::{EventException, TimeRange};
use crate::domain::repositories::{CalendarRepository, EventRepository};
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
    calendar_repo: Option<Arc<dyn CalendarRepository>>,
    event_repo: Arc<dyn EventRepository>,
}

impl CreateEventExceptionUseCase {
    /// Constructs a new `CreateEventExceptionUseCase`.
    pub fn new(event_repo: Arc<dyn EventRepository>) -> Self {
        Self {
            calendar_repo: None,
            event_repo,
        }
    }

    /// Attaches a `CalendarRepository` so rescheduled occurrences enforce `allow_overlaps = false`.
    #[must_use]
    pub fn with_calendar_repo(mut self, calendar_repo: Arc<dyn CalendarRepository>) -> Self {
        self.calendar_repo = Some(calendar_repo);
        self
    }

    /// Validates recurrence, enforces calendar overlap rules on reschedule, and persists the event exception.
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

        let allow_overlaps = if let Some(ref cal_repo) = self.calendar_repo {
            cal_repo
                .find_by_id(tenant_id, event.calendar_id)
                .await?
                .map(|c| c.allow_overlaps)
                .unwrap_or(true)
        } else {
            true
        };

        self.event_repo
            .add_exception_checked(tenant_id, event.calendar_id, &exception, allow_overlaps)
            .await
    }
}
