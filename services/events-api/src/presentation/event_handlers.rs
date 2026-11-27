use super::state::AppState;
use crate::application::use_cases::{
    CheckAvailabilityQuery, CheckAvailabilityUseCase, CreateEventCommand,
    CreateEventExceptionCommand, CreateEventExceptionUseCase, CreateEventUseCase,
    DeleteEventUseCase, FindEventsByRangeQuery, FindEventsByRangeUseCase, UpdateEventCommand,
    UpdateEventUseCase,
};
use crate::domain::models::{
    AvailabilityResult, Event, EventException, EventInstance, EventStatus, TimeRange,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::ApiResponse;
use ikeda_core::tenant::TenantContext;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Header name constant for request idempotency.
pub const HEADER_IDEMPOTENCY_KEY: &str = "idempotency-key";

pub(crate) fn require_events_write(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("events:write")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'events:write', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

/// Query parameters for fetching events within a date range.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct EventQueryParams {
    /// ID of the calendar.
    pub calendar_id: Uuid,
    /// UTC search window start.
    pub starts_at: DateTime<Utc>,
    /// UTC search window end.
    pub ends_at: DateTime<Utc>,
}

/// Request payload for dynamic criteria searching on events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventSearchRequest {
    /// ID of the calendar.
    pub calendar_id: Uuid,
    /// UTC search window start.
    pub starts_at: DateTime<Utc>,
    /// UTC search window end.
    pub ends_at: DateTime<Utc>,
    /// Optional dynamic recursive criteria.
    pub criteria: Option<Criteria>,
}

/// Query parameters for calendar availability slot calculation.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct AvailabilityQueryParams {
    /// Range start timestamp in UTC.
    pub starts_at: DateTime<Utc>,
    /// Range end timestamp in UTC.
    pub ends_at: DateTime<Utc>,
    /// Length of individual bookable slots in minutes (default: 30).
    pub slot_duration_minutes: Option<i64>,
}

/// Request payload for patching an existing event (alias to UpdateEventCommand).
pub type PatchEventPayload = UpdateEventCommand;

