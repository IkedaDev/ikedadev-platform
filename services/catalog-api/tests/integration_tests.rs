use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use catalog_api::application::use_cases::{
    BatchCatalogItemsQuery, CreateCatalogItemCommand, CreateCategoryCommand, CreateVariantCommand,
};
use catalog_api::domain::models::{BundleComponent, ItemDetails};
use catalog_api::infrastructure::{
    SqlxApiKeyValidator, SqlxCatalogItemRepository, SqlxCategoryRepository,
};
use catalog_api::presentation::{create_router, ApiDoc, AppState};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, Pagination,
};
use ikeda_core::tenant::{hash_api_key, CachedApiKeyValidator, HEADER_API_KEY};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_catalog_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Catalog API");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/categories"));
    assert!(openapi.paths.paths.contains_key("/v1/categories/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/items"));
    assert!(openapi.paths.paths.contains_key("/v1/items/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/items/search"));
    assert!(openapi.paths.paths.contains_key("/v1/items/batch"));
    assert!(openapi.paths.paths.contains_key("/v1/items/{id}/variants"));
    assert!(openapi
        .paths
        .paths
        .contains_key("/v1/items/{id}/variants/{variant_id}"));
}

#[tokio::test]
async fn test_catalog_rls_criteria_batch_and_soft_delete_on_postgres() {
    let db_url = std::env::var("CATALOG_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/catalog_db?sslmode=disable"
            .to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL catalog_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let key_a = format!("ik_cat_a_{}", Uuid::new_v4().simple());
    let key_b = format!("ik_cat_b_{}", Uuid::new_v4().simple());

    for (tid, name, raw_key) in [
        (tenant_a, "Tenant A Catalog Key", &key_a),
        (tenant_b, "Tenant B Catalog Key", &key_b),
    ] {
        let mut tx = pool.begin().await.expect("Failed to begin catalog key tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tid.to_string())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ik_cat_key', $3, '{"*"}', true)
            "#,
        )
        .bind(tid)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .execute(&mut *tx)
        .await
        .expect("Failed to insert catalog API key");
        tx.commit().await.expect("Failed to commit catalog key tx");
    }

    let state = AppState::new(
        Arc::new(SqlxCategoryRepository::new(pool.clone())),
        Arc::new(SqlxCatalogItemRepository::new(pool.clone())),
        Arc::new(CachedApiKeyValidator::new(SqlxApiKeyValidator::new(pool))),
    );
    let app = create_router(state);

    // 1. Tenant A creates a Category
    let res_cat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/categories")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateCategoryCommand {
                        parent_id: None,
                        name: "Terapias Spa".to_string(),
                        slug: format!("spa-{}", Uuid::new_v4().simple()),
                        description: None,
                        display_order: 1,
                        is_active: Some(true),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_cat.status(), StatusCode::CREATED);
    let cat_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_cat.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let cat_id = Uuid::parse_str(cat_json["data"]["id"].as_str().unwrap()).unwrap();

    // 2. Tenant A creates SERVICE (35_000 CLP, metadata.featured = "true")
    let svc_sku = format!("SVC-{}", Uuid::new_v4().simple());
    let res_svc = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateCatalogItemCommand {
                        category_id: Some(cat_id),
                        sku: Some(svc_sku.clone()),
                        name: "Masaje Piedras Calientes".to_string(),
                        description: Some("Relajación profunda".to_string()),
                        image_url: Some(
                            "https://cdn.kropy.cl/catalog/masaje-piedras.webp".to_string(),
                        ),
                        gallery_urls: vec![
                            "https://cdn.kropy.cl/catalog/masaje-1.webp".to_string(),
                            "https://cdn.kropy.cl/catalog/masaje-2.webp".to_string(),
                        ],
                        base_price: 35_000,
                        currency: Some("CLP".to_string()),
                        tax_rate_bps: Some(1900),
                        tax_included: Some(true),
                        details: ItemDetails::Service {
                            duration_minutes: 60,
                            buffer_minutes: 15,
                        },
                        is_active: Some(true),
                        metadata: Some(serde_json::json!({ "featured": "true", "room": "zen" })),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_svc.status(), StatusCode::CREATED);
    let svc_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_svc.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let svc_id = Uuid::parse_str(svc_json["data"]["id"].as_str().unwrap()).unwrap();

    // Add a 90-min variant to SERVICE
    let res_var = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/items/{svc_id}/variants"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateVariantCommand {
                        name: "90 minutos".to_string(),
                        sku: Some(format!("VAR-{}", Uuid::new_v4().simple())),
                        price_override: Some(48_000),
                        duration_override: Some(90),
                        is_active: Some(true),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_var.status(), StatusCode::CREATED);

    // 3. Tenant A creates PRODUCT (15_000 CLP) & BUNDLE (45_000 CLP)
    let res_prod = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateCatalogItemCommand {
                        category_id: Some(cat_id),
                        sku: Some(format!("PRD-{}", Uuid::new_v4().simple())),
                        name: "Crema Hidratante".to_string(),
                        description: None,
                        image_url: None,
                        gallery_urls: vec![],
                        base_price: 15_000,
                        currency: Some("CLP".to_string()),
                        tax_rate_bps: Some(1900),
                        tax_included: Some(true),
                        details: ItemDetails::Product {
                            unit_of_measure: Some("UNIT".to_string()),
                            track_inventory: true,
                        },
                        is_active: Some(true),
                        metadata: Some(serde_json::json!({ "featured": "false" })),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_prod.status(), StatusCode::CREATED);
    let prod_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_prod.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let prod_id = Uuid::parse_str(prod_json["data"]["id"].as_str().unwrap()).unwrap();

    let res_bundle = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateCatalogItemCommand {
                        category_id: Some(cat_id),
                        sku: Some(format!("BND-{}", Uuid::new_v4().simple())),
                        name: "Pack Spa Premium".to_string(),
                        description: None,
                        image_url: None,
                        gallery_urls: vec![],
                        base_price: 45_000,
                        currency: Some("CLP".to_string()),
                        tax_rate_bps: Some(1900),
                        tax_included: Some(true),
                        details: ItemDetails::Bundle {
                            items: vec![
                                BundleComponent {
                                    item_id: svc_id,
                                    quantity: 1,
                                },
                                BundleComponent {
                                    item_id: prod_id,
                                    quantity: 1,
                                },
                            ],
                        },
                        is_active: Some(true),
                        metadata: Some(serde_json::json!({ "featured": "true" })),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_bundle.status(), StatusCode::CREATED);
    let bundle_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_bundle.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let bundle_id = Uuid::parse_str(bundle_json["data"]["id"].as_str().unwrap()).unwrap();

    // 4. RLS Isolation: Tenant B CANNOT read Tenant A's item or batch-load it
    let res_b_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/items/{svc_id}"))
                .header(HEADER_API_KEY, &key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_get.status(), StatusCode::NOT_FOUND);

    let res_b_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items/batch")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_b)
                .body(Body::from(
                    serde_json::to_string(&BatchCatalogItemsQuery {
                        ids: vec![svc_id, prod_id, bundle_id],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_batch.status(), StatusCode::OK);
    let b_batch_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_b_batch.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(b_batch_json["data"].as_array().unwrap().len(), 0);

    // 5. Tenant A Criteria search: metadata.featured == "true" AND base_price between 30_000 and 40_000
    let criteria = Criteria::new()
        .with_filter(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition::new(
                "metadata.featured",
                FilterOperator::Eq,
                serde_json::json!("true"),
            )),
            FilterRule::Condition(FilterCondition::new(
                "base_price",
                FilterOperator::Gte,
                serde_json::json!(30_000),
            )),
            FilterRule::Condition(FilterCondition::new(
                "base_price",
                FilterOperator::Lte,
                serde_json::json!(40_000),
            )),
        ]))
        .with_order_by(vec![OrderBy::asc("base_price")])
        .with_pagination(Pagination::new(1, 10));

    let res_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search.status(), StatusCode::OK);
    let search_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let found_items = search_json["data"].as_array().unwrap();
    assert_eq!(found_items.len(), 1);
    assert_eq!(found_items[0]["id"], svc_id.to_string());
    assert_eq!(
        found_items[0]["image_url"],
        "https://cdn.kropy.cl/catalog/masaje-piedras.webp"
    );
    assert_eq!(found_items[0]["gallery_urls"].as_array().unwrap().len(), 2);
    assert_eq!(found_items[0]["variants"].as_array().unwrap().len(), 1);

    // 6. Batch lookup by Tenant A returns svc + bundle hydrated (including image fields)
    let res_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items/batch")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&BatchCatalogItemsQuery {
                        ids: vec![svc_id, bundle_id],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_batch.status(), StatusCode::OK);
    let batch_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_batch.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let batch_items = batch_json["data"].as_array().unwrap();
    assert_eq!(batch_items.len(), 2);
    let batch_svc = batch_items
        .iter()
        .find(|it| it["id"] == svc_id.to_string())
        .unwrap();
    assert_eq!(
        batch_svc["image_url"],
        "https://cdn.kropy.cl/catalog/masaje-piedras.webp"
    );
    assert_eq!(batch_svc["gallery_urls"].as_array().unwrap().len(), 2);

    // 6b. PATCH clears image_url to null and updates gallery_urls
    let res_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/items/{svc_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::json!({
                        "image_url": null,
                        "gallery_urls": ["https://cdn.kropy.cl/catalog/masaje-3.webp"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_patch.status(), StatusCode::OK);
    let patch_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_patch.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(patch_json["data"]["image_url"].is_null());
    assert_eq!(
        patch_json["data"]["gallery_urls"],
        serde_json::json!(["https://cdn.kropy.cl/catalog/masaje-3.webp"])
    );

    // 7. Soft Delete: delete svc_id -> disappears from GET and search
    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/items/{svc_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);

    let res_get_deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/items/{svc_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_deleted.status(), StatusCode::NOT_FOUND);

    let res_search_after = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/items/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search_after.status(), StatusCode::OK);
    let search_after_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search_after.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_after_json["data"].as_array().unwrap().len(), 0);
}
