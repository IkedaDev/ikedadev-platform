use super::state::AppState;
use crate::application::use_cases::{
    CreateBookingCommand, CreateBookingUseCase, DeleteBookingUseCase, GetBookingUseCase,
    SearchBookingsUseCase, TransitionBookingStatusCommand, TransitionBookingStatusUseCase,
    UpdateBookingCommand, UpdateBookingUseCase,
};
use crate::domain::models::Booking;
use crate::domain::repositories::IdempotentCreateResult;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::{ApiResponse, PaginatedApiResponse};
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

/// Header name for idempotent booking creation.
pub const HEADER_IDEMPOTENCY_KEY: &str = "idempotency-key";

fn require_bookings_write(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("bookings:write")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'bookings:write', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

/// Creates a new multi-service booking with frozen commercial snapshots and optional `Idempotency-Key`.
#[utoipa::path(
    post,
    path = "/v1/bookings",
    tag = "Bookings",
    request_body = CreateBookingCommand,
    params(
        ("Idempotency-Key" = Option<String>, Header, description = "Optional idempotency key (TTL 24h) to prevent duplicate bookings on retries")
    ),
    responses(
        (status = 201, description = "Booking created successfully", body = ApiResponse<Booking>),
        (status = 409, description = "Idempotency-Key conflict or duplicate booking code", body = ApiResponse<serde_json::Value>),
        (status = 422, description = "Validation error", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_booking(
    State(state): State<AppState>,
    tenant: TenantContext,
    headers: HeaderMap,
    Json(cmd): Json<CreateBookingCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_bookings_write(&tenant)?;
    let idempotency_key = headers
        .get(HEADER_IDEMPOTENCY_KEY)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let uc = CreateBookingUseCase::new(state.booking_repo.clone());
    match uc
        .execute(tenant.tenant_id, tenant.user_id, cmd, idempotency_key)
        .await?
    {
        IdempotentCreateResult::Created(booking) => {
            let body = serde_json::to_value(ApiResponse::created(
                *booking,
                "Booking created successfully",
            ))
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to serialize booking response: {e}"))
            })?;
            Ok((StatusCode::CREATED, Json(body)))
        }
        IdempotentCreateResult::Replayed { status, body } => {
            let code = u16::try_from(status)
                .ok()
                .and_then(|s| StatusCode::from_u16(s).ok())
                .unwrap_or(StatusCode::CREATED);
            Ok((code, Json(body)))
        }
    }
}

/// Retrieves a single booking by ID including its frozen line items.
#[utoipa::path(
    get,
    path = "/v1/bookings/{id}",
    tag = "Bookings",
    params(
        ("id" = Uuid, Path, description = "Booking ID")
    ),
    responses(
        (status = 200, description = "Booking found", body = ApiResponse<Booking>),
        (status = 404, description = "Booking not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_booking(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let uc = GetBookingUseCase::new(state.booking_repo.clone());
    let booking = uc.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::ok(booking)))
}

/// Partially updates a non-terminal booking (rejects edits on COMPLETED/CANCELLED/NO_SHOW with 409).
#[utoipa::path(
    patch,
    path = "/v1/bookings/{id}",
    tag = "Bookings",
    params(
        ("id" = Uuid, Path, description = "Booking ID")
    ),
    request_body = UpdateBookingCommand,
    responses(
        (status = 200, description = "Booking updated", body = ApiResponse<Booking>),
        (status = 404, description = "Booking not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Booking is in a terminal state and cannot be edited", body = ApiResponse<serde_json::Value>),
        (status = 422, description = "Validation error", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_booking(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateBookingCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_bookings_write(&tenant)?;
    let uc = UpdateBookingUseCase::new(state.booking_repo.clone());
    let booking = uc
        .execute(tenant.tenant_id, id, tenant.user_id, cmd)
        .await?;
    Ok(Json(ApiResponse::ok(booking)))
}

/// Transitions a booking to a new lifecycle status enforcing the strict state machine.
#[utoipa::path(
    patch,
    path = "/v1/bookings/{id}/status",
    tag = "Bookings",
    params(
        ("id" = Uuid, Path, description = "Booking ID")
    ),
    request_body = TransitionBookingStatusCommand,
    responses(
        (status = 200, description = "Booking status transitioned", body = ApiResponse<Booking>),
        (status = 404, description = "Booking not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Invalid state transition", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn transition_booking_status(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<TransitionBookingStatusCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_bookings_write(&tenant)?;
    let uc = TransitionBookingStatusUseCase::new(state.booking_repo.clone());
    let booking = uc
        .execute(tenant.tenant_id, id, tenant.user_id, cmd)
        .await?;
    Ok(Json(ApiResponse::ok(booking)))
}

/// Soft-deletes a booking.
#[utoipa::path(
    delete,
    path = "/v1/bookings/{id}",
    tag = "Bookings",
    params(
        ("id" = Uuid, Path, description = "Booking ID")
    ),
    responses(
        (status = 204, description = "Booking soft-deleted"),
        (status = 404, description = "Booking not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_booking(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_bookings_write(&tenant)?;
    let uc = DeleteBookingUseCase::new(state.booking_repo.clone());
    uc.execute(tenant.tenant_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Dynamic paginated Criteria search across tenant bookings with batch-loaded items.
#[utoipa::path(
    post,
    path = "/v1/bookings/search",
    tag = "Bookings",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated bookings matching criteria", body = PaginatedApiResponse<Booking>),
        (status = 400, description = "Invalid filter or sort field", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn search_bookings(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<impl IntoResponse, AppError> {
    let uc = SearchBookingsUseCase::new(state.booking_repo.clone());
    let response = uc.execute(tenant.tenant_id, criteria).await?;
    Ok(Json(response))
}
