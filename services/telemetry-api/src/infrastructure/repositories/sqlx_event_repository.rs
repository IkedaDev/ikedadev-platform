use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::{
    ClientTimelineQuery, EventKind, EventMetrics, EventNameCount, MetricsQuery, TelemetryEvent,
};
use crate::domain::repositories::{EventRepository, EventSearchPage};
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct TelemetryEventRow {
    id: Uuid,
    tenant_id: Uuid,
    kind: String,
    event_name: String,
    source: String,
    client_id: Option<Uuid>,
    actor_id: Option<Uuid>,
    anonymous_id: Option<String>,
    session_id: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<Uuid>,
    request_id: Option<String>,
    properties: serde_json::Value,
    context: serde_json::Value,
    occurred_at: DateTime<Utc>,
    received_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct MetricsSummaryRow {
    total_events: i64,
    unique_clients: i64,
    unique_sessions: i64,
}

#[derive(Debug, FromRow)]
struct TopEventRow {
    event_name: String,
    count: i64,
}

impl TryFrom<TelemetryEventRow> for TelemetryEvent {
    type Error = AppError;

    fn try_from(r: TelemetryEventRow) -> Result<Self, Self::Error> {
        let kind = EventKind::parse_str(&r.kind)?;
        Ok(Self {
            id: r.id,
            tenant_id: r.tenant_id,
            kind,
            event_name: r.event_name,
            source: r.source,
            client_id: r.client_id,
            actor_id: r.actor_id,
            anonymous_id: r.anonymous_id,
            session_id: r.session_id,
            resource_type: r.resource_type,
            resource_id: r.resource_id,
            request_id: r.request_id,
            properties: r.properties,
            context: r.context,
            occurred_at: r.occurred_at,
            received_at: r.received_at,
        })
    }
}

/// PostgreSQL + RLS implementation of [`EventRepository`].
pub struct SqlxEventRepository {
    pool: PgPool,
}

impl SqlxEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventRepository for SqlxEventRepository {
    async fn insert(
        &self,
        tenant_id: Uuid,
        event: &TelemetryEvent,
    ) -> Result<TelemetryEvent, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, TelemetryEventRow>(
            r#"
            INSERT INTO telemetry_events (
                id, tenant_id, kind, event_name, source,
                client_id, actor_id, anonymous_id, session_id,
                resource_type, resource_id, request_id,
                properties, context, occurred_at, received_at
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12,
                $13, $14, $15, $16
            )
            RETURNING id, tenant_id, kind, event_name, source,
                      client_id, actor_id, anonymous_id, session_id,
                      resource_type, resource_id, request_id,
                      properties, context, occurred_at, received_at
            "#,
        )
        .bind(event.id)
        .bind(tenant_id)
        .bind(event.kind.as_str())
        .bind(&event.event_name)
        .bind(&event.source)
        .bind(event.client_id)
        .bind(event.actor_id)
        .bind(&event.anonymous_id)
        .bind(&event.session_id)
        .bind(&event.resource_type)
        .bind(event.resource_id)
        .bind(&event.request_id)
        .bind(&event.properties)
        .bind(&event.context)
        .bind(event.occurred_at)
        .bind(event.received_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to insert telemetry event: {e}")))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit telemetry insert: {e}"))
        })?;

        TelemetryEvent::try_from(row)
    }

    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        events: &[TelemetryEvent],
    ) -> Result<usize, AppError> {
        if events.is_empty() {
            return Ok(0);
        }

        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let len = events.len();
        let mut ids = Vec::with_capacity(len);
        let mut tenant_ids = Vec::with_capacity(len);
        let mut kinds = Vec::with_capacity(len);
        let mut event_names = Vec::with_capacity(len);
        let mut sources = Vec::with_capacity(len);
        let mut client_ids = Vec::with_capacity(len);
        let mut actor_ids = Vec::with_capacity(len);
        let mut anonymous_ids = Vec::with_capacity(len);
        let mut session_ids = Vec::with_capacity(len);
        let mut resource_types = Vec::with_capacity(len);
        let mut resource_ids = Vec::with_capacity(len);
        let mut request_ids = Vec::with_capacity(len);
        let mut properties = Vec::with_capacity(len);
        let mut contexts = Vec::with_capacity(len);
        let mut occurred_ats = Vec::with_capacity(len);
        let mut received_ats = Vec::with_capacity(len);

        for ev in events {
            ids.push(ev.id);
            tenant_ids.push(tenant_id);
            kinds.push(ev.kind.as_str().to_string());
            event_names.push(ev.event_name.clone());
            sources.push(ev.source.clone());
            client_ids.push(ev.client_id);
            actor_ids.push(ev.actor_id);
            anonymous_ids.push(ev.anonymous_id.clone());
            session_ids.push(ev.session_id.clone());
            resource_types.push(ev.resource_type.clone());
            resource_ids.push(ev.resource_id);
            request_ids.push(ev.request_id.clone());
            properties.push(ev.properties.clone());
            contexts.push(ev.context.clone());
            occurred_ats.push(ev.occurred_at);
            received_ats.push(ev.received_at);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO telemetry_events (
                id, tenant_id, kind, event_name, source,
                client_id, actor_id, anonymous_id, session_id,
                resource_type, resource_id, request_id,
                properties, context, occurred_at, received_at
            )
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::uuid[],
                $3::varchar[],
                $4::varchar[],
                $5::varchar[],
                $6::uuid[],
                $7::uuid[],
                $8::varchar[],
                $9::varchar[],
                $10::varchar[],
                $11::uuid[],
                $12::varchar[],
                $13::jsonb[],
                $14::jsonb[],
                $15::timestamptz[],
                $16::timestamptz[]
            )
            "#,
        )
        .bind(&ids)
        .bind(&tenant_ids)
        .bind(&kinds)
        .bind(&event_names)
        .bind(&sources)
        .bind(&client_ids)
        .bind(&actor_ids)
        .bind(&anonymous_ids)
        .bind(&session_ids)
        .bind(&resource_types)
        .bind(&resource_ids)
        .bind(&request_ids)
        .bind(&properties)
        .bind(&contexts)
        .bind(&occurred_ats)
        .bind(&received_ats)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to bulk-insert telemetry events: {e}"))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit telemetry batch insert: {e}"))
        })?;

        Ok(usize::try_from(result.rows_affected()).unwrap_or(len))
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<EventSearchPage, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut count_qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT COUNT(*)::bigint FROM telemetry_events WHERE tenant_id = ");
        count_qb.push_bind(tenant_id);
        if let Some(ref filter) = criteria.filter {
            count_qb.push(" AND ");
            append_filter_group(&mut count_qb, filter)?;
        }

        let total_items: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                AppError::DatabaseError(format!("Failed to count telemetry events: {e}"))
            })?;

        let pagination = criteria.pagination.unwrap_or_default();
        let page = pagination.page.max(1);
        let limit = pagination.limit.clamp(1, 100);
        let offset = (page - 1).saturating_mul(limit);

        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, tenant_id, kind, event_name, source, \
             client_id, actor_id, anonymous_id, session_id, \
             resource_type, resource_id, request_id, \
             properties, context, occurred_at, received_at \
             FROM telemetry_events WHERE tenant_id = ",
        );
        data_qb.push_bind(tenant_id);
        if let Some(ref filter) = criteria.filter {
            data_qb.push(" AND ");
            append_filter_group(&mut data_qb, filter)?;
        }
        let order_by_slice = criteria.order_by.as_deref().unwrap_or(&[]);
        append_order_by(&mut data_qb, order_by_slice)?;

        let limit_i64 = i64::try_from(limit).unwrap_or(100);
        let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);
        data_qb.push(" LIMIT ");
        data_qb.push_bind(limit_i64);
        data_qb.push(" OFFSET ");
        data_qb.push_bind(offset_i64);

        let rows: Vec<TelemetryEventRow> = data_qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| {
                AppError::DatabaseError(format!("Failed to search telemetry events: {e}"))
            })?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit telemetry search tx: {e}"))
        })?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(TelemetryEvent::try_from(row)?);
        }

        Ok(EventSearchPage {
            items,
            total_items: u64::try_from(total_items).unwrap_or(0),
        })
    }

    async fn get_metrics(
        &self,
        tenant_id: Uuid,
        query: &MetricsQuery,
    ) -> Result<EventMetrics, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let kind_str = query.kind.map(EventKind::as_str);

        let summary = sqlx::query_as::<_, MetricsSummaryRow>(
            r#"
            SELECT
                COUNT(*)::bigint AS total_events,
                COUNT(DISTINCT client_id)::bigint AS unique_clients,
                COUNT(DISTINCT session_id)::bigint AS unique_sessions
            FROM telemetry_events
            WHERE tenant_id = $1
              AND occurred_at >= $2
              AND occurred_at <= $3
              AND ($4::varchar IS NULL OR kind = $4)
              AND ($5::varchar IS NULL OR source = $5)
            "#,
        )
        .bind(tenant_id)
        .bind(query.from)
        .bind(query.to)
        .bind(kind_str)
        .bind(query.source.as_deref())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to aggregate telemetry metrics: {e}"))
        })?;

        let top_rows = sqlx::query_as::<_, TopEventRow>(
            r#"
            SELECT event_name, COUNT(*)::bigint AS count
            FROM telemetry_events
            WHERE tenant_id = $1
              AND occurred_at >= $2
              AND occurred_at <= $3
              AND ($4::varchar IS NULL OR kind = $4)
              AND ($5::varchar IS NULL OR source = $5)
            GROUP BY event_name
            ORDER BY count DESC, event_name ASC
            LIMIT $6
            "#,
        )
        .bind(tenant_id)
        .bind(query.from)
        .bind(query.to)
        .bind(kind_str)
        .bind(query.source.as_deref())
        .bind(query.top_limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to fetch top telemetry events: {e}"))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit telemetry metrics tx: {e}"))
        })?;

        let by_event_name = top_rows
            .into_iter()
            .map(|r| EventNameCount {
                event_name: r.event_name,
                count: u64::try_from(r.count).unwrap_or(0),
            })
            .collect();

        Ok(EventMetrics {
            from: query.from,
            to: query.to,
            total_events: u64::try_from(summary.total_events).unwrap_or(0),
            unique_clients: u64::try_from(summary.unique_clients).unwrap_or(0),
            unique_sessions: u64::try_from(summary.unique_sessions).unwrap_or(0),
            by_event_name,
        })
    }

    async fn get_client_timeline(
        &self,
        tenant_id: Uuid,
        query: &ClientTimelineQuery,
    ) -> Result<EventSearchPage, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let kind_str = query.kind.map(EventKind::as_str);

        let total_items: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM telemetry_events
            WHERE tenant_id = $1
              AND client_id = $2
              AND ($3::varchar IS NULL OR kind = $3)
            "#,
        )
        .bind(tenant_id)
        .bind(query.client_id)
        .bind(kind_str)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to count client timeline events: {e}"))
        })?;

        let offset = (query.page.saturating_sub(1)).saturating_mul(query.limit);
        let limit_i64 = i64::try_from(query.limit).unwrap_or(100);
        let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);

        let rows = sqlx::query_as::<_, TelemetryEventRow>(
            r#"
            SELECT id, tenant_id, kind, event_name, source,
                   client_id, actor_id, anonymous_id, session_id,
                   resource_type, resource_id, request_id,
                   properties, context, occurred_at, received_at
            FROM telemetry_events
            WHERE tenant_id = $1
              AND client_id = $2
              AND ($3::varchar IS NULL OR kind = $3)
            ORDER BY occurred_at DESC, received_at DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(tenant_id)
        .bind(query.client_id)
        .bind(kind_str)
        .bind(limit_i64)
        .bind(offset_i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to fetch client timeline events: {e}"))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit client timeline tx: {e}"))
        })?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(TelemetryEvent::try_from(row)?);
        }

        Ok(EventSearchPage {
            items,
            total_items: u64::try_from(total_items).unwrap_or(0),
        })
    }
}
