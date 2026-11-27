use crate::domain::models::{Event, EventException, TimeRange};
use crate::domain::services::{detect_overlaps, expand_recurrence};
use async_trait::async_trait;
use chrono::Duration;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use std::collections::HashMap;
use uuid::Uuid;

/// Bounded horizon (days) used to check future recurring event occurrences for overlap.
pub const RECURRING_OVERLAP_HORIZON_DAYS: i64 = 90;

/// Port defining persistence operations for `Event` and `EventException` entities.
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Persists a new event.
    async fn create(&self, tenant_id: Uuid, event: &Event) -> Result<Event, AppError>;

    /// Finds an event by its unique identifier within the tenant.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, AppError>;

    /// Finds events that overlap with a time range on a specific calendar.
    async fn find_in_range(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        range: &TimeRange,
    ) -> Result<Vec<Event>, AppError>;

    /// Queries events matching dynamic criteria filters.
    async fn find_by_criteria(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        criteria: &Criteria,
    ) -> Result<Vec<Event>, AppError>;

    /// Retrieves all exceptions registered for a recurring event.
    async fn find_exceptions(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<EventException>, AppError>;

    /// Batch-loads exceptions for a set of recurring events (eliminates N+1 queries).
    async fn find_exceptions_batch(
        &self,
        tenant_id: Uuid,
        event_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<EventException>>, AppError> {
        let mut map = HashMap::new();
        for &event_id in event_ids {
            let exs = self.find_exceptions(tenant_id, event_id).await?;
            if !exs.is_empty() {
                map.insert(event_id, exs);
            }
        }
        Ok(map)
    }

    /// Adds an exception (cancellation or rescheduling) to a recurring event.
    async fn add_exception(
        &self,
        tenant_id: Uuid,
        exception: &EventException,
    ) -> Result<EventException, AppError>;

    /// Updates existing event details.
    async fn update(&self, tenant_id: Uuid, event: &Event) -> Result<Event, AppError>;

    /// Soft-deletes an event by ID.
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;

    /// Atomically checks idempotency, enforces strict overlap rules (when `allow_overlaps == false`),
    /// and persists a new event. Implementations backed by PostgreSQL acquire a per-calendar
    /// `pg_advisory_xact_lock` inside a single transaction.
    async fn create_checked(
        &self,
        tenant_id: Uuid,
        event: &Event,
        allow_overlaps: bool,
    ) -> Result<Event, AppError> {
        if !allow_overlaps {
            let candidate_windows = candidate_overlap_windows(event)?;
            let query_span = bounding_time_range(&candidate_windows, event.time_range)?;
            let existing_events = self
                .find_in_range(tenant_id, event.calendar_id, &query_span)
                .await?;

            // Idempotent retry check BEFORE overlap detection
            if let Some(ref ext_id) = event.external_resource_id {
                if let Some(existing) = existing_events
                    .iter()
                    .find(|e| e.external_resource_id.as_deref() == Some(ext_id.as_str()))
                {
                    return Ok(existing.clone());
                }
            }

            let recurring_ids: Vec<Uuid> = existing_events
                .iter()
                .filter(|e| e.is_recurring())
                .map(|e| e.id)
                .collect();
            let exceptions_map = self
                .find_exceptions_batch(tenant_id, &recurring_ids)
                .await?;

            let mut all_instances = Vec::new();
            for existing in &existing_events {
                let exceptions = exceptions_map
                    .get(&existing.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let instances = expand_recurrence(existing, &query_span, exceptions)?;
                all_instances.extend(instances);
            }

            for window in &candidate_windows {
                if detect_overlaps(&all_instances, window) {
                    return Err(AppError::Conflict(
                        "Event time window conflicts with an existing reservation on this calendar"
                            .to_string(),
                    ));
                }
            }
        }

        self.create(tenant_id, event).await
    }

    /// Atomically enforces strict overlap rules (excluding `event.id`) and updates an event.
    async fn update_checked(
        &self,
        tenant_id: Uuid,
        event: &Event,
        allow_overlaps: bool,
    ) -> Result<Event, AppError> {
        if !allow_overlaps {
            let candidate_windows = candidate_overlap_windows(event)?;
            let query_span = bounding_time_range(&candidate_windows, event.time_range)?;
            let existing_events = self
                .find_in_range(tenant_id, event.calendar_id, &query_span)
                .await?;

            let recurring_ids: Vec<Uuid> = existing_events
                .iter()
                .filter(|e| e.id != event.id && e.is_recurring())
                .map(|e| e.id)
                .collect();
            let exceptions_map = self
                .find_exceptions_batch(tenant_id, &recurring_ids)
                .await?;

            let mut all_instances = Vec::new();
            for existing in &existing_events {
                if existing.id == event.id {
                    continue;
                }
                let exceptions = exceptions_map
                    .get(&existing.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let instances = expand_recurrence(existing, &query_span, exceptions)?;
                all_instances.extend(instances);
            }

            for window in &candidate_windows {
                if detect_overlaps(&all_instances, window) {
                    return Err(AppError::Conflict(
                        "Event overlaps with an existing reservation on this calendar".to_string(),
                    ));
                }
            }
        }

        self.update(tenant_id, event).await
    }

    /// Atomically enforces strict overlap rules when rescheduling an occurrence and inserts the exception.
    async fn add_exception_checked(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        exception: &EventException,
        allow_overlaps: bool,
    ) -> Result<EventException, AppError> {
        if !allow_overlaps && !exception.is_cancelled {
            if let Some(rescheduled) = exception.rescheduled_time_range {
                let existing_events = self
                    .find_in_range(tenant_id, calendar_id, &rescheduled)
                    .await?;
                let recurring_ids: Vec<Uuid> = existing_events
                    .iter()
                    .filter(|e| e.is_recurring())
                    .map(|e| e.id)
                    .collect();
                let exceptions_map = self
                    .find_exceptions_batch(tenant_id, &recurring_ids)
                    .await?;

                let mut all_instances = Vec::new();
                for existing in &existing_events {
                    let mut exceptions = exceptions_map
                        .get(&existing.id)
                        .cloned()
                        .unwrap_or_default();
                    if existing.id == exception.event_id {
                        exceptions.push(EventException::cancel(
                            tenant_id,
                            exception.event_id,
                            exception.original_starts_at,
                        ));
                    }
                    let instances = expand_recurrence(existing, &rescheduled, &exceptions)?;
                    all_instances.extend(instances);
                }

                if detect_overlaps(&all_instances, &rescheduled) {
                    return Err(AppError::Conflict(
                        "Rescheduled occurrence overlaps with an existing reservation on this calendar"
                            .to_string(),
                    ));
                }
            }
        }

        self.add_exception(tenant_id, exception).await
    }
}

/// Computes the candidate time windows to check for overlap (single window for one-off events,
/// or expanded occurrences over a 90-day horizon for recurring events).
pub fn candidate_overlap_windows(event: &Event) -> Result<Vec<TimeRange>, AppError> {
    if !event.is_recurring() {
        return Ok(vec![event.time_range]);
    }
    let horizon_end = event.time_range.starts_at + Duration::days(RECURRING_OVERLAP_HORIZON_DAYS);
    let horizon = TimeRange::new(
        event.time_range.starts_at,
        horizon_end.max(event.time_range.ends_at),
    )?;
    let instances = expand_recurrence(event, &horizon, &[])?;
    if instances.is_empty() {
        return Ok(vec![event.time_range]);
    }
    Ok(instances.into_iter().map(|i| i.time_range).collect())
}

/// Computes the bounding `TimeRange` covering all candidate windows.
pub fn bounding_time_range(
    windows: &[TimeRange],
    fallback: TimeRange,
) -> Result<TimeRange, AppError> {
    let mut min_start = fallback.starts_at;
    let mut max_end = fallback.ends_at;
    for w in windows {
        if w.starts_at < min_start {
            min_start = w.starts_at;
        }
        if w.ends_at > max_end {
            max_end = w.ends_at;
        }
    }
    TimeRange::new(min_start, max_end)
}
