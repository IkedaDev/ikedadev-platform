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
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/identity_db?sslmode=disable"
            .to_string()
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

    // Bootstrap a platform-admin tenant + API key directly in identity_db (RLS-safe under NOBYPASSRLS)
    let platform_tenant_id = Uuid::new_v4();
    let platform_slug = format!("platform-{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Platform Root')")
        .bind(platform_tenant_id)
        .bind(&platform_slug)
        .execute(&pool)
        .await
        .expect("Failed to insert platform tenant");

    let platform_admin_key = format!("ik_plat_{}", Uuid::new_v4().simple());
    {
        let mut tx = pool.begin().await.expect("Failed to begin bootstrap tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(platform_tenant_id.to_string())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, 'Platform Admin Bootstrap', 'ik_plat_boot', $2, '{"platform:admin"}', true)
            "#,
        )
        .bind(platform_tenant_id)
        .bind(hash_api_key(&platform_admin_key))
        .execute(&mut *tx)
        .await
        .expect("Failed to insert platform admin key");
        tx.commit().await.expect("Failed to commit bootstrap tx");
    }

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

    // Bootstrap tenant:admin keys for Tenant A and Tenant B (RLS-safe under NOBYPASSRLS)
    let key_a_admin = format!("ik_ta_{}", Uuid::new_v4().simple());
    let key_b_admin = format!("ik_tb_{}", Uuid::new_v4().simple());
    for (tid, k) in [(tenant_a_id, &key_a_admin), (tenant_b_id, &key_b_admin)] {
        let mut tx = pool.begin().await.expect("Failed to begin tenant key tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tid.to_string())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, 'Tenant Admin Key', 'ik_t_admin', $2, '{"tenant:admin"}', true)
            "#,
        )
        .bind(tid)
        .bind(hash_api_key(k))
        .execute(&mut *tx)
        .await
        .expect("Failed to insert tenant admin key");
        tx.commit().await.expect("Failed to commit tenant key tx");
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

#[tokio::test]
async fn test_identity_rbac_privilege_escalation_and_cross_tenant_isolation() {
    use async_trait::async_trait;
    use identity_api::domain::models::{ApiKeyRecord, ApiKeySummary, Tenant, TenantUser};
    use identity_api::domain::repositories::{
        ApiKeyRepository, TenantRepository, TenantUserRepository,
    };
    use ikeda_core::error::AppError;
    use ikeda_core::tenant::{ApiKeyValidator, AuthenticatedKey};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemTenantRepo(Mutex<Vec<Tenant>>);
    #[async_trait]
    impl TenantRepository for InMemTenantRepo {
        async fn create(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
            self.0.lock().unwrap().push(tenant.clone());
            Ok(tenant.clone())
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Tenant>, AppError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id == id && t.deleted_at.is_none())
                .cloned())
        }
        async fn find_by_slug(&self, slug: &str) -> Result<Option<Tenant>, AppError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.slug == slug && t.deleted_at.is_none())
                .cloned())
        }
        async fn update(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
            let mut guard = self.0.lock().unwrap();
            if let Some(slot) = guard.iter_mut().find(|t| t.id == tenant.id) {
                *slot = tenant.clone();
                return Ok(tenant.clone());
            }
            Err(AppError::NotFound("not found".to_string()))
        }
    }

    #[derive(Default)]
    struct InMemUserRepo(Mutex<Vec<TenantUser>>);
    #[async_trait]
    impl TenantUserRepository for InMemUserRepo {
        async fn create(
            &self,
            _tenant_id: Uuid,
            user: &TenantUser,
        ) -> Result<TenantUser, AppError> {
            self.0.lock().unwrap().push(user.clone());
            Ok(user.clone())
        }
        async fn find_by_id(
            &self,
            tenant_id: Uuid,
            id: Uuid,
        ) -> Result<Option<TenantUser>, AppError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.tenant_id == tenant_id && u.id == id && u.deleted_at.is_none())
                .cloned())
        }
        async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|u| u.tenant_id == tenant_id && u.deleted_at.is_none())
                .cloned()
                .collect())
        }
        async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
            let mut guard = self.0.lock().unwrap();
            if let Some(u) = guard
                .iter_mut()
                .find(|u| u.tenant_id == tenant_id && u.id == id && u.deleted_at.is_none())
            {
                u.deleted_at = Some(chrono::Utc::now());
                u.is_active = false;
                return Ok(true);
            }
            Ok(false)
        }
    }

    #[derive(Default)]
    struct InMemApiKeyRepo(Mutex<Vec<ApiKeySummary>>);
    #[async_trait]
    impl ApiKeyRepository for InMemApiKeyRepo {
        async fn create(
            &self,
            tenant_id: Uuid,
            record: &ApiKeyRecord,
        ) -> Result<ApiKeySummary, AppError> {
            let summary = ApiKeySummary {
                id: record.id,
                tenant_id,
                name: record.name.clone(),
                key_prefix: record.key_prefix.clone(),
                scopes: record.scopes.clone(),
                is_active: record.is_active,
                expires_at: record.expires_at,
                last_used_at: record.last_used_at,
                created_at: record.created_at,
                revoked_at: record.revoked_at,
            };
            self.0.lock().unwrap().push(summary.clone());
            Ok(summary)
        }
        async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|k| k.tenant_id == tenant_id)
                .cloned()
                .collect())
        }
        async fn revoke(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
            let mut guard = self.0.lock().unwrap();
            if let Some(k) = guard
                .iter_mut()
                .find(|k| k.tenant_id == tenant_id && k.id == id && k.revoked_at.is_none())
            {
                k.revoked_at = Some(chrono::Utc::now());
                k.is_active = false;
                return Ok(true);
            }
            Ok(false)
        }
    }

    struct StaticKeyValidator(HashMap<String, AuthenticatedKey>);
    #[async_trait]
    impl ApiKeyValidator for StaticKeyValidator {
        async fn validate_key_hash(
            &self,
            key_hash: &str,
        ) -> Result<Option<AuthenticatedKey>, AppError> {
            Ok(self.0.get(key_hash).cloned())
        }
    }

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let mut keys = HashMap::new();
    keys.insert(
        hash_api_key("key-a-admin"),
        AuthenticatedKey {
            tenant_id: tenant_a,
            scopes: vec!["tenant:admin".to_string()],
        },
    );
    keys.insert(
        hash_api_key("key-a-staff"),
        AuthenticatedKey {
            tenant_id: tenant_a,
            scopes: vec!["tenant:member".to_string()],
        },
    );
    keys.insert(
        hash_api_key("key-b-admin"),
        AuthenticatedKey {
            tenant_id: tenant_b,
            scopes: vec!["tenant:admin".to_string()],
        },
    );

    let state = AppState::new(
        Arc::new(InMemTenantRepo::default()),
        Arc::new(InMemUserRepo::default()),
        Arc::new(InMemApiKeyRepo::default()),
        Arc::new(MockIdentityProvider::default()),
        Arc::new(StaticKeyValidator(keys)),
    );
    let app = create_router(state);

    // 1. Staff (tenant:member) CANNOT create users -> 403 Forbidden
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/users")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, "key-a-staff")
                .body(Body::from(
                    serde_json::to_string(&CreateTenantUserCommand {
                        email: "staff-attempt@a.cl".to_string(),
                        full_name: "Staff Attempt".to_string(),
                        temporary_password: None,
                        roles: vec!["tenant:member".to_string()],
                        send_invite_email: false,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 2. tenant:admin CANNOT escalate to platform:admin when creating a user -> 403 Forbidden
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/users")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, "key-a-admin")
                .body(Body::from(
                    serde_json::to_string(&CreateTenantUserCommand {
                        email: "escalate@a.cl".to_string(),
                        full_name: "Escalation Attempt".to_string(),
                        temporary_password: None,
                        roles: vec!["platform:admin".to_string()],
                        send_invite_email: false,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. tenant:admin CANNOT mint an API key with platform:admin or wildcard scope -> 403 Forbidden
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, "key-a-admin")
                .body(Body::from(
                    serde_json::to_string(&CreateApiKeyCommand {
                        name: "Evil Key".to_string(),
                        scopes: vec!["platform:admin".to_string()],
                        expires_at: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 4. tenant:admin creates a valid user in Tenant A -> 201 Created
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/users")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, "key-a-admin")
                .body(Body::from(
                    serde_json::to_string(&CreateTenantUserCommand {
                        email: "valid@a.cl".to_string(),
                        full_name: "Valid User A".to_string(),
                        temporary_password: None,
                        roles: vec!["tenant:member".to_string()],
                        send_invite_email: false,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let user_a_id = Uuid::parse_str(body["data"]["id"].as_str().unwrap()).unwrap();

    // 5. Staff CANNOT delete users -> 403 Forbidden
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/users/{user_a_id}"))
                .header(HEADER_API_KEY, "key-a-staff")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 6. Tenant B admin sees 0 users and gets 404 when trying to delete Tenant A's user
    let res_list_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/users")
                .header(HEADER_API_KEY, "key-b-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list_b.status(), StatusCode::OK);
    let list_b_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_list_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(list_b_body["data"].as_array().unwrap().len(), 0);

    let res_del_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/users/{user_a_id}"))
                .header(HEADER_API_KEY, "key-b-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_b.status(), StatusCode::NOT_FOUND);

    // 7. tenant:admin CANNOT read or patch /v1/tenants/{id} directly -> 403 Forbidden
    let res_get_t = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/tenants/{tenant_a}"))
                .header(HEADER_API_KEY, "key-a-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_t.status(), StatusCode::FORBIDDEN);
}
