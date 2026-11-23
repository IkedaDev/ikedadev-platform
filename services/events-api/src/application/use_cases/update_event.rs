use crate::domain::models::{Event, EventStatus, TimeRange};
use crate::domain::repositories::{CalendarRepository, EventRepository};
use crate::domain::services::{detect_overlaps, expand_recurrence};
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request command for updating an existing event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateEventCommand {
    /// Optional updated title.
    pub title: Option<String>,
    /// Optional updated description.
    pub description: Option<String>,
    /// Optional updated start timestamp in UTC.
    pub starts_at: Option<DateTime<Utc>>,
    /// Optional updated end timestamp in UTC.
    pub ends_at: Option<DateTime<Utc>>,
    /// Optional updated confirmation status.
    pub status: Option<EventStatus>,
    /// Optional updated JSONB metadata payload.
    pub metadata: Option<serde_json::Value>,
}

/// Application use case: updates an existing event, enforcing overlap validation
/// while excluding the event itself from collision detection.
pub struct UpdateEventUseCase {
    calendar_repo: Arc<dyn CalendarRepository>,
    event_repo: Arc<dyn EventRepository>,
}

impl UpdateEventUseCase {
    /// Constructs a new `UpdateEventUseCase`.
    pub fn new(
        calendar_repo: Arc<dyn CalendarRepository>,
        event_repo: Arc<dyn EventRepository>,
    ) -> Self {
        Self {
            calendar_repo,
            event_repo,
        }
    }

    /// Executes the event update and overlap validation.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
        cmd: UpdateEventCommand,
    ) -> Result<Event, AppError> {
        let mut event = self
            .event_repo
            .find_by_id(tenant_id, event_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event with ID '{event_id}' not found")))?;

        if let Some(title) = cmd.title {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "Event title cannot be empty".to_string(),
                ));
            }
            event.title = trimmed.to_string();
        }

        if let Some(description) = cmd.description {
            event.description = Some(description);
        }

        let starts_at = cmd.starts_at.unwrap_or(event.time_range.starts_at);
        let ends_at = cmd.ends_at.unwrap_or(event.time_range.ends_at);
        let new_time_range = TimeRange::new(starts_at, ends_at)?;

        // Verify calendar overlap rules if strict
        let calendar = self
            .calendar_repo
            .find_by_id(tenant_id, event.calendar_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Calendar '{}' not found", event.calendar_id))
            })?;

        if !calendar.allow_overlaps {
            let existing_events = self
                .event_repo
                .find_in_range(tenant_id, calendar.id, &new_time_range)
                .await?;

            let mut all_instances = Vec::new();
            for existing in &existing_events {
                // Critical: exclude the event currently being updated from collision detection
                if existing.id == event_id {
                    continue;
                }

                let exceptions = if existing.is_recurring() {
                    self.event_repo
                        .find_exceptions(tenant_id, existing.id)
                        .await?
                } else {
                    Vec::new()
                };

                let instances = expand_recurrence(existing, &new_time_range, &exceptions)?;
                all_instances.extend(instances);
            }

            if detect_overlaps(&all_instances, &new_time_range) {
                return Err(AppError::Conflict(
                    "Event overlaps with an existing reservation on this calendar".to_string(),
                ));
            }
        }

        event.time_range = new_time_range;

        if let Some(status) = cmd.status {
            event.status = status;
        }

        if let Some(metadata) = cmd.metadata {
            event.metadata = metadata;
        }

        self.event_repo.update(tenant_id, &event).await
    }
}
