use crate::domain::models::{Event, EventException, EventStatus, TimeRange};
use crate::domain::repositories::{
    bounding_time_range, candidate_overlap_windows, EventRepository,
};
use crate::domain::services::{detect_overlaps, expand_recurrence};
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::sqlx_builder::{
    append_filter_group, append_order_by, resolve_metadata_field, ColumnKind, ResolvedField,
};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

fn resolve_event_field(field: &str) -> Result<ResolvedField, AppError> {
    match field.trim() {
        "id" => Ok(ResolvedField::new("id", ColumnKind::Uuid)),
        "calendar_id" => Ok(ResolvedField::new("calendar_id", ColumnKind::Uuid)),
        "title" => Ok(ResolvedField::new("title", ColumnKind::Text)),
        "description" => Ok(ResolvedField::new("description", ColumnKind::Text)),
        "starts_at" => Ok(ResolvedField::new("starts_at", ColumnKind::Timestamp)),
        "ends_at" => Ok(ResolvedField::new("ends_at", ColumnKind::Timestamp)),
        "timezone" => Ok(ResolvedField::new("timezone", ColumnKind::Text)),
        "status" => Ok(ResolvedField::new("status", ColumnKind::Text)),
        "external_resource_id" => Ok(ResolvedField::new("external_resource_id", ColumnKind::Text)),
        "created_at" => Ok(ResolvedField::new("created_at", ColumnKind::Timestamp)),
        "updated_at" => Ok(ResolvedField::new("updated_at", ColumnKind::Timestamp)),
        other => {
            if let Some(res) = resolve_metadata_field(other) {
                return res;
            }
            Err(AppError::BadRequest(format!(
                "Unsupported event search/sort field: '{other}'"
            )))
        }
    }
}

async fn lock_calendar_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    calendar_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2::text, 0))")
        .bind(tenant_id)
        .bind(calendar_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to acquire calendar lock: {e}")))?;
    Ok(())
}

async fn fetch_in_range_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    calendar_id: Uuid,
    range: &TimeRange,
) -> Result<Vec<Event>, AppError> {
    let rows = sqlx::query_as::<_, EventRow>(
        r#"
        SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at,
               timezone, rrule, status, external_resource_id, metadata,
               created_at, updated_at, deleted_at
        FROM events
        WHERE tenant_id = $1
          AND calendar_id = $2
          AND deleted_at IS NULL
          AND (
              (rrule IS NULL AND starts_at < $4 AND ends_at > $3)
              OR (rrule IS NOT NULL AND starts_at <= $4)
          )
        ORDER BY starts_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(calendar_id)
    .bind(range.starts_at)
    .bind(range.ends_at)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::DatabaseError(format!("Failed to find events in range: {e}")))?;

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        events.push(row.into_event()?);
    }
    Ok(events)
}

async fn fetch_exceptions_batch_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    event_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<EventException>>, AppError> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<_, EventExceptionRow>(
        r#"
        SELECT id, tenant_id, event_id, original_starts_at, is_cancelled,
               rescheduled_starts_at, rescheduled_ends_at, created_at
        FROM event_exceptions
        WHERE tenant_id = $1 AND event_id = ANY($2)
        ORDER BY original_starts_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(event_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::DatabaseError(format!("Failed to batch-load event exceptions: {e}")))?;

    let mut map: HashMap<Uuid, Vec<EventException>> = HashMap::new();
    for row in rows {
        let ex = row.into_exception()?;
        map.entry(ex.event_id).or_default().push(ex);
    }
    Ok(map)
}

fn map_exception_write_err(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = err {
        if db_err.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "An exception for this occurrence already exists".to_string(),
            );
        }
    }
    AppError::DatabaseError(format!("Failed to insert exception: {err}"))
}

