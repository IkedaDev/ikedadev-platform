use crate::domain::models::Calendar;
use crate::domain::repositories::CalendarRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request command for updating an existing calendar.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCalendarCommand {
    /// Optional updated calendar display name.
    pub name: Option<String>,
    /// Optional updated primary timezone identifier (e.g. "America/Santiago").
    pub timezone: Option<String>,
    /// Optional updated flag indicating whether concurrent overlapping bookings are permitted.
    pub allow_overlaps: Option<bool>,
    /// Optional updated weekly operating hours specification.
    pub working_hours: Option<serde_json::Value>,
}

/// Application use case: updates an existing calendar's configuration.
pub struct UpdateCalendarUseCase {
    calendar_repo: Arc<dyn CalendarRepository>,
}

impl UpdateCalendarUseCase {
    /// Constructs a new `UpdateCalendarUseCase`.
    pub fn new(calendar_repo: Arc<dyn CalendarRepository>) -> Self {
        Self { calendar_repo }
    }

    /// Executes the calendar update, validating timezone and constraints.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        cmd: UpdateCalendarCommand,
    ) -> Result<Calendar, AppError> {
        let mut calendar = self
            .calendar_repo
            .find_by_id(tenant_id, calendar_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Calendar '{calendar_id}' not found")))?;

        if let Some(name) = cmd.name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "Calendar name cannot be empty".to_string(),
                ));
            }
            calendar.name = trimmed.to_string();
        }

        if let Some(tz_str) = cmd.timezone {
            let trimmed = tz_str.trim();
            trimmed.parse::<chrono_tz::Tz>().map_err(|_| {
                AppError::BadRequest(format!("Invalid IANA timezone identifier: '{trimmed}'"))
            })?;
            calendar.timezone = trimmed.to_string();
        }

        if let Some(allow_overlaps) = cmd.allow_overlaps {
            calendar.allow_overlaps = allow_overlaps;
        }

        if let Some(working_hours) = cmd.working_hours {
            calendar.working_hours = Some(working_hours);
        }

        self.calendar_repo.update(tenant_id, &calendar).await
    }
}
