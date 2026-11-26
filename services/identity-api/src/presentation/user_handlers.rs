use super::api_key_handlers::SCOPE_TENANT_ADMIN;
use super::state::AppState;
use super::tenant_handlers::SCOPE_PLATFORM_ADMIN;
use crate::application::use_cases::{
    CreateTenantUserCommand, CreateTenantUserUseCase, DeleteTenantUserUseCase,
    ListTenantUsersUseCase,
};
use crate::domain::models::TenantUser;
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

fn require_tenant_or_platform_admin(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_TENANT_ADMIN) || ctx.has_scope(SCOPE_PLATFORM_ADMIN) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'tenant:admin' or 'platform:admin'".to_string(),
        ))
    }
}

fn reject_privileged_role_escalation(
    ctx: &TenantContext,
    roles: &[String],
) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_PLATFORM_ADMIN) {
        return Ok(());
    }
    let escalates = roles.iter().any(|r| {
        let trimmed = r.trim();
        trimmed == SCOPE_PLATFORM_ADMIN || trimmed == "*"
    });
    if escalates {
        return Err(AppError::Forbidden(
            "Cannot assign 'platform:admin' or wildcard roles without 'platform:admin' scope"
                .to_string(),
        ));
    }
    Ok(())
}

/// Provisions a new user in Keycloak (with `tenant_id` attribute) and persists them in `tenant_users`.
#[utoipa::path(
    post,
    path = "/v1/users",
    tag = "Users",
    request_body = CreateTenantUserCommand,
    responses(
        (status = 201, description = "User provisioned in Keycloak and tenant_users", body = ApiResponse<TenantUser>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Conflict: email already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid user payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_user(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateTenantUserCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    reject_privileged_role_escalation(&ctx, &cmd.roles)?;
    let use_case = CreateTenantUserUseCase::new(state.user_repo.clone(), state.idp.clone());
    let user = use_case.execute(ctx.tenant_id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(user, "User provisioned successfully")),
    ))
}

/// Lists all active users belonging to the authenticated tenant.
#[utoipa::path(
    get,
    path = "/v1/users",
    tag = "Users",
    responses(
        (status = 200, description = "List of active tenant users", body = ApiResponse<Vec<TenantUser>>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let use_case = ListTenantUsersUseCase::new(state.user_repo.clone());
    let users = use_case.execute(ctx.tenant_id).await?;
    Ok(Json(ApiResponse::ok(users)))
}

/// Disables a user in Keycloak and performs a soft delete in `tenant_users`.
#[utoipa::path(
    delete,
    path = "/v1/users/{id}",
    tag = "Users",
    params(
        ("id" = Uuid, Path, description = "Internal tenant user unique identifier")
    ),
    responses(
        (status = 200, description = "User disabled in Keycloak and soft-deleted", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "User not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_user(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    let use_case = DeleteTenantUserUseCase::new(state.user_repo.clone(), state.idp.clone());
    use_case.execute(ctx.tenant_id, id).await?;
    Ok(Json(ApiResponse::message(
        "User disabled and deleted successfully",
    )))
}
