use super::state::AppState;
use crate::application::use_cases::{
    taxonomy_use_cases, CreateCategoryCommand, CreateTagCommand, UpdateCategoryCommand,
};
use crate::domain::models::{ContentCategory, ContentTag};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use ikeda_core::prelude::*;
use uuid::Uuid;

#[utoipa::path(
    post,
    path = "/v1/categories",
    tag = "Taxonomy",
    request_body = CreateCategoryCommand,
    responses(
        (status = 201, description = "Category created", body = ApiResponse<ContentCategory>),
        (status = 409, description = "Duplicate category slug")
    )
)]
pub async fn create_category(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateCategoryCommand>,
) -> Result<(StatusCode, ApiResponse<ContentCategory>), AppError> {
    let cat = taxonomy_use_cases::create_category(state.taxonomy_repo.as_ref(), &ctx, cmd).await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(cat)))
}

#[utoipa::path(
    get,
    path = "/v1/categories",
    tag = "Taxonomy",
    responses(
        (status = 200, description = "Tenant categories", body = ApiResponse<Vec<ContentCategory>>)
    )
)]
pub async fn list_categories(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<ApiResponse<Vec<ContentCategory>>, AppError> {
    let cats = taxonomy_use_cases::list_categories(state.taxonomy_repo.as_ref(), &ctx).await?;
    Ok(ApiResponse::ok(cats))
}

#[utoipa::path(
    patch,
    path = "/v1/categories/{id}",
    tag = "Taxonomy",
    params(("id" = Uuid, Path, description = "Category UUID")),
    request_body = UpdateCategoryCommand,
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<ContentCategory>),
        (status = 404, description = "Category not found")
    )
)]
pub async fn update_category(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCategoryCommand>,
) -> Result<ApiResponse<ContentCategory>, AppError> {
    let cat =
        taxonomy_use_cases::update_category(state.taxonomy_repo.as_ref(), &ctx, id, cmd).await?;
    Ok(ApiResponse::ok(cat))
}

#[utoipa::path(
    delete,
    path = "/v1/categories/{id}",
    tag = "Taxonomy",
    params(("id" = Uuid, Path, description = "Category UUID")),
    responses(
        (status = 204, description = "Category soft-deleted"),
        (status = 404, description = "Category not found")
    )
)]
pub async fn delete_category(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    taxonomy_use_cases::delete_category(state.taxonomy_repo.as_ref(), &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/tags",
    tag = "Taxonomy",
    request_body = CreateTagCommand,
    responses(
        (status = 201, description = "Tag created", body = ApiResponse<ContentTag>),
        (status = 409, description = "Duplicate tag slug")
    )
)]
pub async fn create_tag(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateTagCommand>,
) -> Result<(StatusCode, ApiResponse<ContentTag>), AppError> {
    let tag = taxonomy_use_cases::create_tag(state.taxonomy_repo.as_ref(), &ctx, cmd).await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(tag)))
}

#[utoipa::path(
    get,
    path = "/v1/tags",
    tag = "Taxonomy",
    responses(
        (status = 200, description = "Tenant tags", body = ApiResponse<Vec<ContentTag>>)
    )
)]
pub async fn list_tags(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<ApiResponse<Vec<ContentTag>>, AppError> {
    let tags = taxonomy_use_cases::list_tags(state.taxonomy_repo.as_ref(), &ctx).await?;
    Ok(ApiResponse::ok(tags))
}

#[utoipa::path(
    delete,
    path = "/v1/tags/{id}",
    tag = "Taxonomy",
    params(("id" = Uuid, Path, description = "Tag UUID")),
    responses(
        (status = 204, description = "Tag soft-deleted"),
        (status = 404, description = "Tag not found")
    )
)]
pub async fn delete_tag(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    taxonomy_use_cases::delete_tag(state.taxonomy_repo.as_ref(), &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
