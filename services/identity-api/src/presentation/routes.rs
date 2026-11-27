use super::api_key_handlers::{self, *};
use super::state::AppState;
use super::tenant_handlers::{self, *};
use super::user_handlers::{self, *};
use crate::application::use_cases::{
    CreateApiKeyCommand, CreateTenantCommand, CreateTenantUserCommand, UpdateTenantCommand,
};
use crate::domain::models::{ApiKeySummary, CreatedApiKey, Tenant, TenantUser};
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
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

/// OpenAPI documentation schema definition for Identity API.
#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        tenant_handlers::create_tenant,
        tenant_handlers::get_tenant,
        tenant_handlers::patch_tenant,
        api_key_handlers::create_api_key,
        api_key_handlers::list_api_keys,
        api_key_handlers::revoke_api_key,
        user_handlers::create_user,
        user_handlers::list_users,
        user_handlers::delete_user
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<Tenant>,
            ApiResponse<CreatedApiKey>,
            ApiResponse<Vec<ApiKeySummary>>,
            ApiResponse<TenantUser>,
            ApiResponse<Vec<TenantUser>>,
            Tenant,
            CreateTenantCommand,
            UpdateTenantCommand,
            ApiKeySummary,
            CreatedApiKey,
            CreateApiKeyCommand,
            TenantUser,
            CreateTenantUserCommand
        )
    ),
    tags(
        (name = "Health", description = "Service lifecycle and readiness endpoints"),
        (name = "Tenants", description = "Platform tenant provisioning (requires platform:admin)"),
        (name = "API Keys", description = "Per-tenant M2M API key lifecycle (requires tenant:admin or platform:admin)"),
        (name = "Users", description = "Tenant user provisioning synchronized with Keycloak")
    ),
    info(
        title = "IkedaDev Identity API",
        version = "0.1.0",
        description = "Multi-tenant orchestrator for platform Tenants, M2M API Keys, and Keycloak Users."
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
        service: "identity-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Builds and configures the Identity presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        // Tenants (platform:admin)
        .route("/v1/tenants", post(create_tenant))
        .route("/v1/tenants/{id}", get(get_tenant).patch(patch_tenant))
        // API Keys (tenant:admin or platform:admin)
        .route("/v1/api-keys", post(create_api_key).get(list_api_keys))
        .route("/v1/api-keys/{id}", delete(revoke_api_key))
        // Tenant Users (synchronized with Keycloak)
        .route("/v1/users", post(create_user).get(list_users))
        .route("/v1/users/{id}", delete(delete_user))
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
