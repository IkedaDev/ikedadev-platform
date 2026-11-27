use super::client_handlers::{self, *};
use super::state::AppState;
use crate::application::use_cases::{CreateClientCommand, UpdateClientCommand};
use crate::domain::models::Client;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use ikeda_core::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

/// Health status response payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    /// Service running state.
    pub status: String,
    /// Service identifier.
    pub service: String,
    /// Semantic version of the service.
    pub version: String,
}

/// OpenAPI documentation schema definition for CRM API.
#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        client_handlers::create_client,
        client_handlers::get_client,
        client_handlers::patch_client,
        client_handlers::delete_client,
        client_handlers::search_clients
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<Client>,
            PaginatedApiResponse<Client>,
            Client,
            CreateClientCommand,
            UpdateClientCommand,
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
        (name = "Clients", description = "Multi-tenant CRM end-customer management and dynamic criteria search")
    ),
    info(
        title = "IkedaDev CRM API",
        version = "0.1.0",
        description = "Multi-tenant CRM service managing end-customers with PostgreSQL RLS, partial unique constraints, JSONB metadata, and recursive Criteria search."
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
        service: "crm-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Builds and configures the CRM presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/clients", post(create_client))
        .route("/v1/clients/search", post(search_clients))
        .route(
            "/v1/clients/{id}",
            get(get_client).patch(patch_client).delete(delete_client),
        )
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
