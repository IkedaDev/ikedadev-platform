use super::state::AppState;
use crate::application::use_cases::{
    CreateTenantCommand, CreateTenantUseCase, GetTenantUseCase, UpdateTenantCommand,
    UpdateTenantUseCase,
};
use crate::domain::models::Tenant;
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

/// Required scope for platform-level tenant administration.
pub const SCOPE_PLATFORM_ADMIN: &str = "platform:admin";

/// Provisions a new platform tenant (requires `platform:admin` scope).
#[utoipa::path(
    post,
    path = "/v1/tenants",
    tag = "Tenants",
    request_body = CreateTenantCommand,
    responses(
        (status = 201, description = "Tenant provisioned successfully", body = ApiResponse<Tenant>),
        (status = 403, description = "Forbidden: missing platform:admin scope", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Conflict: slug already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_tenant(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateTenantCommand>,
) -> Result<impl IntoResponse, AppError> {
    ctx.require_scope(SCOPE_PLATFORM_ADMIN)?;
    let use_case = CreateTenantUseCase::new(state.tenant_repo.clone());
    let tenant = use_case.execute(cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(tenant, "Tenant created successfully")),
    ))
}

/// Retrieves a tenant by ID (requires `platform:admin` scope).
#[utoipa::path(
    get,
    path = "/v1/tenants/{id}",
    tag = "Tenants",
    params(
        ("id" = Uuid, Path, description = "Tenant unique identifier")
    ),
    responses(
        (status = 200, description = "Tenant found", body = ApiResponse<Tenant>),
        (status = 403, description = "Forbidden: missing platform:admin scope", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Tenant not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_tenant(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.require_scope(SCOPE_PLATFORM_ADMIN)?;
    let use_case = GetTenantUseCase::new(state.tenant_repo.clone());
    let tenant = use_case.execute(id).await?;
    Ok(Json(ApiResponse::ok(tenant)))
}

/// Updates a tenant's name, status, or config (requires `platform:admin` scope).
#[utoipa::path(
    patch,
    path = "/v1/tenants/{id}",
    tag = "Tenants",
    params(
        ("id" = Uuid, Path, description = "Tenant unique identifier")
    ),
    request_body = UpdateTenantCommand,
    responses(
        (status = 200, description = "Tenant updated successfully", body = ApiResponse<Tenant>),
        (status = 403, description = "Forbidden: missing platform:admin scope", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Tenant not found", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_tenant(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateTenantCommand>,
) -> Result<impl IntoResponse, AppError> {
    ctx.require_scope(SCOPE_PLATFORM_ADMIN)?;
    let use_case = UpdateTenantUseCase::new(state.tenant_repo.clone());
    let tenant = use_case.execute(id, cmd).await?;
    Ok(Json(ApiResponse::ok(tenant)))
}
