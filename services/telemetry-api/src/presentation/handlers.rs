use super::dto::{ClientTimelineQueryParams, HealthStatus};
use super::state::AppState;
use crate::application::use_cases::{
    GetClientTimelineUseCase, GetMetricsCommand, GetMetricsUseCase, IngestBatchCommand,
    IngestBatchEventsUseCase, IngestBatchResult, IngestEventCommand, IngestEventUseCase,
    SearchEventsUseCase,
};
use crate::domain::models::{EventMetrics, TelemetryEvent};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::observability::REQUEST_ID_HEADER;
use ikeda_core::response::{ApiResponse, PaginatedApiResponse};
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

fn require_telemetry_write(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("telemetry:write")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'telemetry:write', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

fn require_telemetry_read(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("telemetry:read")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'telemetry:read', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

fn extract_request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Health check endpoint.
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service is operational", body = ApiResponse<HealthStatus>)
    )
)]
pub async fn health_check() -> ApiResponse<HealthStatus> {
    ApiResponse::ok(HealthStatus {
        status: "healthy".to_string(),
        service: "telemetry-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Ingests a single `TRACKING` or `AUDIT` telemetry event.
#[utoipa::path(
    post,
    path = "/v1/events",
    tag = "Events",
    request_body = IngestEventCommand,
    responses(
        (status = 201, description = "Telemetry event ingested", body = ApiResponse<TelemetryEvent>),
        (status = 422, description = "Validation error", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn ingest_event(
    State(state): State<AppState>,
    tenant: TenantContext,
    headers: HeaderMap,
    Json(cmd): Json<IngestEventCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_telemetry_write(&tenant)?;
    let request_id = extract_request_id(&headers);
    let uc = IngestEventUseCase::new(state.event_repo.clone());
    let event = uc
        .execute(tenant.tenant_id, tenant.user_id, request_id, cmd)
        .await?;
    Ok(ApiResponse::created(event, "Telemetry event ingested")
        .into_response_with_status(StatusCode::CREATED))
}

/// Ingests a batch of 1..=100 telemetry events in a single PostgreSQL transaction.
#[utoipa::path(
    post,
    path = "/v1/events/batch",
    tag = "Events",
    request_body = IngestBatchCommand,
    responses(
        (status = 201, description = "Telemetry event batch ingested", body = ApiResponse<IngestBatchResult>),
        (status = 422, description = "Validation error (empty batch, >100 events, or invalid item)", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn ingest_batch_events(
    State(state): State<AppState>,
    tenant: TenantContext,
    headers: HeaderMap,
    Json(cmd): Json<IngestBatchCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_telemetry_write(&tenant)?;
    let request_id = extract_request_id(&headers);
    let uc = IngestBatchEventsUseCase::new(state.event_repo.clone());
    let result = uc
        .execute(tenant.tenant_id, tenant.user_id, request_id, cmd)
        .await?;
    Ok(
        ApiResponse::created(result, "Telemetry event batch ingested")
            .into_response_with_status(StatusCode::CREATED),
    )
}

/// Dynamic paginated `Criteria` search across tenant telemetry events (including JSONB `properties.<path>`).
#[utoipa::path(
    post,
    path = "/v1/events/search",
    tag = "Events",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated telemetry events matching criteria", body = PaginatedApiResponse<TelemetryEvent>),
        (status = 400, description = "Invalid filter or sort field", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn search_events(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<impl IntoResponse, AppError> {
    require_telemetry_read(&tenant)?;
    let uc = SearchEventsUseCase::new(state.event_repo.clone());
    let response = uc.execute(tenant.tenant_id, criteria).await?;
    Ok(Json(response))
}

/// Retrieves a paginated chronological timeline of telemetry events for a specific client.
#[utoipa::path(
    get,
    path = "/v1/clients/{client_id}/timeline",
    tag = "Timeline",
    params(
        ("client_id" = Uuid, Path, description = "Client UUID"),
        ClientTimelineQueryParams
    ),
    responses(
        (status = 200, description = "Paginated client telemetry timeline", body = PaginatedApiResponse<TelemetryEvent>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_client_timeline(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(client_id): Path<Uuid>,
    Query(params): Query<ClientTimelineQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    require_telemetry_read(&tenant)?;
    let uc = GetClientTimelineUseCase::new(state.event_repo.clone());
    let response = uc
        .execute(
            tenant.tenant_id,
            client_id,
            params.kind,
            params.page,
            params.limit,
        )
        .await?;
    Ok(Json(response))
}

/// Computes aggregated telemetry metrics (`total_events`, `unique_clients`, `unique_sessions`, `top_events`) over `[from, to]`.
#[utoipa::path(
    post,
    path = "/v1/metrics/summary",
    tag = "Metrics",
    request_body = GetMetricsCommand,
    responses(
        (status = 200, description = "Aggregated telemetry metrics", body = ApiResponse<EventMetrics>),
        (status = 422, description = "Invalid date range or filter", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_metrics_summary(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(cmd): Json<GetMetricsCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_telemetry_read(&tenant)?;
    let uc = GetMetricsUseCase::new(state.event_repo.clone());
    let metrics = uc.execute(tenant.tenant_id, cmd).await?;
    Ok(Json(ApiResponse::ok(metrics)))
}
