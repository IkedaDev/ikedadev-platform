use super::booking_handlers::{self, *};
use super::state::AppState;
use crate::application::use_cases::{
    CreateBookingCommand, TransitionBookingStatusCommand, UpdateBookingCommand,
};
use crate::domain::models::{Booking, BookingItem, BookingStatus};
use crate::domain::services::BookingItemInput;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, patch, post},
    Router,
};
use ikeda_core::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

/// Health status response payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// OpenAPI documentation schema definition for Bookings API.
#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        booking_handlers::create_booking,
        booking_handlers::get_booking,
        booking_handlers::patch_booking,
        booking_handlers::transition_booking_status,
        booking_handlers::delete_booking,
        booking_handlers::search_bookings
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<Booking>,
            PaginatedApiResponse<Booking>,
            Booking,
            BookingItem,
            BookingStatus,
            BookingItemInput,
            CreateBookingCommand,
            UpdateBookingCommand,
            TransitionBookingStatusCommand,
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
        (name = "Bookings", description = "Multi-tenant domain-agnostic bookings with frozen snapshots, state machine, idempotency and Criteria search")
    ),
    info(
        title = "IkedaDev Bookings API",
        version = "0.1.0",
        description = "Multi-tenant commercial bookings service with frozen line-item snapshots, strict lifecycle state machine, transactional idempotency, PostgreSQL RLS, and dynamic Criteria search."
    )
)]
pub struct ApiDoc;

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
        service: "bookings-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Builds and configures the Bookings presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/bookings", post(create_booking))
        .route("/v1/bookings/search", post(search_bookings))
        .route(
            "/v1/bookings/{id}",
            get(get_booking).patch(patch_booking).delete(delete_booking),
        )
        .route("/v1/bookings/{id}/status", patch(transition_booking_status))
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
