use super::dto::HealthStatus;
use super::handlers;
use crate::application::use_cases::{
    GetMetricsCommand, IngestBatchCommand, IngestBatchResult, IngestEventCommand,
};
use crate::domain::models::{EventKind, EventMetrics, EventNameCount, TelemetryEvent};
use ikeda_core::prelude::*;
use utoipa::OpenApi;

/// OpenAPI documentation schema definition for Telemetry API.
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health_check,
        handlers::ingest_event,
        handlers::ingest_batch_events,
        handlers::search_events,
        handlers::get_client_timeline,
        handlers::get_metrics_summary
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<TelemetryEvent>,
            ApiResponse<IngestBatchResult>,
            ApiResponse<EventMetrics>,
            PaginatedApiResponse<TelemetryEvent>,
            TelemetryEvent,
            EventKind,
            EventMetrics,
            EventNameCount,
            IngestEventCommand,
            IngestBatchCommand,
            IngestBatchResult,
            GetMetricsCommand,
            Criteria,
            FilterCondition,
            FilterGroup,
            FilterRule,
            FilterOperator,
            OrderBy,
            OrderDirection,
            Pagination,
            PaginationMeta
        )
    ),
    tags(
        (name = "Health", description = "Service lifecycle and readiness endpoints"),
        (name = "Events", description = "Append-only telemetry (TRACKING & AUDIT) ingestion and dynamic Criteria search"),
        (name = "Timeline", description = "Per-client chronological telemetry timeline"),
        (name = "Metrics", description = "Aggregated telemetry summary metrics")
    ),
    info(
        title = "IkedaDev Telemetry API",
        version = "0.1.0",
        description = "Multi-tenant append-only telemetry (TRACKING & AUDIT) microservice with PostgreSQL RLS, UNNEST batch ingestion, dynamic Criteria + JSONB search, client timelines, and metrics summaries."
    )
)]
pub struct ApiDoc;
