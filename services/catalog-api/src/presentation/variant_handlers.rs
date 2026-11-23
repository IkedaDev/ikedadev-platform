use super::state::AppState;
use crate::application::use_cases::{
    CreateVariantCommand, CreateVariantUseCase, DeleteVariantUseCase, UpdateVariantCommand,
    UpdateVariantUseCase,
};
use crate::domain::models::CatalogItemVariant;
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

/// Adds a new variant to an existing catalog item.
#[utoipa::path(
    post,
    path = "/v1/items/{id}/variants",
    tag = "Variants",
    params(
        ("id" = Uuid, Path, description = "Parent catalog item unique identifier")
    ),
    request_body = CreateVariantCommand,
    responses(
        (status = 201, description = "Variant created", body = ApiResponse<CatalogItemVariant>),
        (status = 404, description = "Parent item not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Variant SKU already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid variant payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_variant(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<CreateVariantCommand>,
) -> Result<impl IntoResponse, AppError> {
    let uc = CreateVariantUseCase::new(state.item_repo.clone());
    let variant = uc.execute(tenant.tenant_id, id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(
            variant,
            "Variant created successfully",
        )),
    ))
}

/// Partially updates an existing variant on a catalog item.
#[utoipa::path(
    patch,
    path = "/v1/items/{id}/variants/{variant_id}",
    tag = "Variants",
    params(
        ("id" = Uuid, Path, description = "Parent catalog item unique identifier"),
        ("variant_id" = Uuid, Path, description = "Variant unique identifier")
    ),
    request_body = UpdateVariantCommand,
    responses(
        (status = 200, description = "Variant updated", body = ApiResponse<CatalogItemVariant>),
        (status = 404, description = "Variant not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Variant SKU already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_variant(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((id, variant_id)): Path<(Uuid, Uuid)>,
    Json(cmd): Json<UpdateVariantCommand>,
) -> Result<impl IntoResponse, AppError> {
    let uc = UpdateVariantUseCase::new(state.item_repo.clone());
    let updated = uc.execute(tenant.tenant_id, id, variant_id, cmd).await?;
    Ok(Json(ApiResponse::ok(updated)))
}

/// Soft-deletes a variant from a catalog item.
#[utoipa::path(
    delete,
    path = "/v1/items/{id}/variants/{variant_id}",
    tag = "Variants",
    params(
        ("id" = Uuid, Path, description = "Parent catalog item unique identifier"),
        ("variant_id" = Uuid, Path, description = "Variant unique identifier")
    ),
    responses(
        (status = 200, description = "Variant deleted", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Variant not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_variant(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((id, variant_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let uc = DeleteVariantUseCase::new(state.item_repo.clone());
    uc.execute(tenant.tenant_id, id, variant_id).await?;
    Ok(Json(ApiResponse::message("Variant deleted successfully")))
}
