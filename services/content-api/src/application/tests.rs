use crate::application::preview_token::PreviewTokenService;
use crate::application::rbac::{
    require_content_publish, require_content_read, require_content_write,
};
use crate::application::use_cases::article_use_cases::{
    update_article, update_article_status, UpdateArticleCommand, UpdateArticleStatusCommand,
};
use crate::application::use_cases::webhook_use_cases::{create_webhook, CreateWebhookCommand};
use crate::domain::models::{Article, ArticleRevision, ArticleSeo, ArticleStatus, PublishWebhook};
use crate::domain::ports::{
    ArticleRepository, CreateArticleData, CreateRevisionData, CreateWebhookData,
    ScheduledPublishPayload, SchedulerPort, UpdateArticleData, UpdateWebhookData,
    WebhookRepository,
};
use crate::domain::services::{
    compute_reading_metrics, normalize_slug, sanitize_html, validate_webhook_target_url,
};
use crate::infrastructure::NoopScheduler;
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[test]
fn test_ammonia_xss_sanitization() {
    let dirty = r#"
        <h2>Hola Mundo</h2>
        <script>alert('xss')</script>
        <p onclick="evil()">Texto <a href="javascript:alert(1)">malo</a> y <a href="https://ikedadev.com">bueno</a></p>
        <img src="https://cdn.example.com/a.webp" onerror="alert('img')" alt="Foto" />
    "#;
    let clean = sanitize_html(dirty);
    assert!(!clean.contains("<script"));
    assert!(!clean.contains("alert("));
    assert!(!clean.contains("onerror"));
    assert!(!clean.contains("onclick"));
    assert!(!clean.contains("javascript:"));
    assert!(clean.contains("<h2>Hola Mundo</h2>"));
    assert!(clean.contains("https://ikedadev.com"));
    assert!(clean.contains("noopener noreferrer"));
    assert!(clean.contains("https://cdn.example.com/a.webp"));
}

#[test]
fn test_slug_normalization() {
    assert_eq!(
        normalize_slug("  ¡Guía Rápida de Rust & Axum 2026! ", 180),
        "guia-rapida-de-rust-axum-2026"
    );
    assert_eq!(normalize_slug("Año Nuevo / Ñandú", 180), "ano-nuevo-nandu");
    assert_eq!(normalize_slug("a---b   c", 180), "a-b-c");
}

#[test]
fn test_reading_time_calculation() {
    assert_eq!(compute_reading_metrics("<p></p>"), (0, 1));
    let words_250 = format!("<p>{}</p>", vec!["rust"; 250].join(" "));
    assert_eq!(compute_reading_metrics(&words_250), (250, 2));
    let words_400 = format!("<p>{}</p>", vec!["axum"; 400].join(" "));
    assert_eq!(compute_reading_metrics(&words_400), (400, 2));
    let words_401 = format!("<p>{}</p>", vec!["axum"; 401].join(" "));
    assert_eq!(compute_reading_metrics(&words_401), (401, 3));
}

#[test]
fn test_anti_ssrf_webhook_url_validation() {
    let blocked = [
        "http://api.cloudflare.com/client/v4/pages/webhooks/deploy_hooks/123",
        "https://localhost/hook",
        "https://127.0.0.1/hook",
        "https://127.10.20.30/hook",
        "https://10.0.0.1/hook",
        "https://172.16.0.5/hook",
        "https://192.168.1.10/hook",
        "https://169.254.169.254/latest/meta-data/",
        "https://metadata.google.internal/computeMetadata/v1/",
        "https://[::1]/hook",
        "https://[fc00::1]/hook",
        "https://[fe80::1]/hook",
        "https://user:pass@api.vercel.com/v1/integrations/deploy/123",
    ];
    for url in blocked {
        assert!(
            validate_webhook_target_url(url).is_err(),
            "Expected SSRF validation to block '{url}'"
        );
    }

    let allowed = [
        "https://api.cloudflare.com/client/v4/pages/webhooks/deploy_hooks/abc-123",
        "https://api.vercel.com/v1/integrations/deploy/prj_123/xyz",
        "https://api.netlify.com/build_hooks/65f000000000",
    ];
    for url in allowed {
        assert!(
            validate_webhook_target_url(url).is_ok(),
            "Expected SSRF validation to allow '{url}'"
        );
    }
}

