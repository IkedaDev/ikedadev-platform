use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, Pagination,
};
use ikeda_core::error::AppError;
use ikeda_core::tenant::{hash_api_key, CachedApiKeyValidator, HEADER_API_KEY};
use media_api::application::use_cases::{
    ConfirmMediaCommand, PresignMediaCommand, UpdateMediaCommand,
};
use media_api::domain::models::{MediaContext, TenantQuotaLimits};
use media_api::domain::ports::{GeneratedPresignedPut, ObjectStoragePort};
use media_api::infrastructure::{SqlxApiKeyValidator, SqlxMediaRepository};
use media_api::presentation::{create_router, ApiDoc, AppState};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

struct SpyObjectStorage {
    deleted_keys: Mutex<Vec<String>>,
}

impl SpyObjectStorage {
    fn new() -> Self {
        Self {
            deleted_keys: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ObjectStoragePort for SpyObjectStorage {
    fn provider_name(&self) -> &'static str {
        "R2_PLATFORM"
    }

    async fn generate_presigned_put(
        &self,
        storage_key: &str,
        _mime_type: &str,
        _size_bytes: i64,
    ) -> Result<GeneratedPresignedPut, AppError> {
        let public_url = format!("https://cdn.test.ikedadev.com/{storage_key}");
        Ok(GeneratedPresignedPut {
            upload_url: format!("{public_url}?sig=test"),
            public_url,
            expires_in_secs: 900,
        })
    }

    async fn delete_object(&self, storage_key: &str) -> Result<(), AppError> {
        self.deleted_keys
            .lock()
            .unwrap()
            .push(storage_key.to_string());
        Ok(())
    }
}

#[test]
fn test_media_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Media API");
    for path in [
        "/health",
        "/v1/media/presign",
        "/v1/media/confirm",
        "/v1/media",
        "/v1/media/search",
        "/v1/media/quota",
        "/v1/media/{id}",
        "/v1/media/{id}/restore",
    ] {
        assert!(
            openapi.paths.paths.contains_key(path),
            "Missing OpenAPI path: {path}"
        );
    }
}

#[tokio::test]
async fn test_media_rls_rbac_quotas_lifecycle_and_criteria_on_postgres() {
    let db_url = std::env::var("MEDIA_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/media_db?sslmode=disable"
            .to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL media_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let key_a_admin = format!("ik_med_a_adm_{}", Uuid::new_v4().simple());
    let key_a_staff = format!("ik_med_a_stf_{}", Uuid::new_v4().simple());
    let key_b_admin = format!("ik_med_b_adm_{}", Uuid::new_v4().simple());

    for (tid, name, raw_key, scopes) in [
        (
            tenant_a,
            "Tenant A Admin",
            &key_a_admin,
            vec!["tenant:admin", "media:read", "media:write", "media:delete"],
        ),
        (
            tenant_a,
            "Tenant A Staff",
            &key_a_staff,
            vec!["tenant:member", "media:read", "media:write"],
        ),
        (
            tenant_b,
            "Tenant B Admin",
            &key_b_admin,
            vec!["tenant:admin", "media:read", "media:write", "media:delete"],
        ),
    ] {
        let mut tx = pool.begin().await.expect("begin key tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tid.to_string())
            .execute(&mut *tx)
            .await
            .expect("set RLS");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ik_med_key', $3, $4, true)
            "#,
        )
        .bind(tid)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .bind(&scopes)
        .execute(&mut *tx)
        .await
        .expect("insert api key");
        tx.commit().await.expect("commit key tx");
    }

    let spy_storage = Arc::new(SpyObjectStorage::new());
    let state = AppState::new(
        Arc::new(SqlxMediaRepository::new(pool.clone())),
        spy_storage.clone(),
        Arc::new(CachedApiKeyValidator::new(SqlxApiKeyValidator::new(pool))),
    )
    .with_default_quotas(TenantQuotaLimits {
        max_storage_bytes: 500_000, // 500 KB for quota exhaustion test
        max_file_size_bytes: 300_000,
    });
    let app = create_router(state);

    // 1. Unauthenticated request -> 401
    let res_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/media")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);

    // 2. Staff CAN presign + confirm in `content` context
    let res_presign = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/presign")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_staff)
                .body(Body::from(
                    serde_json::to_string(&PresignMediaCommand {
                        file_name: "../../evil/hero.webp".to_string(),
                        mime_type: "image/webp".to_string(),
                        size_bytes: 200_000,
                        context: Some(MediaContext::Content),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_presign.status(), StatusCode::OK);
    let presign_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_presign.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let asset_id = Uuid::parse_str(presign_json["data"]["asset_id"].as_str().unwrap()).unwrap();
    let storage_key = presign_json["data"]["storage_key"].as_str().unwrap();
    assert!(storage_key.starts_with(&format!("tenants/{tenant_a}/content/")));
    assert!(!storage_key.contains(".."));

    let res_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/confirm")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_staff)
                .body(Body::from(
                    serde_json::to_string(&ConfirmMediaCommand {
                        asset_id,
                        alt_text: Some("Portada Blog".to_string()),
                        width: Some(1200),
                        height: Some(630),
                        metadata: Some(serde_json::json!({ "campaign": "launch" })),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_confirm.status(), StatusCode::OK);

    // 3. RBAC: Staff CANNOT presign `branding`, patch to `branding`, delete, permanent delete, or restore -> 403
    let res_staff_branding = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/presign")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_staff)
                .body(Body::from(
                    serde_json::to_string(&PresignMediaCommand {
                        file_name: "logo.png".to_string(),
                        mime_type: "image/png".to_string(),
                        size_bytes: 50_000,
                        context: Some(MediaContext::Branding),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_staff_branding.status(), StatusCode::FORBIDDEN);

    let res_staff_patch_branding = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/media/{asset_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_staff)
                .body(Body::from(
                    serde_json::to_string(&UpdateMediaCommand {
                        context: Some(MediaContext::Branding),
                        ..Default::default()
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_staff_patch_branding.status(), StatusCode::FORBIDDEN);

    for (method, uri) in [
        ("DELETE", format!("/v1/media/{asset_id}")),
        ("DELETE", format!("/v1/media/{asset_id}?permanent=true")),
        ("POST", format!("/v1/media/{asset_id}/restore")),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .header(HEADER_API_KEY, &key_a_staff)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "Staff must get 403 on {method} {uri}"
        );
    }

    // 4. Tenant Admin CAN upload to `branding`
    let res_admin_branding = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/presign")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&PresignMediaCommand {
                        file_name: "logo.png".to_string(),
                        mime_type: "image/png".to_string(),
                        size_bytes: 150_000,
                        context: Some(MediaContext::Branding),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_admin_branding.status(), StatusCode::OK);
    let b_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_admin_branding.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let branding_id = Uuid::parse_str(b_json["data"]["asset_id"].as_str().unwrap()).unwrap();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/confirm")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&ConfirmMediaCommand {
                        asset_id: branding_id,
                        alt_text: Some("Logo".to_string()),
                        width: None,
                        height: None,
                        metadata: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // 5. Quota check (200_000 + 150_000 = 350_000 used of 500_000) -> requesting 200_000 exceeds 500_000 -> 422
    let res_quota_exceeded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/presign")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(
                    serde_json::to_string(&PresignMediaCommand {
                        file_name: "too-big.webp".to_string(),
                        mime_type: "image/webp".to_string(),
                        size_bytes: 200_000,
                        context: Some(MediaContext::General),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res_quota_exceeded.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // 6. Strict RLS Isolation: Tenant B gets 404 on Tenant A's asset & 0 bytes quota
    for (method, uri) in [
        ("GET", format!("/v1/media/{asset_id}")),
        ("DELETE", format!("/v1/media/{asset_id}")),
        ("POST", format!("/v1/media/{asset_id}/restore")),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .header(HEADER_API_KEY, &key_b_admin)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    let res_quota_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/media/quota")
                .header(HEADER_API_KEY, &key_b_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let qb_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_quota_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(qb_json["data"]["used_bytes"], 0);

    // 7. Criteria search (context + metadata JSONB) + Anti-SQLi negative test
    let criteria = Criteria {
        filter: Some(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition {
                field: "context".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("content"),
            }),
            FilterRule::Condition(FilterCondition {
                field: "metadata.campaign".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("launch"),
            }),
        ])),
        order_by: None,
        pagination: Some(Pagination::new(1, 10)),
    };
    let res_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
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
    assert_eq!(search_json["data"].as_array().unwrap().len(), 1);

    // Anti-SQLi rejected with 400
    let sqli_criteria = Criteria {
        filter: Some(FilterGroup::And(vec![FilterRule::Condition(
            FilterCondition {
                field: "1=1; DROP TABLE media_assets;--".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("x"),
            },
        )])),
        order_by: None,
        pagination: None,
    };
    let res_sqli = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/media/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::from(serde_json::to_string(&sqli_criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_sqli.status(), StatusCode::BAD_REQUEST);

    // 8. Soft Delete -> Restore -> Permanent Delete (verifies ObjectStoragePort::delete_object & quota freed)
    let res_soft = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/media/{asset_id}"))
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_soft.status(), StatusCode::NO_CONTENT);
    assert!(spy_storage.deleted_keys.lock().unwrap().is_empty());

    let res_restore = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/media/{asset_id}/restore"))
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_restore.status(), StatusCode::OK);

    let res_perm = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/media/{asset_id}?permanent=true"))
                .header(HEADER_API_KEY, &key_a_admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_perm.status(), StatusCode::NO_CONTENT);
    assert_eq!(spy_storage.deleted_keys.lock().unwrap().len(), 1);
}
