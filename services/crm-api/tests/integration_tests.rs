use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use crm_api::application::use_cases::{CreateClientCommand, UpdateClientCommand};
use crm_api::infrastructure::{SqlxApiKeyValidator, SqlxClientRepository};
use crm_api::presentation::{create_router, ApiDoc, AppState};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, Pagination,
};
use ikeda_core::tenant::{hash_api_key, HEADER_API_KEY, HEADER_TENANT_ID};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_crm_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev CRM API");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/clients"));
    assert!(openapi.paths.paths.contains_key("/v1/clients/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/clients/search"));
}

#[tokio::test]
async fn test_crm_rls_isolation_tax_id_uniqueness_and_criteria_search() {
    let db_url = std::env::var("CRM_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/crm_db?sslmode=disable".to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL crm_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let raw_key_a = format!("ikeda_crm_a_{}", Uuid::new_v4().simple());
    let raw_key_b = format!("ikeda_crm_b_{}", Uuid::new_v4().simple());

    // Insert real API keys into crm_db.api_keys
    for (tenant_id, name, raw_key) in [
        (tenant_a, "Tenant A CRM Key", &raw_key_a),
        (tenant_b, "Tenant B CRM Key", &raw_key_b),
    ] {
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ikeda_crm', $3, '{"*"}', true)
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .execute(&pool)
        .await
        .expect("Failed to insert CRM API key");
    }

    let state = AppState::new(
        Arc::new(SqlxClientRepository::new(pool.clone())),
        Arc::new(SqlxApiKeyValidator::new(pool)),
    );
    let app = create_router(state);

    // 1. Tenant A creates Client 1 (VIP, Santiago) with tax_id
    let shared_tax_id = format!("12.{:03}.{:03}-9", 100, 200);
    let c1_cmd = CreateClientCommand {
        full_name: "Sofía González".to_string(),
        email: Some("sofia@anami.cl".to_string()),
        phone: Some("+56911112222".to_string()),
        tax_id: Some(shared_tax_id.clone()),
        address: Some("Las Condes 100".to_string()),
        notes: Some("Cliente frecuente".to_string()),
        keycloak_user_id: None,
        is_active: Some(true),
        metadata: Some(serde_json::json!({ "tier": "vip", "city": "Santiago" })),
    };

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::from(serde_json::to_string(&c1_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let client_a1_id = Uuid::parse_str(json["data"]["id"].as_str().unwrap()).unwrap();

    // 2. Duplicate tax_id within Tenant A -> 409 Conflict
    let dup_cmd = CreateClientCommand {
        full_name: "Duplicado".to_string(),
        email: Some("otro@anami.cl".to_string()),
        phone: None,
        tax_id: Some(shared_tax_id.clone()),
        address: None,
        notes: None,
        keycloak_user_id: None,
        is_active: None,
        metadata: None,
    };
    let res_dup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::from(serde_json::to_string(&dup_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_dup.status(), StatusCode::CONFLICT);

    // 3. Tenant A creates Client 2 (Standard, Valparaíso)
    let c2_cmd = CreateClientCommand {
        full_name: "Carlos Muñoz".to_string(),
        email: Some("carlos@anami.cl".to_string()),
        phone: None,
        tax_id: Some("18.765.432-1".to_string()),
        address: None,
        notes: None,
        keycloak_user_id: None,
        is_active: Some(true),
        metadata: Some(serde_json::json!({ "tier": "standard", "city": "Valparaiso" })),
    };
    let res_c2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::from(serde_json::to_string(&c2_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_c2.status(), StatusCode::CREATED);

    // 4. Tenant B creates a client with the EXACT SAME tax_id -> 201 Created (isolated by tenant!)
    let c_b_cmd = CreateClientCommand {
        full_name: "Sofía Tenant B".to_string(),
        email: Some("sofia@anami.cl".to_string()),
        phone: None,
        tax_id: Some(shared_tax_id.clone()),
        address: None,
        notes: None,
        keycloak_user_id: None,
        is_active: Some(true),
        metadata: Some(serde_json::json!({ "tier": "vip", "city": "Santiago" })),
    };
    let res_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_b)
                .body(Body::from(serde_json::to_string(&c_b_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b.status(), StatusCode::CREATED);

    // 5. Tenant B CANNOT read, update, or delete Tenant A's Client 1 -> 404 Not Found (RLS)
    let res_b_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/clients/{client_a1_id}"))
                .header(HEADER_API_KEY, &raw_key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_get.status(), StatusCode::NOT_FOUND);

    let res_b_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/clients/{client_a1_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_b)
                .body(Body::from(
                    serde_json::to_string(&UpdateClientCommand {
                        full_name: Some("Hacked".to_string()),
                        ..Default::default()
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_patch.status(), StatusCode::NOT_FOUND);

    let res_b_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/clients/{client_a1_id}"))
                .header(HEADER_API_KEY, &raw_key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_del.status(), StatusCode::NOT_FOUND);

    // 6. Recursive Criteria Search on standard column + JSONB metadata field
    let criteria = Criteria::new()
        .with_filter(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition::new(
                "full_name",
                FilterOperator::Contains,
                serde_json::json!("Sofía"),
            )),
            FilterRule::Condition(FilterCondition::new(
                "metadata.tier",
                FilterOperator::Eq,
                serde_json::json!("vip"),
            )),
        ]))
        .with_order_by(vec![OrderBy::asc("full_name")])
        .with_pagination(Pagination::new(1, 10));

    let res_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::from(serde_json::to_string(&criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search.status(), StatusCode::OK);
    let search_bytes = axum::body::to_bytes(res_search.into_body(), usize::MAX)
        .await
        .unwrap();
    let search_json: serde_json::Value = serde_json::from_slice(&search_bytes).unwrap();
    let items = search_json["data"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], client_a1_id.to_string());
    assert_eq!(search_json["pagination"]["total_items"], 1);

    // 7. Tenant A soft-deletes Client 1, then re-creates with same tax_id -> 201 Created
    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/clients/{client_a1_id}"))
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);

    let res_recreate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/clients")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &raw_key_a)
                .body(Body::from(serde_json::to_string(&c1_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_recreate.status(), StatusCode::CREATED);

    // 8. Without API key, plain X-Tenant-Id is rejected with 401 Unauthorized
    let res_unauth = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/clients/{client_a1_id}"))
                .header(HEADER_TENANT_ID, tenant_a.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);
}
