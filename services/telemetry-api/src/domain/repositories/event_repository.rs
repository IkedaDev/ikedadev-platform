use crate::domain::models::{ClientTimelineQuery, EventMetrics, MetricsQuery, TelemetryEvent};
use async_trait::async_trait;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Result page returned by telemetry event search and timeline queries.
#[derive(Debug, Clone)]
pub struct EventSearchPage {
    pub items: Vec<TelemetryEvent>,
    pub total_items: u64,
}

/// Domain repository port for append-only telemetry persistence and analytics.
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Persists a single telemetry event within the tenant's RLS scope.
    async fn insert(
        &self,
        tenant_id: Uuid,
        event: &TelemetryEvent,
    ) -> Result<TelemetryEvent, AppError>;

    /// Persists a validated batch of telemetry events in a single RLS transaction.
    /// Returns the number of accepted/inserted rows.
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        events: &[TelemetryEvent],
    ) -> Result<usize, AppError>;

    /// Executes a dynamic paginated `Criteria` search within the tenant's RLS scope.
    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<EventSearchPage, AppError>;

    /// Computes aggregated telemetry metrics for `[from, to]` within the tenant's RLS scope.
    async fn get_metrics(
        &self,
        tenant_id: Uuid,
        query: &MetricsQuery,
    ) -> Result<EventMetrics, AppError>;

    /// Retrieves a paginated chronological timeline for a specific `client_id`.
    async fn get_client_timeline(
        &self,
        tenant_id: Uuid,
        query: &ClientTimelineQuery,
    ) -> Result<EventSearchPage, AppError>;
}
