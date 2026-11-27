use super::event_handlers::require_events_write;
use super::state::AppState;
use crate::application::use_cases::{
    CreateCalendarCommand, CreateCalendarUseCase, DeleteCalendarUseCase, UpdateCalendarCommand,
    UpdateCalendarUseCase,
};
use crate::domain::models::Calendar;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use ikeda_core::error::AppError;
use ikeda_core::response::ApiResponse;
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

/// Creates a new calendar for the authenticated tenant.
#[utoipa::path(
    post,
    path = "/v1/calendars",
    tag = "Calendars",
    request_body = CreateCalendarCommand,
    responses(
        (status = 201, description = "Calendar created successfully", body = ApiResponse<Calendar>),
        (status = 400, description = "Invalid request payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_calendar(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(cmd): Json<CreateCalendarCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = CreateCalendarUseCase::new(state.calendar_repo.clone());
    let calendar = use_case.execute(tenant.tenant_id, cmd).await?;

    let response = ApiResponse::created(calendar, "Calendar created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Lists all active calendars owned by the authenticated tenant.
#[utoipa::path(
    get,
    path = "/v1/calendars",
    tag = "Calendars",
    responses(
        (status = 200, description = "List of calendars", body = ApiResponse<Vec<Calendar>>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_calendars(
    State(state): State<AppState>,
    tenant: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let calendars = state.calendar_repo.find_by_tenant(tenant.tenant_id).await?;

    Ok(Json(ApiResponse::ok(calendars)))
}

/// Retrieves a specific calendar by ID.
#[utoipa::path(
    get,
    path = "/v1/calendars/{id}",
    tag = "Calendars",
    params(
        ("id" = Uuid, Path, description = "Calendar unique identifier")
    ),
    responses(
        (status = 200, description = "Calendar found", body = ApiResponse<Calendar>),
        (status = 404, description = "Calendar not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_calendar(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let calendar = state
        .calendar_repo
        .find_by_id(tenant.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Calendar with ID '{id}' not found")))?;

    Ok(Json(ApiResponse::ok(calendar)))
}

/// Updates details of an existing calendar.
#[utoipa::path(
    patch,
    path = "/v1/calendars/{id}",
    tag = "Calendars",
    params(
        ("id" = Uuid, Path, description = "Calendar unique identifier")
    ),
    request_body = UpdateCalendarCommand,
    responses(
        (status = 200, description = "Calendar updated successfully", body = ApiResponse<Calendar>),
        (status = 404, description = "Calendar not found", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_calendar(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCalendarCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = UpdateCalendarUseCase::new(state.calendar_repo.clone());
    let calendar = use_case.execute(tenant.tenant_id, id, cmd).await?;

    Ok(Json(ApiResponse::ok(calendar)))
}

/// Soft-deletes a calendar by ID.
#[utoipa::path(
    delete,
    path = "/v1/calendars/{id}",
    tag = "Calendars",
    params(
        ("id" = Uuid, Path, description = "Calendar unique identifier")
    ),
    responses(
        (status = 200, description = "Calendar deleted successfully", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Calendar not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_calendar(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_events_write(&tenant)?;
    let use_case = DeleteCalendarUseCase::new(state.calendar_repo.clone());
    use_case.execute(tenant.tenant_id, id).await?;

    Ok(Json(ApiResponse::message("Calendar deleted successfully")))
}