#[test]
fn test_article_state_machine() {
    assert!(ArticleStatus::Draft.can_transition_to(ArticleStatus::Scheduled));
    assert!(ArticleStatus::Draft.can_transition_to(ArticleStatus::Published));
    assert!(ArticleStatus::Scheduled.can_transition_to(ArticleStatus::Published));
    assert!(ArticleStatus::Published.can_transition_to(ArticleStatus::Archived));
    assert!(ArticleStatus::Archived.can_transition_to(ArticleStatus::Draft));
    assert!(!ArticleStatus::Published.can_transition_to(ArticleStatus::Scheduled));
    assert!(!ArticleStatus::Archived.can_transition_to(ArticleStatus::Scheduled));
}

#[test]
fn test_rbac_matrix() {
    let tid = Uuid::new_v4();
    let staff_ctx = TenantContext::new(tid).with_scopes(vec!["staff".to_string()]);
    assert!(require_content_read(&staff_ctx).is_ok());
    assert!(require_content_write(&staff_ctx).is_ok());
    assert!(matches!(
        require_content_publish(&staff_ctx),
        Err(AppError::Forbidden(_))
    ));

    let member_ctx = TenantContext::new(tid).with_scopes(vec![
        "tenant:member".to_string(),
        "content:read".to_string(),
        "content:write".to_string(),
    ]);
    assert!(require_content_read(&member_ctx).is_ok());
    assert!(require_content_write(&member_ctx).is_ok());
    assert!(matches!(
        require_content_publish(&member_ctx),
        Err(AppError::Forbidden(_))
    ));

    let admin_ctx = TenantContext::new(tid).with_scopes(vec!["tenant:admin".to_string()]);
    assert!(require_content_read(&admin_ctx).is_ok());
    assert!(require_content_write(&admin_ctx).is_ok());
    assert!(require_content_publish(&admin_ctx).is_ok());
}

#[test]
fn test_preview_token_sign_and_verify() {
    let svc = PreviewTokenService::new("test-secret-key-123");
    let tid = Uuid::new_v4();
    let aid = Uuid::new_v4();

    let generated = svc.generate(tid, aid).expect("should sign token");
    let verified = svc.verify(&generated.token).expect("should verify token");
    assert_eq!(verified.tenant_id, tid);
    assert_eq!(verified.article_id, aid);

    let wrong_svc = PreviewTokenService::new("different-secret");
    assert!(matches!(
        wrong_svc.verify(&generated.token),
        Err(AppError::Unauthorized(_))
    ));
}

#[test]
fn test_qstash_signature_verification() {
    let signing_key = "test-qstash-key";
    let scheduler = NoopScheduler::new(signing_key);
    let body = br#"{"tenant_id":"11111111-1111-4111-8111-111111111111"}"#;
    let body_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(body));

    let claims = serde_json::json!({
        "iss": "Upstash",
        "body": body_hash,
        "exp": (Utc::now() + Duration::minutes(5)).timestamp(),
    });
    let sig = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(signing_key.as_bytes()),
    )
    .expect("encode jwt");

    assert!(scheduler.verify_signature(&sig, body).is_ok());
    assert!(matches!(
        scheduler.verify_signature(&sig, b"tampered"),
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        scheduler.verify_signature("", body),
        Err(AppError::Unauthorized(_))
    ));
}

// --- In-memory mock repos to test RBAC enforcement in use cases ---

struct MockArticleRepo {
    article: Mutex<Article>,
}

