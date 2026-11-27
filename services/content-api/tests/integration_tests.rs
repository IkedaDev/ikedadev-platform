use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use content_api::application::preview_token::PreviewTokenService;
use content_api::application::use_cases::{
    CreateArticleCommand, CreateCategoryCommand, CreateTagCommand, CreateWebhookCommand,
    UpdateArticleCommand, UpdateArticleStatusCommand,
};
use content_api::domain::models::ArticleStatus;
use content_api::infrastructure::{
    NoopScheduler, SqlxApiKeyValidator, SqlxArticleRepository, SqlxTaxonomyRepository,
    SqlxWebhookRepository,
};
use content_api::presentation::{create_router, ApiDoc, AppState};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, Pagination,
};
use ikeda_core::tenant::{hash_api_key, CachedApiKeyValidator, HEADER_API_KEY};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_content_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Content API");
    for path in [
        "/health",
        "/v1/articles",
        "/v1/articles/search",
        "/v1/articles/preview",
        "/v1/articles/by-slug/{slug}",
        "/v1/articles/{id}",
        "/v1/articles/{id}/status",
        "/v1/articles/{id}/revisions",
        "/v1/articles/{id}/revisions/{revision_id}/restore",
        "/v1/articles/{id}/preview-token",
        "/v1/internal/qstash/publish",
        "/v1/categories",
        "/v1/categories/{id}",
        "/v1/tags",
        "/v1/tags/{id}",
        "/v1/webhooks",
        "/v1/webhooks/{id}",
        "/v1/webhooks/{id}/test",
    ] {
        assert!(
            openapi.paths.paths.contains_key(path),
            "Missing OpenAPI path: {path}"
        );
    }

    // Regression: media endpoints have been extracted to services/media-api
    assert!(!openapi.paths.paths.contains_key("/v1/media"));
    assert!(!openapi.paths.paths.contains_key("/v1/media/presign"));
}

