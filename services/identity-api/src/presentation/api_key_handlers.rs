use super::state::AppState;
use super::tenant_handlers::SCOPE_PLATFORM_ADMIN;
use crate::application::use_cases::{
    CreateApiKeyCommand, CreateApiKeyUseCase, ListApiKeysUseCase, RevokeApiKeyUseCase,
};
use crate::domain::models::{ApiKeySummary, CreatedApiKey};
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

/// Required scope for tenant-level administration.
pub const SCOPE_TENANT_ADMIN: &str = "tenant:admin";

fn require_tenant_or_platform_admin(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_TENANT_ADMIN) || ctx.has_scope(SCOPE_PLATFORM_ADMIN) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'tenant:admin' or 'platform:admin'".to_string(),
        ))
    }
}

fn reject_privileged_scope_escalation(
    ctx: &TenantContext,
    scopes: &[String],
) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_PLATFORM_ADMIN) {
        return Ok(());
    }
    let escalates = scopes.iter().any(|s| {
        let trimmed = s.trim();
        trimmed == SCOPE_PLATFORM_ADMIN || trimmed == "*"
    });
    if escalates {
        return Err(AppError::Forbidden(
            "Cannot mint an API key with 'platform:admin' or wildcard scopes without 'platform:admin' scope"
                .to_string(),
        ));
    }
    Ok(())
}

/// Mints a new API key for the authenticated tenant, returning the plaintext secret ONCE.
#[utoipa::path(
    post,
    path = "/v1/api-keys",
    tag = "API Keys",
    request_body = CreateApiKeyCommand,
    responses(
        (status = 201, description = "API key minted (plaintext returned once)", body = ApiResponse<CreatedApiKey>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateApiKeyCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    reject_privileged_scope_escalation(&ctx, &cmd.scopes)?;
    let use_case = CreateApiKeyUseCase::new(state.api_key_repo.clone());
    let created = use_case.execute(ctx.tenant_id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(
            created,
            "API key created. Store the raw_key securely — it will not be shown again.",
        )),
    ))
}

/// Lists all API keys for the authenticated tenant (prefix and metadata only).
#[utoipa::path(
    get,
    path = "/v1/api-keys",
    tag = "API Keys",
    responses(
        (status = 200, description = "List of API key summaries", body = ApiResponse<Vec<ApiKeySummary>>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    let use_case = ListApiKeysUseCase::new(state.api_key_repo.clone());
    let keys = use_case.execute(ctx.tenant_id).await?;
    Ok(Json(ApiResponse::ok(keys)))
}

/// Revokes an API key by ID within the authenticated tenant.
#[utoipa::path(
    delete,
    path = "/v1/api-keys/{id}",
    tag = "API Keys",
    params(
        ("id" = Uuid, Path, description = "API key unique identifier")
    ),
    responses(
        (status = 200, description = "API key revoked successfully", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "API key not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    let use_case = RevokeApiKeyUseCase::new(state.api_key_repo.clone());
    use_case.execute(ctx.tenant_id, id).await?;
    Ok(Json(ApiResponse::message("API key revoked successfully")))
}