#[async_trait]
impl ArticleRepository for MockArticleRepo {
    async fn create(&self, _data: CreateArticleData) -> Result<Article, AppError> {
        unreachable!()
    }
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Article>, AppError> {
        let guard = self.article.lock().unwrap();
        if guard.tenant_id == tenant_id && guard.id == id {
            Ok(Some(guard.clone()))
        } else {
            Ok(None)
        }
    }
    async fn find_by_slug(
        &self,
        _tenant_id: Uuid,
        _slug: &str,
    ) -> Result<Option<Article>, AppError> {
        Ok(None)
    }
    async fn search(
        &self,
        _tenant_id: Uuid,
        _criteria: &Criteria,
    ) -> Result<(Vec<Article>, u64), AppError> {
        Ok((vec![], 0))
    }
    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        data: UpdateArticleData,
    ) -> Result<Article, AppError> {
        let mut guard = self.article.lock().unwrap();
        if let Some(title) = data.title {
            guard.title = title;
        }
        Ok(guard.clone())
    }
    async fn update_status(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        status: ArticleStatus,
        published_at: Option<DateTime<Utc>>,
        scheduled_for: Option<DateTime<Utc>>,
    ) -> Result<Article, AppError> {
        let mut guard = self.article.lock().unwrap();
        guard.status = status;
        guard.published_at = published_at;
        guard.scheduled_for = scheduled_for;
        Ok(guard.clone())
    }
    async fn soft_delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Article, AppError> {
        Ok(self.article.lock().unwrap().clone())
    }
    async fn create_revision(
        &self,
        data: CreateRevisionData,
        _keep_last: i64,
    ) -> Result<ArticleRevision, AppError> {
        Ok(ArticleRevision {
            id: Uuid::new_v4(),
            tenant_id: data.tenant_id,
            article_id: data.article_id,
            version_number: 1,
            title: data.title,
            content_json: data.content_json,
            content_html: data.content_html,
            edited_by: data.edited_by,
            created_at: Utc::now(),
        })
    }
    async fn list_revisions(
        &self,
        _tenant_id: Uuid,
        _article_id: Uuid,
    ) -> Result<Vec<ArticleRevision>, AppError> {
        Ok(vec![])
    }
    async fn find_revision(
        &self,
        _tenant_id: Uuid,
        _article_id: Uuid,
        _revision_id: Uuid,
    ) -> Result<Option<ArticleRevision>, AppError> {
        Ok(None)
    }
}

struct MockWebhookRepo;

