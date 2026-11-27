use super::state::AppState;
use crate::application::use_cases::{
    webhook_use_cases, CreateWebhookCommand, UpdateWebhookCommand, WebhookTestResult,
};
use crate::domain::models::PublishWebhook;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use ikeda_core::prelude::*;
use uuid::Uuid;

#[utoipa::path(
    post,
    path = "/v1/webhooks",
    tag = "Deploy Hooks",
    request_body = CreateWebhookCommand,
    responses(
        (status = 201, description = "Deploy hook created", body = ApiResponse<PublishWebhook>),
        (status = 400, description = "Invalid or SSRF-blocked target URL"),
        (status = 403, description = "Missing content:publish scope")
    )
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateWebhookCommand>,
) -> Result<(StatusCode, ApiResponse<PublishWebhook>), AppError> {
    let hook = webhook_use_cases::create_webhook(state.webhook_repo.as_ref(), &ctx, cmd).await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(hook)))
}

#[utoipa::path(
    get,
    path = "/v1/webhooks",
    tag = "Deploy Hooks",
    responses(
        (status = 200, description = "Tenant deploy hooks", body = ApiResponse<Vec<PublishWebhook>>),
        (status = 403, description = "Missing content:publish scope")
    )
)]
pub async fn list_webhooks(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<ApiResponse<Vec<PublishWebhook>>, AppError> {
    let hooks = webhook_use_cases::list_webhooks(state.webhook_repo.as_ref(), &ctx).await?;
    Ok(ApiResponse::ok(hooks))
}

#[utoipa::path(
    patch,
    path = "/v1/webhooks/{id}",
    tag = "Deploy Hooks",
    params(("id" = Uuid, Path, description = "Webhook UUID")),
    request_body = UpdateWebhookCommand,
    responses(
        (status = 200, description = "Deploy hook updated", body = ApiResponse<PublishWebhook>),
        (status = 403, description = "Missing content:publish scope"),
        (status = 404, description = "Deploy hook not found")
    )
)]
pub async fn update_webhook(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateWebhookCommand>,
) -> Result<ApiResponse<PublishWebhook>, AppError> {
    let hook =
        webhook_use_cases::update_webhook(state.webhook_repo.as_ref(), &ctx, id, cmd).await?;
    Ok(ApiResponse::ok(hook))
}

#[utoipa::path(
    delete,
    path = "/v1/webhooks/{id}",
    tag = "Deploy Hooks",
    params(("id" = Uuid, Path, description = "Webhook UUID")),
    responses(
        (status = 204, description = "Deploy hook soft-deleted"),
        (status = 403, description = "Missing content:publish scope"),
        (status = 404, description = "Deploy hook not found")
    )
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    webhook_use_cases::delete_webhook(state.webhook_repo.as_ref(), &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/webhooks/{id}/test",
    tag = "Deploy Hooks",
    params(("id" = Uuid, Path, description = "Webhook UUID")),
    responses(
        (status = 200, description = "Deploy hook test result", body = ApiResponse<WebhookTestResult>),
        (status = 403, description = "Missing content:publish scope"),
        (status = 404, description = "Deploy hook not found")
    )
)]
pub async fn test_webhook(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<WebhookTestResult>, AppError> {
    let res =
        webhook_use_cases::test_webhook(state.webhook_repo.as_ref(), &state.http_client, &ctx, id)
            .await?;
    Ok(ApiResponse::ok(res))
}
