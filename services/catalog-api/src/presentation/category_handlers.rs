use super::item_handlers::require_catalog_write;
use super::state::AppState;
use crate::application::use_cases::{
    CreateCategoryCommand, CreateCategoryUseCase, DeleteCategoryUseCase, ListCategoriesUseCase,
    UpdateCategoryCommand, UpdateCategoryUseCase,
};
use crate::domain::models::Category;
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

/// Creates a new catalog category for the authenticated tenant.
#[utoipa::path(
    post,
    path = "/v1/categories",
    tag = "Categories",
    request_body = CreateCategoryCommand,
    responses(
        (status = 201, description = "Category created", body = ApiResponse<Category>),
        (status = 409, description = "Slug already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_category(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(cmd): Json<CreateCategoryCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_catalog_write(&tenant)?;
    let uc = CreateCategoryUseCase::new(state.category_repo.clone());
    let category = uc.execute(tenant.tenant_id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(
            category,
            "Category created successfully",
        )),
    ))
}

/// Lists all active catalog categories for the authenticated tenant.
#[utoipa::path(
    get,
    path = "/v1/categories",
    tag = "Categories",
    responses(
        (status = 200, description = "List of categories", body = ApiResponse<Vec<Category>>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_categories(
    State(state): State<AppState>,
    tenant: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let uc = ListCategoriesUseCase::new(state.category_repo.clone());
    let categories = uc.execute(tenant.tenant_id).await?;
    Ok(Json(ApiResponse::ok(categories)))
}

/// Partially updates an existing catalog category.
#[utoipa::path(
    patch,
    path = "/v1/categories/{id}",
    tag = "Categories",
    params(
        ("id" = Uuid, Path, description = "Category unique identifier")
    ),
    request_body = UpdateCategoryCommand,
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<Category>),
        (status = 404, description = "Category not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Slug already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_category(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCategoryCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_catalog_write(&tenant)?;
    let uc = UpdateCategoryUseCase::new(state.category_repo.clone());
    let updated = uc.execute(tenant.tenant_id, id, cmd).await?;
    Ok(Json(ApiResponse::ok(updated)))
}

/// Soft-deletes a catalog category.
#[utoipa::path(
    delete,
    path = "/v1/categories/{id}",
    tag = "Categories",
    params(
        ("id" = Uuid, Path, description = "Category unique identifier")
    ),
    responses(
        (status = 200, description = "Category deleted", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Category not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_category(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_catalog_write(&tenant)?;
    let uc = DeleteCategoryUseCase::new(state.category_repo.clone());
    uc.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::message("Category deleted successfully")))
}
