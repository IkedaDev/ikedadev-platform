use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use identity_api::application::use_cases::{
    CreateApiKeyCommand, CreateTenantCommand, CreateTenantUserCommand, UpdateTenantCommand,
};
use identity_api::infrastructure::{
    MockIdentityProvider, SqlxApiKeyRepository, SqlxApiKeyValidator, SqlxTenantRepository,
    SqlxTenantUserRepository,
};
use identity_api::presentation::{create_router, ApiDoc, AppState};
use ikeda_core::tenant::{hash_api_key, HEADER_API_KEY};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_identity_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Identity API");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/tenants"));
    assert!(openapi.paths.paths.contains_key("/v1/tenants/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/api-keys"));
    assert!(openapi.paths.paths.contains_key("/v1/api-keys/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/users"));
    assert!(openapi.paths.paths.contains_key("/v1/users/{id}"));
}

#[tokio::test]
async fn test_identity_scopes_api_keys_users_and_rls_on_postgres() {
    let db_url = std::env::var("IDENTITY_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/identity_db?sslmode=disable".to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL identity_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let mock_idp = Arc::new(MockIdentityProvider::default());
    let state = AppState::new(
        Arc::new(SqlxTenantRepository::new(pool.clone())),
        Arc::new(SqlxTenantUserRepository::new(pool.clone())),
        Arc::new(SqlxApiKeyRepository::new(pool.clone())),
        mock_idp.clone(),
        Arc::new(SqlxApiKeyValidator::new(pool.clone())),
    );
    let app = create_router(state);

    // Bootstrap a platform-admin tenant + API key directly in identity_db
    let platform_tenant_id = Uuid::new_v4();
    let platform_slug = format!("platform-{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Platform Root')")
        .bind(platform_tenant_id)
        .bind(&platform_slug)
        .execute(&pool)
        .await
        .expect("Failed to insert platform tenant");

    let platform_admin_key = format!("ik_plat_{}", Uuid::new_v4().simple());
    sqlx::query(
        r#"
        INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
        VALUES ($1, 'Platform Admin Bootstrap', 'ik_plat_boot', $2, '{"platform:admin"}', true)
        "#,
    )
    .bind(platform_tenant_id)
    .bind(hash_api_key(&platform_admin_key))
    .execute(&pool)
    .await
    .expect("Failed to insert platform admin key");

    // 1. Provision Tenant A and Tenant B via POST /v1/tenants (with platform:admin)
    let slug_a = format!("tenant-a-{}", Uuid::new_v4().simple());
    let res_ta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tenants")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &platform_admin_key)
                .body(Body::from(
                    serde_json::to_string(&CreateTenantCommand {
                        slug: slug_a,
                        name: "Tenant A Corp".to_string(),
                        config: Some(serde_json::json!({ "tier": "enterprise" })),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ta.status(), StatusCode::CREATED);
    let ta_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_ta.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let tenant_a_id = Uuid::parse_str(ta_json["data"]["id"].as_str().unwrap()).unwrap();

    // Patch Tenant A
    let res_patch_ta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/tenants/{tenant_a_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &platform_admin_key)
                .body(Body::from(
                    serde_json::to_string(&UpdateTenantCommand {
                        name: Some("Tenant A Updated".to_string()),
                        ..Default::default()
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_patch_ta.status(), StatusCode::OK);

    // Create Tenant B
    let slug_b = format!("tenant-b-{}", Uuid::new_v4().simple());
    let res_tb = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tenants")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &platform_admin_key)
                .body(Body::from(
                    serde_json::to_string(&CreateTenantCommand {
                        slug: slug_b,
                        name: "Tenant B Corp".to_string(),
                        config: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_tb.status(), StatusCode::CREATED);
    let tb_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_tb.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let tenant_b_id = Uuid::parse_str(tb_json["data"]["id"].as_str().unwrap()).unwrap();

    // Bootstrap tenant:admin keys for Tenant A and Tenant B
    let key_a_admin = format!("ik_ta_{}", Uuid::new_v4().simple());
    let key_b_admin = format!("ik_tb_{}", Uuid::new_v4().simple());
    for (tid, k) in [(tenant_a_id, &key_a_admin), (tenant_b_id, &key_b_admin)] {
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, 'Tenant Admin Key', 'ik_t_admin', $2, '{"tenant:admin"}', true)
            "#,
        )
        .bind(tid)
        .bind(hash_api_key(k))
        .execute(&pool)
        .await
        .expect("Failed to insert tenant admin key");
    }

    // 2. Tenant A (without platform:admin) CANNOT create a tenant -> 403 Forbidden
    let res_forb = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tenants")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&CreateTenantCommand {
                        slug: "unauthorized-tenant".to_string(),
                        name: "Should Fail".to_string(),
                        config: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forb.status(), StatusCode::FORBIDDEN);

    // 3. Tenant A mints a new API key via POST /v1/api-keys, uses it, then revokes it
    let res_mint = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&CreateApiKeyCommand {
                        name: "Minted Runtime Key".to_string(),
                        scopes: vec!["tenant:admin".to_string()],
                        expires_at: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_mint.status(), StatusCode::CREATED);
    let mint_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_mint.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let minted_id = Uuid::parse_str(mint_json["data"]["key"]["id"].as_str().unwrap()).unwrap();
    let minted_raw = mint_json["data"]["raw_key"].as_str().unwrap().to_string();

    // Authenticate with the newly minted key -> 200 OK
    let res_use_minted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/api-keys")
                .header(HEADER_API_KEY, &minted_raw)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_use_minted.status(), StatusCode::OK);

    // Revoke minted key -> 200 OK
    let res_revoke = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/api-keys/{minted_id}"))
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_revoke.status(), StatusCode::OK);

    // Using revoked key now returns 401 Unauthorized!
    let res_revoked_use = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/api-keys")
                .header(HEADER_API_KEY, &minted_raw)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_revoked_use.status(), StatusCode::UNAUTHORIZED);

    // 4. Tenant A provisions a user -> synced with MockIdentityProvider + tenant_users (RLS)
    let res_user = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/users")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&CreateTenantUserCommand {
                        email: format!("user-{}@tenanta.cl", Uuid::new_v4().simple()),
                        full_name: "María López".to_string(),
                        temporary_password: None,
                        roles: vec!["staff".to_string()],
                        send_invite_email: true,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_user.status(), StatusCode::CREATED);
    let user_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_user.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let user_a_id = Uuid::parse_str(user_json["data"]["id"].as_str().unwrap()).unwrap();
    let kc_user_a_id =
        Uuid::parse_str(user_json["data"]["keycloak_user_id"].as_str().unwrap()).unwrap();

    // Tenant B lists users -> sees 0 (RLS isolation)
    let res_b_users = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/users")
                .header(HEADER_API_KEY, &key_b_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_users.status(), StatusCode::OK);
    let b_users_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_b_users.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(b_users_json["data"].as_array().unwrap().len(), 0);

    // Tenant B attempts to delete Tenant A's user -> 404 Not Found
    let res_b_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/users/{user_a_id}"))
                .header(HEADER_API_KEY, &key_b_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_del.status(), StatusCode::NOT_FOUND);

    // Tenant A deletes its own user -> disabled in Keycloak + soft-deleted in DB
    let res_a_del = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/users/{user_a_id}"))
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_a_del.status(), StatusCode::OK);
    assert!(mock_idp
        .disabled_users
        .lock()
        .unwrap()
        .contains(&kc_user_a_id));
}
