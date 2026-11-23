use super::state::AppState;
use crate::application::use_cases::{
    BatchCatalogItemsQuery, CreateCatalogItemCommand, CreateCatalogItemUseCase,
    DeleteCatalogItemUseCase, GetCatalogItemUseCase, GetCatalogItemsBatchUseCase,
    SearchCatalogItemsUseCase, UpdateCatalogItemCommand, UpdateCatalogItemUseCase,
};
use crate::domain::models::CatalogItem;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::{ApiResponse, PaginatedApiResponse};
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

/// Creates a new catalog item (`SERVICE`, `PRODUCT`, or `BUNDLE`).
#[utoipa::path(
    post,
    path = "/v1/items",
    tag = "Items",
    request_body = CreateCatalogItemCommand,
    responses(
        (status = 201, description = "Catalog item created", body = ApiResponse<CatalogItem>),
        (status = 409, description = "SKU already exists in tenant", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid item payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_item(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(cmd): Json<CreateCatalogItemCommand>,
) -> Result<impl IntoResponse, AppError> {
    let uc = CreateCatalogItemUseCase::new(state.category_repo.clone(), state.item_repo.clone());
    let item = uc.execute(tenant.tenant_id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(
            item,
            "Catalog item created successfully",
        )),
    ))
}

/// Retrieves a catalog item by ID, hydrated with its active variants and bundle components.
#[utoipa::path(
    get,
    path = "/v1/items/{id}",
    tag = "Items",
    params(
        ("id" = Uuid, Path, description = "Catalog item unique identifier")
    ),
    responses(
        (status = 200, description = "Catalog item found", body = ApiResponse<CatalogItem>),
        (status = 404, description = "Catalog item not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_item(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let uc = GetCatalogItemUseCase::new(state.item_repo.clone());
    let item = uc.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::ok(item)))
}

/// Partially updates an existing catalog item.
#[utoipa::path(
    patch,
    path = "/v1/items/{id}",
    tag = "Items",
    params(
        ("id" = Uuid, Path, description = "Catalog item unique identifier")
    ),
    request_body = UpdateCatalogItemCommand,
    responses(
        (status = 200, description = "Catalog item updated", body = ApiResponse<CatalogItem>),
        (status = 404, description = "Catalog item not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "SKU already exists in tenant", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_item(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCatalogItemCommand>,
) -> Result<impl IntoResponse, AppError> {
    let uc = UpdateCatalogItemUseCase::new(state.category_repo.clone(), state.item_repo.clone());
    let updated = uc.execute(tenant.tenant_id, id, cmd).await?;
    Ok(Json(ApiResponse::ok(updated)))
}

/// Soft-deletes a catalog item by ID.
#[utoipa::path(
    delete,
    path = "/v1/items/{id}",
    tag = "Items",
    params(
        ("id" = Uuid, Path, description = "Catalog item unique identifier")
    ),
    responses(
        (status = 200, description = "Catalog item deleted", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Catalog item not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_item(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let uc = DeleteCatalogItemUseCase::new(state.item_repo.clone());
    uc.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::message(
        "Catalog item deleted successfully",
    )))
}

/// Searches catalog items using recursive `Criteria` (standard columns & JSONB `metadata.*`) with pagination.
#[utoipa::path(
    post,
    path = "/v1/items/search",
    tag = "Items",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated catalog items", body = PaginatedApiResponse<CatalogItem>),
        (status = 400, description = "Invalid search criteria", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn search_items(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<impl IntoResponse, AppError> {
    let uc = SearchCatalogItemsUseCase::new(state.item_repo.clone());
    let page = uc.execute(tenant.tenant_id, criteria).await?;
    Ok(Json(page))
}

/// Batch lookup of up to 100 catalog items by ID in a single query (eliminates N+1 in BFFs).
#[utoipa::path(
    post,
    path = "/v1/items/batch",
    tag = "Items",
    request_body = BatchCatalogItemsQuery,
    responses(
        (status = 200, description = "Batch of catalog items found", body = ApiResponse<Vec<CatalogItem>>),
        (status = 400, description = "Too many IDs (max 100)", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn batch_get_items(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(query): Json<BatchCatalogItemsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let uc = GetCatalogItemsBatchUseCase::new(state.item_repo.clone());
    let items = uc.execute(tenant.tenant_id, query.ids).await?;
    Ok(Json(ApiResponse::ok(items)))
}
