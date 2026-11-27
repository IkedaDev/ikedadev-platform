use super::category_handlers::{self, *};
use super::item_handlers::{self, *};
use super::state::AppState;
use super::variant_handlers::{self, *};
use crate::application::use_cases::{
    BatchCatalogItemsQuery, CreateCatalogItemCommand, CreateCategoryCommand, CreateVariantCommand,
    UpdateCatalogItemCommand, UpdateCategoryCommand, UpdateVariantCommand,
};
use crate::domain::models::{
    BundleComponent, CatalogItem, CatalogItemVariant, Category, ItemDetails,
};
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, patch, post},
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

/// OpenAPI documentation schema definition for Catalog API.
#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        category_handlers::create_category,
        category_handlers::list_categories,
        category_handlers::patch_category,
        category_handlers::delete_category,
        item_handlers::create_item,
        item_handlers::get_item,
        item_handlers::patch_item,
        item_handlers::delete_item,
        item_handlers::search_items,
        item_handlers::batch_get_items,
        variant_handlers::create_variant,
        variant_handlers::patch_variant,
        variant_handlers::delete_variant
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<Category>,
            ApiResponse<Vec<Category>>,
            ApiResponse<CatalogItem>,
            ApiResponse<Vec<CatalogItem>>,
            ApiResponse<CatalogItemVariant>,
            PaginatedApiResponse<CatalogItem>,
            Category,
            CreateCategoryCommand,
            UpdateCategoryCommand,
            CatalogItem,
            ItemDetails,
            BundleComponent,
            CatalogItemVariant,
            CreateCatalogItemCommand,
            UpdateCatalogItemCommand,
            BatchCatalogItemsQuery,
            CreateVariantCommand,
            UpdateVariantCommand,
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
        (name = "Categories", description = "Hierarchical catalog categories per tenant"),
        (name = "Items", description = "Unified commercial catalog items (SERVICE, PRODUCT, BUNDLE)"),
        (name = "Variants", description = "Configurable price and duration variants per item")
    ),
    info(
        title = "IkedaDev Catalog API",
        version = "0.1.0",
        description = "Multi-tenant unified commercial catalog service supporting Services, Products, Bundles, Variants, Batch Lookup, and recursive Criteria search."
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
        service: "catalog-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Builds and configures the Catalog presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        // Categories
        .route("/v1/categories", post(create_category).get(list_categories))
        .route(
            "/v1/categories/{id}",
            patch(patch_category).delete(delete_category),
        )
        // Catalog Items
        .route("/v1/items", post(create_item))
        .route("/v1/items/search", post(search_items))
        .route(
            "/v1/items/batch",
            post(batch_get_items).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route(
            "/v1/items/{id}",
            get(get_item).patch(patch_item).delete(delete_item),
        )
        // Variants
        .route("/v1/items/{id}/variants", post(create_variant))
        .route(
            "/v1/items/{id}/variants/{variant_id}",
            patch(patch_variant).delete(delete_variant),
        )
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