#[async_trait]
impl WebhookRepository for MockWebhookRepo {
    async fn create(&self, data: CreateWebhookData) -> Result<PublishWebhook, AppError> {
        Ok(PublishWebhook {
            id: Uuid::new_v4(),
            tenant_id: data.tenant_id,
            name: data.name,
            target_url: data.target_url,
            events: data.events,
            is_active: data.is_active,
            last_triggered_at: None,
            last_status_code: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        })
    }
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<PublishWebhook>, AppError> {
        Ok(vec![])
    }
    async fn find_by_id(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<PublishWebhook>, AppError> {
        Ok(None)
    }
    async fn find_active_for_event(
        &self,
        _tenant_id: Uuid,
        _event_name: &str,
    ) -> Result<Vec<PublishWebhook>, AppError> {
        Ok(vec![])
    }
    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _data: UpdateWebhookData,
    ) -> Result<PublishWebhook, AppError> {
        unreachable!()
    }
    async fn record_delivery(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _status_code: i16,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn soft_delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
}

fn sample_article(tenant_id: Uuid, status: ArticleStatus) -> Article {
    Article {
        id: Uuid::new_v4(),
        tenant_id,
        category_id: None,
        category: None,
        tags: vec![],
        slug: "sample".to_string(),
        title: "Sample".to_string(),
        excerpt: None,
        content_json: serde_json::json!({}),
        content_html: "<p>Hello</p>".to_string(),
        cover_image_url: None,
        status,
        published_at: None,
        scheduled_for: None,
        author_id: None,
        author_name_snapshot: None,
        word_count: 1,
        reading_time_minutes: 1,
        seo: ArticleSeo::default(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    }
}

#[tokio::test]
async fn test_staff_cannot_publish_or_edit_published_or_create_webhooks() {
    let tid = Uuid::new_v4();
    let staff_ctx = TenantContext::new(tid).with_scopes(vec![
        "staff".to_string(),
        "content:read".to_string(),
        "content:write".to_string(),
    ]);
    let admin_ctx = TenantContext::new(tid).with_scopes(vec!["tenant:admin".to_string()]);
    let webhook_repo: Arc<dyn WebhookRepository> = Arc::new(MockWebhookRepo);
    let scheduler = NoopScheduler::default();

    // 1. Staff cannot create webhook -> 403
    let wh_res = create_webhook(
        webhook_repo.as_ref(),
        &staff_ctx,
        CreateWebhookCommand {
            name: "CF Pages".to_string(),
            target_url: "https://api.cloudflare.com/client/v4/pages/webhooks/deploy_hooks/123"
                .to_string(),
            events: None,
            is_active: Some(true),
        },
    )
    .await;
    assert!(matches!(wh_res, Err(AppError::Forbidden(_))));

    // 2. Staff cannot publish a DRAFT article -> 403
    let draft = sample_article(tid, ArticleStatus::Draft);
    let draft_id = draft.id;
    let draft_repo = MockArticleRepo {
        article: Mutex::new(draft),
    };
    let pub_res = update_article_status(
        &draft_repo,
        &scheduler,
        webhook_repo.clone(),
        reqwest::Client::new(),
        &staff_ctx,
        draft_id,
        UpdateArticleStatusCommand {
            status: ArticleStatus::Published,
            scheduled_for: None,
        },
    )
    .await;
    assert!(matches!(pub_res, Err(AppError::Forbidden(_))));

    // 3. Staff CAN edit a DRAFT article
    let edit_draft = update_article(
        &draft_repo,
        webhook_repo.clone(),
        reqwest::Client::new(),
        &staff_ctx,
        draft_id,
        UpdateArticleCommand {
            title: Some("Draft Updated by Staff".to_string()),
            ..Default::default()
        },
        false,
    )
    .await;
    assert!(edit_draft.is_ok());

    // 4. Staff CANNOT edit a PUBLISHED or SCHEDULED article -> 403
    for locked_status in [ArticleStatus::Published, ArticleStatus::Scheduled] {
        let locked = sample_article(tid, locked_status);
        let locked_id = locked.id;
        let locked_repo = MockArticleRepo {
            article: Mutex::new(locked),
        };
        let staff_edit = update_article(
            &locked_repo,
            webhook_repo.clone(),
            reqwest::Client::new(),
            &staff_ctx,
            locked_id,
            UpdateArticleCommand {
                title: Some("Unauthorized Edit".to_string()),
                ..Default::default()
            },
            false,
        )
        .await;
        assert!(
            matches!(staff_edit, Err(AppError::Forbidden(_))),
            "Expected 403 when staff edits {locked_status:?}"
        );

        // Admin CAN edit it
        let admin_edit = update_article(
            &locked_repo,
            webhook_repo.clone(),
            reqwest::Client::new(),
            &admin_ctx,
            locked_id,
            UpdateArticleCommand {
                title: Some("Admin Edit OK".to_string()),
                ..Default::default()
            },
            false,
        )
        .await;
        assert!(admin_edit.is_ok());
    }

    // Ensure ScheduledPublishPayload compiles cleanly in tests
    let _payload = ScheduledPublishPayload {
        tenant_id: tid,
        article_id: draft_id,
        scheduled_for: Utc::now() + Duration::hours(1),
    };
}