/// Creates a new event with idempotency support and optional overlap prevention.
#[utoipa::path(
    post,
    path = "/v1/events",
    tag = "Events",
    request_body = CreateEventCommand,
    params(
        ("Idempotency-Key" = Option<String>, Header, description = "Unique client request key for idempotent retries")
    ),
    responses(
        (status = 201, description = "Event created successfully", body = ApiResponse<Event>),
        (status = 409, description = "Conflict: event overlaps on strict calendar", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid event payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_event(
    State(state): State<AppState>,
    tenant: TenantContext,
    headers: HeaderMap,
    Json(mut cmd): Json<CreateEventCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    // Idempotency: inspect Idempotency-Key header and bind to external_resource_id if unassigned
    if cmd.external_resource_id.is_none() {
        if let Some(idempotency_val) = headers.get(HEADER_IDEMPOTENCY_KEY) {
            if let Ok(key_str) = idempotency_val.to_str() {
                let trimmed = key_str.trim();
                if !trimmed.is_empty() {
                    cmd.external_resource_id = Some(trimmed.to_string());
                }
            }
        }
    }

    let use_case = CreateEventUseCase::new(state.calendar_repo.clone(), state.event_repo.clone());
    let event = use_case.execute(tenant.tenant_id, cmd).await?;

    let response = ApiResponse::created(event, "Event created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Retrieves events expanded across a date range.
#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "Events",
    params(
        EventQueryParams
    ),
    responses(
        (status = 200, description = "List of expanded event occurrences", body = ApiResponse<Vec<EventInstance>>),
        (status = 400, description = "Invalid time range", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_events_by_range(
    State(state): State<AppState>,
    tenant: TenantContext,
    Query(params): Query<EventQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let range = TimeRange::new(params.starts_at, params.ends_at)?;
    let query = FindEventsByRangeQuery {
        calendar_id: params.calendar_id,
        range,
        criteria: None,
    };

    let use_case = FindEventsByRangeUseCase::new(state.event_repo.clone());
    let instances = use_case.execute(tenant.tenant_id, query).await?;

    Ok(Json(ApiResponse::ok(instances)))
}

/// Dynamic criteria search for events.
#[utoipa::path(
    post,
    path = "/v1/events/search",
    tag = "Events",
    request_body = EventSearchRequest,
    responses(
        (status = 200, description = "Filtered event occurrences", body = ApiResponse<Vec<EventInstance>>),
        (status = 400, description = "Invalid search payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn search_events(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(req): Json<EventSearchRequest>,
) -> Result<impl IntoResponse, AppError> {
    let range = TimeRange::new(req.starts_at, req.ends_at)?;
    let criteria = req.criteria.unwrap_or_default();

    let raw_events = state
        .event_repo
        .find_by_criteria(tenant.tenant_id, req.calendar_id, &criteria)
        .await?;

    let recurring_ids: Vec<Uuid> = raw_events
        .iter()
        .filter(|e| e.is_recurring())
        .map(|e| e.id)
        .collect();
    let exceptions_map = state
        .event_repo
        .find_exceptions_batch(tenant.tenant_id, &recurring_ids)
        .await?;

    let mut all_instances = Vec::new();
    for event in &raw_events {
        let exceptions = exceptions_map
            .get(&event.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let instances = crate::domain::services::expand_recurrence(event, &range, exceptions)?;
        all_instances.extend(instances);
    }

    Ok(Json(ApiResponse::ok(all_instances)))
}

/// Retrieves a specific event by ID.
#[utoipa::path(
    get,
    path = "/v1/events/{id}",
    tag = "Events",
    params(
        ("id" = Uuid, Path, description = "Event unique identifier")
    ),
    responses(
        (status = 200, description = "Event found", body = ApiResponse<Event>),
        (status = 404, description = "Event not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_event(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let event = state
        .event_repo
        .find_by_id(tenant.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Event with ID '{id}' not found")))?;

    Ok(Json(ApiResponse::ok(event)))
}

/// Updates details of an existing event.
#[utoipa::path(
    patch,
    path = "/v1/events/{id}",
    tag = "Events",
    params(
        ("id" = Uuid, Path, description = "Event unique identifier")
    ),
    request_body = UpdateEventCommand,
    responses(
        (status = 200, description = "Event updated successfully", body = ApiResponse<Event>),
        (status = 404, description = "Event not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Conflict: event overlaps on strict calendar", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_event(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateEventCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = UpdateEventUseCase::new(state.calendar_repo.clone(), state.event_repo.clone());
    let updated = use_case.execute(tenant.tenant_id, id, cmd).await?;
    Ok(Json(ApiResponse::ok(updated)))
}

/// Soft-deletes an event by ID.
#[utoipa::path(
    delete,
    path = "/v1/events/{id}",
    tag = "Events",
    params(
        ("id" = Uuid, Path, description = "Event unique identifier")
    ),
    responses(
        (status = 200, description = "Event deleted successfully", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Event not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_event(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = DeleteEventUseCase::new(state.event_repo.clone());
    use_case.execute(tenant.tenant_id, id).await?;

    Ok(Json(ApiResponse::message("Event deleted successfully")))
}

/// Creates an occurrence exception (cancellation or rescheduling) for a recurring event.
#[utoipa::path(
    post,
    path = "/v1/events/{id}/exceptions",
    tag = "Events",
    params(
        ("id" = Uuid, Path, description = "Parent recurring event identifier")
    ),
    request_body = CreateEventExceptionCommand,
    responses(
        (status = 201, description = "Event occurrence exception created", body = ApiResponse<EventException>),
        (status = 404, description = "Event not found", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid exception payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_event_exception(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<CreateEventExceptionCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = CreateEventExceptionUseCase::new(state.event_repo.clone())
        .with_calendar_repo(state.calendar_repo.clone());
    let exception = use_case.execute(tenant.tenant_id, id, cmd).await?;

    let response = ApiResponse::created(exception, "Event exception created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Computes available booking slots for a calendar.
#[utoipa::path(
    get,
    path = "/v1/calendars/{id}/availability",
    tag = "Calendars",
    params(
        ("id" = Uuid, Path, description = "Calendar unique identifier"),
        AvailabilityQueryParams
    ),
    responses(
        (status = 200, description = "Calendar availability blocks", body = ApiResponse<AvailabilityResult>),
        (status = 404, description = "Calendar not found", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid time range", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_calendar_availability(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Query(params): Query<AvailabilityQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let range = TimeRange::new(params.starts_at, params.ends_at)?;
    let query = CheckAvailabilityQuery {
        calendar_id: id,
        range,
        slot_duration_minutes: params.slot_duration_minutes,
    };

    let use_case =
        CheckAvailabilityUseCase::new(state.calendar_repo.clone(), state.event_repo.clone());
    let availability = use_case.execute(tenant.tenant_id, query).await?;

    Ok(Json(ApiResponse::ok(availability)))
}