#[derive(Debug, FromRow)]
struct EventRow {
    id: Uuid,
    tenant_id: Uuid,
    calendar_id: Uuid,
    title: String,
    description: Option<String>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    timezone: String,
    rrule: Option<String>,
    status: String,
    external_resource_id: Option<String>,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl EventRow {
    fn into_event(self) -> Result<Event, AppError> {
        let time_range = TimeRange::new(self.starts_at, self.ends_at)?;
        let status = match self.status.as_str() {
            "CONFIRMED" => EventStatus::Confirmed,
            "TENTATIVE" => EventStatus::Tentative,
            "CANCELLED" => EventStatus::Cancelled,
            _ => EventStatus::Confirmed,
        };

        Ok(Event {
            id: self.id,
            tenant_id: self.tenant_id,
            calendar_id: self.calendar_id,
            title: self.title,
            description: self.description,
            time_range,
            timezone: self.timezone,
            rrule: self.rrule,
            status,
            external_resource_id: self.external_resource_id,
            metadata: self.metadata,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct EventExceptionRow {
    id: Uuid,
    tenant_id: Uuid,
    event_id: Uuid,
    original_starts_at: DateTime<Utc>,
    is_cancelled: bool,
    rescheduled_starts_at: Option<DateTime<Utc>>,
    rescheduled_ends_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl EventExceptionRow {
    fn into_exception(self) -> Result<EventException, AppError> {
        let rescheduled_time_range = match (self.rescheduled_starts_at, self.rescheduled_ends_at) {
            (Some(s), Some(e)) => Some(TimeRange::new(s, e)?),
            _ => None,
        };

        Ok(EventException {
            id: self.id,
            tenant_id: self.tenant_id,
            event_id: self.event_id,
            original_starts_at: self.original_starts_at,
            is_cancelled: self.is_cancelled,
            rescheduled_time_range,
            created_at: self.created_at,
        })
    }
}

/// PostgreSQL implementation of `EventRepository` using SQLx and RLS.
pub struct SqlxEventRepository {
    pool: PgPool,
}

impl SqlxEventRepository {
    /// Constructs a repository backed by a PostgreSQL connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventRepository for SqlxEventRepository {
    async fn create(&self, tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        // Idempotency check: if external_resource_id is set and already exists for this tenant/calendar, return it
        if let Some(ext_id) = &event.external_resource_id {
            let existing = sqlx::query_as::<_, EventRow>(
                r#"
                SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                       timezone, rrule, status, external_resource_id, metadata,
                       created_at, updated_at, deleted_at
                FROM events
                WHERE tenant_id = $1 AND calendar_id = $2 AND external_resource_id = $3 AND deleted_at IS NULL
                "#,
            )
            .bind(tenant_id)
            .bind(event.calendar_id)
            .bind(ext_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Idempotency lookup failed: {e}")))?;

            if let Some(row) = existing {
                tx.commit().await.map_err(|e| {
                    AppError::DatabaseError(format!("Transaction commit failed: {e}"))
                })?;
                return row.into_event();
            }
        }

        let status_str = match event.status {
            EventStatus::Confirmed => "CONFIRMED",
            EventStatus::Tentative => "TENTATIVE",
            EventStatus::Cancelled => "CANCELLED",
        };

        let row = sqlx::query_as::<_, EventRow>(
            r#"
            INSERT INTO events (
                id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                timezone, rrule, status, external_resource_id, metadata,
                created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                      timezone, rrule, status, external_resource_id, metadata,
                      created_at, updated_at, deleted_at
            "#,
        )
        .bind(event.id)
        .bind(tenant_id)
        .bind(event.calendar_id)
        .bind(&event.title)
        .bind(&event.description)
        .bind(event.time_range.starts_at)
        .bind(event.time_range.ends_at)
        .bind(&event.timezone)
        .bind(&event.rrule)
        .bind(status_str)
        .bind(&event.external_resource_id)
        .bind(&event.metadata)
        .bind(event.created_at)
        .bind(event.updated_at)
        .bind(event.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to insert event: {e}")))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit event transaction: {e}"))
        })?;

        row.into_event()
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                   timezone, rrule, status, external_resource_id, metadata,
                   created_at, updated_at, deleted_at
            FROM events
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find event: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        match row {
            Some(r) => Ok(Some(r.into_event()?)),
            None => Ok(None),
        }
    }

    async fn find_in_range(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        range: &TimeRange,
    ) -> Result<Vec<Event>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        // Fetches non-recurring events that overlap, or recurring events where starts_at <= range.ends_at
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                   timezone, rrule, status, external_resource_id, metadata,
                   created_at, updated_at, deleted_at
            FROM events
            WHERE tenant_id = $1
              AND calendar_id = $2
              AND deleted_at IS NULL
              AND (
                  (rrule IS NULL AND starts_at < $4 AND ends_at > $3)
                  OR (rrule IS NOT NULL AND starts_at <= $4)
              )
            ORDER BY starts_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(calendar_id)
        .bind(range.starts_at)
        .bind(range.ends_at)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find events in range: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event()?);
        }

        Ok(events)
    }

    async fn find_by_criteria(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        criteria: &Criteria,
    ) -> Result<Vec<Event>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at, \
             timezone, rrule, status, external_resource_id, metadata, \
             created_at, updated_at, deleted_at \
             FROM events WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id);
        qb.push(" AND calendar_id = ");
        qb.push_bind(calendar_id);
        qb.push(" AND deleted_at IS NULL");

        if let Some(ref filter) = criteria.filter {
            qb.push(" AND ");
            append_filter_group(&mut qb, filter, &resolve_event_field)?;
        }

        append_order_by(
            &mut qb,
            criteria.order_by.as_deref().unwrap_or(&[]),
            "starts_at ASC",
            &resolve_event_field,
        )?;

        let pagination = criteria.pagination.unwrap_or_default();
        let limit = i64::try_from(pagination.sql_limit()).unwrap_or(100);
        let offset = i64::try_from(pagination.offset()).unwrap_or(0);
        qb.push(" LIMIT ");
        qb.push_bind(limit);
        qb.push(" OFFSET ");
        qb.push_bind(offset);

        let rows: Vec<EventRow> = qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to search events: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event()?);
        }

        Ok(events)
    }

    async fn find_exceptions_batch(
        &self,
        tenant_id: Uuid,
        event_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<EventException>>, AppError> {
        if event_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let map = fetch_exceptions_batch_tx(&mut tx, tenant_id, event_ids).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;
        Ok(map)
    }

    async fn find_exceptions(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<EventException>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let rows = sqlx::query_as::<_, EventExceptionRow>(
            r#"
            SELECT id, tenant_id, event_id, original_starts_at, is_cancelled,
                   rescheduled_starts_at, rescheduled_ends_at, created_at
            FROM event_exceptions
            WHERE tenant_id = $1 AND event_id = $2
            ORDER BY original_starts_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(event_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to list event exceptions: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        let mut exceptions = Vec::with_capacity(rows.len());
        for row in rows {
            exceptions.push(row.into_exception()?);
        }

        Ok(exceptions)
    }

    async fn add_exception(
        &self,
        tenant_id: Uuid,
        exception: &EventException,
    ) -> Result<EventException, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let (rescheduled_s, rescheduled_e) = match exception.rescheduled_time_range {
            Some(range) => (Some(range.starts_at), Some(range.ends_at)),
            None => (None, None),
        };

        let row = sqlx::query_as::<_, EventExceptionRow>(
            r#"
            INSERT INTO event_exceptions (
                id, tenant_id, event_id, original_starts_at, is_cancelled,
                rescheduled_starts_at, rescheduled_ends_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, tenant_id, event_id, original_starts_at, is_cancelled,
                      rescheduled_starts_at, rescheduled_ends_at, created_at
            "#,
        )
        .bind(exception.id)
        .bind(tenant_id)
        .bind(exception.event_id)
        .bind(exception.original_starts_at)
        .bind(exception.is_cancelled)
        .bind(rescheduled_s)
        .bind(rescheduled_e)
        .bind(exception.created_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_exception_write_err)?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        row.into_exception()
    }

    async fn update(&self, tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let status_str = match event.status {
            EventStatus::Confirmed => "CONFIRMED",
            EventStatus::Tentative => "TENTATIVE",
            EventStatus::Cancelled => "CANCELLED",
        };

        let row = sqlx::query_as::<_, EventRow>(
            r#"
            UPDATE events
            SET title = $1, description = $2, starts_at = $3, ends_at = $4,
                timezone = $5, rrule = $6, status = $7, metadata = $8, updated_at = NOW()
            WHERE id = $9 AND tenant_id = $10 AND deleted_at IS NULL
            RETURNING id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                      timezone, rrule, status, external_resource_id, metadata,
                      created_at, updated_at, deleted_at
            "#,
        )
        .bind(&event.title)
        .bind(&event.description)
        .bind(event.time_range.starts_at)
        .bind(event.time_range.ends_at)
        .bind(&event.timezone)
        .bind(&event.rrule)
        .bind(status_str)
        .bind(&event.metadata)
        .bind(event.id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to update event: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("Event with ID '{}' not found", event.id)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        row.into_event()
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let result = sqlx::query(
            r#"
            UPDATE events
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft delete event: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn create_checked(
        &self,
        tenant_id: Uuid,
        event: &Event,
        allow_overlaps: bool,
    ) -> Result<Event, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        if !allow_overlaps {
            lock_calendar_tx(&mut tx, tenant_id, event.calendar_id).await?;
        }

        // 1. Idempotency check BEFORE overlap detection so retries never false-conflict
        if let Some(ext_id) = &event.external_resource_id {
            let existing = sqlx::query_as::<_, EventRow>(
                r#"
                SELECT id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                       timezone, rrule, status, external_resource_id, metadata,
                       created_at, updated_at, deleted_at
                FROM events
                WHERE tenant_id = $1 AND calendar_id = $2 AND external_resource_id = $3 AND deleted_at IS NULL
                "#,
            )
            .bind(tenant_id)
            .bind(event.calendar_id)
            .bind(ext_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Idempotency lookup failed: {e}")))?;

            if let Some(row) = existing {
                tx.commit().await.map_err(|e| {
                    AppError::DatabaseError(format!("Transaction commit failed: {e}"))
                })?;
                return row.into_event();
            }
        }

        // 2. Overlap validation under calendar advisory lock
        if !allow_overlaps {
            let candidate_windows = candidate_overlap_windows(event)?;
            let query_span = bounding_time_range(&candidate_windows, event.time_range)?;
            let existing_events =
                fetch_in_range_tx(&mut tx, tenant_id, event.calendar_id, &query_span).await?;

            let recurring_ids: Vec<Uuid> = existing_events
                .iter()
                .filter(|e| e.is_recurring())
                .map(|e| e.id)
                .collect();
            let exceptions_map =
                fetch_exceptions_batch_tx(&mut tx, tenant_id, &recurring_ids).await?;

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

        let status_str = match event.status {
            EventStatus::Confirmed => "CONFIRMED",
            EventStatus::Tentative => "TENTATIVE",
            EventStatus::Cancelled => "CANCELLED",
        };

        let row = sqlx::query_as::<_, EventRow>(
            r#"
            INSERT INTO events (
                id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                timezone, rrule, status, external_resource_id, metadata,
                created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                      timezone, rrule, status, external_resource_id, metadata,
                      created_at, updated_at, deleted_at
            "#,
        )
        .bind(event.id)
        .bind(tenant_id)
        .bind(event.calendar_id)
        .bind(&event.title)
        .bind(&event.description)
        .bind(event.time_range.starts_at)
        .bind(event.time_range.ends_at)
        .bind(&event.timezone)
        .bind(&event.rrule)
        .bind(status_str)
        .bind(&event.external_resource_id)
        .bind(&event.metadata)
        .bind(event.created_at)
        .bind(event.updated_at)
        .bind(event.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to insert event: {e}")))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit event transaction: {e}"))
        })?;

        row.into_event()
    }

    async fn update_checked(
        &self,
        tenant_id: Uuid,
        event: &Event,
        allow_overlaps: bool,
    ) -> Result<Event, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        if !allow_overlaps {
            lock_calendar_tx(&mut tx, tenant_id, event.calendar_id).await?;

            let candidate_windows = candidate_overlap_windows(event)?;
            let query_span = bounding_time_range(&candidate_windows, event.time_range)?;
            let existing_events =
                fetch_in_range_tx(&mut tx, tenant_id, event.calendar_id, &query_span).await?;

            let recurring_ids: Vec<Uuid> = existing_events
                .iter()
                .filter(|e| e.id != event.id && e.is_recurring())
                .map(|e| e.id)
                .collect();
            let exceptions_map =
                fetch_exceptions_batch_tx(&mut tx, tenant_id, &recurring_ids).await?;

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

        let status_str = match event.status {
            EventStatus::Confirmed => "CONFIRMED",
            EventStatus::Tentative => "TENTATIVE",
            EventStatus::Cancelled => "CANCELLED",
        };

        let row = sqlx::query_as::<_, EventRow>(
            r#"
            UPDATE events
            SET title = $1, description = $2, starts_at = $3, ends_at = $4,
                timezone = $5, rrule = $6, status = $7, metadata = $8, updated_at = NOW()
            WHERE id = $9 AND tenant_id = $10 AND deleted_at IS NULL
            RETURNING id, tenant_id, calendar_id, title, description, starts_at, ends_at,
                      timezone, rrule, status, external_resource_id, metadata,
                      created_at, updated_at, deleted_at
            "#,
        )
        .bind(&event.title)
        .bind(&event.description)
        .bind(event.time_range.starts_at)
        .bind(event.time_range.ends_at)
        .bind(&event.timezone)
        .bind(&event.rrule)
        .bind(status_str)
        .bind(&event.metadata)
        .bind(event.id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to update event: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("Event with ID '{}' not found", event.id)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        row.into_event()
    }

    async fn add_exception_checked(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        exception: &EventException,
        allow_overlaps: bool,
    ) -> Result<EventException, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        if !allow_overlaps && !exception.is_cancelled {
            if let Some(rescheduled) = exception.rescheduled_time_range {
                lock_calendar_tx(&mut tx, tenant_id, calendar_id).await?;

                let existing_events =
                    fetch_in_range_tx(&mut tx, tenant_id, calendar_id, &rescheduled).await?;
                let recurring_ids: Vec<Uuid> = existing_events
                    .iter()
                    .filter(|e| e.is_recurring())
                    .map(|e| e.id)
                    .collect();
                let exceptions_map =
                    fetch_exceptions_batch_tx(&mut tx, tenant_id, &recurring_ids).await?;

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

        let (rescheduled_s, rescheduled_e) = match exception.rescheduled_time_range {
            Some(range) => (Some(range.starts_at), Some(range.ends_at)),
            None => (None, None),
        };

        let row = sqlx::query_as::<_, EventExceptionRow>(
            r#"
            INSERT INTO event_exceptions (
                id, tenant_id, event_id, original_starts_at, is_cancelled,
                rescheduled_starts_at, rescheduled_ends_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, tenant_id, event_id, original_starts_at, is_cancelled,
                      rescheduled_starts_at, rescheduled_ends_at, created_at
            "#,
        )
        .bind(exception.id)
        .bind(tenant_id)
        .bind(exception.event_id)
        .bind(exception.original_starts_at)
        .bind(exception.is_cancelled)
        .bind(rescheduled_s)
        .bind(rescheduled_e)
        .bind(exception.created_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_exception_write_err)?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        row.into_exception()
    }
}
