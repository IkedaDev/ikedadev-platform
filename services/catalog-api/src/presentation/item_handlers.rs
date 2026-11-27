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

pub(crate) fn require_catalog_write(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("catalog:write")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'catalog:write', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

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
    require_catalog_write(&tenant)?;
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
    require_catalog_write(&tenant)?;
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
    require_catalog_write(&tenant)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{CatalogItemVariant, Category};
    use crate::domain::repositories::{
        CatalogItemRepository, CatalogItemSearchPage, CategoryRepository,
    };
    use crate::presentation::routes::create_router;
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use ikeda_core::tenant::{ApiKeyValidator, AuthenticatedKey, HEADER_API_KEY};
    use std::sync::Arc;
    use tower::ServiceExt;

    struct DummyCatRepo;
    #[async_trait]
    impl CategoryRepository for DummyCatRepo {
        async fn create(&self, _t: Uuid, c: &Category) -> Result<Category, AppError> {
            Ok(c.clone())
        }
        async fn find_by_id(&self, _t: Uuid, _id: Uuid) -> Result<Option<Category>, AppError> {
            Ok(None)
        }
        async fn find_by_slug(&self, _t: Uuid, _s: &str) -> Result<Option<Category>, AppError> {
            Ok(None)
        }
        async fn list(&self, _t: Uuid) -> Result<Vec<Category>, AppError> {
            Ok(Vec::new())
        }
        async fn update(&self, _t: Uuid, c: &Category) -> Result<Category, AppError> {
            Ok(c.clone())
        }
        async fn soft_delete(&self, _t: Uuid, _id: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    struct DummyItemRepo;
    #[async_trait]
    impl CatalogItemRepository for DummyItemRepo {
        async fn create(&self, _t: Uuid, i: &CatalogItem) -> Result<CatalogItem, AppError> {
            Ok(i.clone())
        }
        async fn find_by_id(&self, _t: Uuid, _id: Uuid) -> Result<Option<CatalogItem>, AppError> {
            Ok(None)
        }
        async fn find_by_sku(&self, _t: Uuid, _s: &str) -> Result<Option<CatalogItem>, AppError> {
            Ok(None)
        }
        async fn find_by_ids_batch(
            &self,
            _t: Uuid,
            _ids: &[Uuid],
        ) -> Result<Vec<CatalogItem>, AppError> {
            Ok(Vec::new())
        }
        async fn search_by_criteria(
            &self,
            _t: Uuid,
            _c: &Criteria,
        ) -> Result<CatalogItemSearchPage, AppError> {
            Ok(CatalogItemSearchPage {
                items: Vec::new(),
                total_items: 0,
            })
        }
        async fn update(&self, _t: Uuid, i: &CatalogItem) -> Result<CatalogItem, AppError> {
            Ok(i.clone())
        }
        async fn soft_delete(&self, _t: Uuid, _id: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
        async fn add_variant(
            &self,
            _t: Uuid,
            v: &CatalogItemVariant,
        ) -> Result<CatalogItemVariant, AppError> {
            Ok(v.clone())
        }
        async fn find_variant_by_id(
            &self,
            _t: Uuid,
            _i: Uuid,
            _v: Uuid,
        ) -> Result<Option<CatalogItemVariant>, AppError> {
            Ok(None)
        }
        async fn update_variant(
            &self,
            _t: Uuid,
            v: &CatalogItemVariant,
        ) -> Result<CatalogItemVariant, AppError> {
            Ok(v.clone())
        }
        async fn delete_variant(&self, _t: Uuid, _i: Uuid, _v: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    struct ScopedValidator {
        scopes: Vec<String>,
    }
    #[async_trait]
    impl ApiKeyValidator for ScopedValidator {
        async fn validate_key_hash(&self, _h: &str) -> Result<Option<AuthenticatedKey>, AppError> {
            Ok(Some(AuthenticatedKey {
                tenant_id: Uuid::new_v4(),
                scopes: self.scopes.clone(),
            }))
        }
    }

    #[tokio::test]
    async fn test_staff_member_can_read_catalog_but_gets_403_on_write_and_413_on_huge_body() {
        let state = AppState::new(
            Arc::new(DummyCatRepo),
            Arc::new(DummyItemRepo),
            Arc::new(ScopedValidator {
                scopes: vec!["tenant:member".into(), "bookings:write".into()],
            }),
        );
        let app = create_router(state);

        // Read succeeds
        let read_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/categories")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read_resp.status(), StatusCode::OK);

        // Write returns 403 Forbidden
        let write_payload = serde_json::json!({
            "name": "Unauthorized Service",
            "base_price": 1000,
            "item_type": "SERVICE",
            "duration_minutes": 30
        });
        let write_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/items")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::from(write_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(write_resp.status(), StatusCode::FORBIDDEN);

        // Oversized body (>256 KiB) returns 413 Payload Too Large
        let huge = "x".repeat(300 * 1024);
        let resp413 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/items")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::from(format!(r#"{{"name":"{huge}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp413.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