#[tokio::test]
async fn test_content_rls_isolation_lifecycle_criteria_and_signed_endpoints() {
    let db_url = std::env::var("CONTENT_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/content_db?sslmode=disable"
            .to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL content_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let key_a = format!("ik_cnt_a_{}", Uuid::new_v4().simple());
    let key_b = format!("ik_cnt_b_{}", Uuid::new_v4().simple());

    for (tid, name, raw_key) in [
        (tenant_a, "Tenant A Content Key", &key_a),
        (tenant_b, "Tenant B Content Key", &key_b),
    ] {
        let mut tx = pool.begin().await.expect("Failed to begin content key tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tid.to_string())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ik_cnt_key', $3, '{"*"}', true)
            "#,
        )
        .bind(tid)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .execute(&mut *tx)
        .await
        .expect("Failed to insert content API key");
        tx.commit().await.expect("Failed to commit content key tx");
    }

    let qstash_key = "test-integration-qstash-key";
    let state = AppState::new(
        Arc::new(SqlxArticleRepository::new(pool.clone())),
        Arc::new(SqlxTaxonomyRepository::new(pool.clone())),
        Arc::new(SqlxWebhookRepository::new(pool.clone())),
        Arc::new(NoopScheduler::new(qstash_key)),
        PreviewTokenService::new("test-integration-preview-secret"),
        Arc::new(CachedApiKeyValidator::new(SqlxApiKeyValidator::new(pool))),
    );
    let app = create_router(state);

    // Regression: /v1/media is no longer served by content-api (extracted to media-api)
    let res_removed_media = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/media")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_removed_media.status(), StatusCode::NOT_FOUND);

    // 1. Tenant A creates Category, Tag, and Webhook
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
                        name: "Ingeniería".to_string(),
                        slug: None,
                        description: Some("Artículos técnicos".to_string()),
                        display_order: 1,
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

    let res_tag = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tags")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateTagCommand {
                        name: "Rust".to_string(),
                        slug: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_tag.status(), StatusCode::CREATED);
    let tag_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_tag.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let tag_id = Uuid::parse_str(tag_json["data"]["id"].as_str().unwrap()).unwrap();

    let res_wh = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/webhooks")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateWebhookCommand {
                        name: "Cloudflare Pages".to_string(),
                        target_url:
                            "https://api.cloudflare.com/client/v4/pages/webhooks/deploy_hooks/test"
                                .to_string(),
                        events: None,
                        is_active: Some(false), // inactive so tests don't make external HTTP calls
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_wh.status(), StatusCode::CREATED);
    let wh_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_wh.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let wh_id = Uuid::parse_str(wh_json["data"]["id"].as_str().unwrap()).unwrap();

    // 2. Tenant A creates a DRAFT article (with XSS payload that must be stripped)
    let unique_slug = format!("arquitectura-rust-{}", Uuid::new_v4().simple());
    let res_art = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/articles")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&CreateArticleCommand {
                        category_id: Some(cat_id),
                        slug: Some(unique_slug.clone()),
                        title: "Arquitectura Multi-Tenant en Rust".to_string(),
                        excerpt: Some("Guía práctica".to_string()),
                        content_json: Some(serde_json::json!({ "type": "doc", "content": [] })),
                        content_html: Some("<p>Hola</p><script>alert(1)</script>".to_string()),
                        cover_image_url: Some("https://cdn.example.com/cover.webp".to_string()),
                        author_name_snapshot: Some("Admin Demo".to_string()),
                        seo: None,
                        metadata: Some(serde_json::json!({ "featured": "true" })),
                        tag_ids: vec![tag_id],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_art.status(), StatusCode::CREATED);
    let art_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_art.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let article_id = Uuid::parse_str(art_json["data"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(art_json["data"]["status"], "DRAFT");
    assert!(!art_json["data"]["content_html"]
        .as_str()
        .unwrap()
        .contains("<script"));

    // 3. STRICT RLS ISOLATION: Tenant B cannot access Tenant A's article, category, tag, or webhook
    for (method, uri) in [
        ("GET", format!("/v1/articles/{article_id}")),
        ("DELETE", format!("/v1/articles/{article_id}")),
        ("DELETE", format!("/v1/categories/{cat_id}")),
        ("DELETE", format!("/v1/tags/{tag_id}")),
        ("DELETE", format!("/v1/webhooks/{wh_id}")),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .header(HEADER_API_KEY, &key_b)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "Tenant B should get 404 on {method} {uri}"
        );
    }

    // Tenant B search returns 0 items
    let res_search_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/articles/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_b)
                .body(Body::from(
                    serde_json::to_string(&Criteria::default()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search_b.status(), StatusCode::OK);
    let search_b_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_b_json["data"].as_array().unwrap().len(), 0);

    // 4. Draft Preview Token -> GET /v1/articles/preview?token=... (NO API KEY / NO JWT)
    let res_pt = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/articles/{article_id}/preview-token"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pt.status(), StatusCode::OK);
    let pt_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_pt.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let preview_token = pt_json["data"]["token"].as_str().unwrap();

    let res_preview = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/preview?token={preview_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_preview.status(), StatusCode::OK);

    // 5. Schedule -> QStash signed callback publishes it without TenantContext
    let res_sched = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/articles/{article_id}/status"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&UpdateArticleStatusCommand {
                        status: ArticleStatus::Scheduled,
                        scheduled_for: Some(Utc::now() + Duration::hours(1)),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_sched.status(), StatusCode::OK);

    let qstash_body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_a,
        "article_id": article_id,
        "scheduled_for": (Utc::now() + Duration::hours(1)).to_rfc3339(),
    }))
    .unwrap();
    let body_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(&qstash_body));
    let qstash_sig = encode(
        &Header::new(Algorithm::HS256),
        &serde_json::json!({
            "iss": "Upstash",
            "body": body_hash,
            "exp": (Utc::now() + Duration::minutes(5)).timestamp(),
        }),
        &EncodingKey::from_secret(qstash_key.as_bytes()),
    )
    .unwrap();

    let res_qstash = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/internal/qstash/publish")
                .header(header::CONTENT_TYPE, "application/json")
                .header("Upstash-Signature", &qstash_sig)
                .body(Body::from(qstash_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_qstash.status(), StatusCode::OK);
    let qstash_art: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_qstash.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(qstash_art["data"]["status"], "PUBLISHED");

    // 6. Edit PUBLISHED article -> creates revision -> restore revision
    let res_edit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/articles/{article_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&UpdateArticleCommand {
                        title: Some("Título v2 Editado".to_string()),
                        content_html: Some("<p>Versión 2 actualizada</p>".to_string()),
                        ..Default::default()
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_edit.status(), StatusCode::OK);

    let res_revs = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/{article_id}/revisions"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_revs.status(), StatusCode::OK);
    let revs_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_revs.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let rev_id = Uuid::parse_str(revs_json["data"][0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        revs_json["data"][0]["title"],
        "Arquitectura Multi-Tenant en Rust"
    );

    // Tenant B cannot read Tenant A's revisions (404)
    let res_revs_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/{article_id}/revisions"))
                .header(HEADER_API_KEY, &key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_revs_b.status(), StatusCode::NOT_FOUND);

    // Restore revision
    let res_restore = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/articles/{article_id}/revisions/{rev_id}/restore"
                ))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_restore.status(), StatusCode::OK);
    let restored_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_restore.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        restored_json["data"]["title"],
        "Arquitectura Multi-Tenant en Rust"
    );

    // 7. Criteria search with metadata JSONB filter + by-slug with ETag & 304 Not Modified
    let criteria = Criteria {
        filter: Some(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition {
                field: "status".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("PUBLISHED"),
            }),
            FilterRule::Condition(FilterCondition {
                field: "metadata.featured".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("true"),
            }),
        ])),
        order_by: None,
        pagination: Some(Pagination::new(1, 10)),
    };

    let res_search_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/articles/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search_a.status(), StatusCode::OK);
    let search_a_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_a_json["data"].as_array().unwrap().len(), 1);

    let res_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/by-slug/{unique_slug}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_slug.status(), StatusCode::OK);
    let etag = res_slug
        .headers()
        .get(header::ETAG)
        .expect("ETag header present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(res_slug.headers().contains_key(header::CACHE_CONTROL));

    let res_304 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/by-slug/{unique_slug}"))
                .header(HEADER_API_KEY, &key_a)
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_304.status(), StatusCode::NOT_MODIFIED);

    // 8. Soft Delete article
    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/articles/{article_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::NO_CONTENT);

    let res_after_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/articles/{article_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_after_del.status(), StatusCode::NOT_FOUND);
}
