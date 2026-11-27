use crate::application::preview_token::{PreviewTokenResponse, PreviewTokenService};
use crate::application::rbac::{
    has_content_publish, require_content_publish, require_content_read, require_content_write,
};
use crate::application::use_cases::webhook_use_cases::dispatch_deploy_hooks;
use crate::domain::models::{Article, ArticleRevision, ArticleSeo, ArticleStatus, WebhookEvent};
use crate::domain::ports::{
    ArticleRepository, CreateArticleData, CreateRevisionData, ScheduledPublishPayload,
    SchedulerPort, UpdateArticleData, WebhookRepository,
};
use crate::domain::services::{
    compute_reading_metrics, normalize_slug, sanitize_html, validate_json_size,
    validate_public_url, validate_seo, MAX_CONTENT_HTML_BYTES, MAX_CONTENT_JSON_BYTES,
    MAX_METADATA_JSON_BYTES,
};
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Number of historical revisions retained per article.
pub const MAX_ARTICLE_REVISIONS: i64 = 20;

/// Payload to create a new editorial article (always starts in `DRAFT`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateArticleCommand {
    pub category_id: Option<Uuid>,
    pub slug: Option<String>,
    pub title: String,
    pub excerpt: Option<String>,
    #[serde(default)]
    pub content_json: Option<serde_json::Value>,
    #[serde(default)]
    pub content_html: Option<String>,
    pub cover_image_url: Option<String>,
    pub author_name_snapshot: Option<String>,
    #[serde(default)]
    pub seo: Option<ArticleSeo>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}

/// Payload to update an existing article.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateArticleCommand {
    pub category_id: Option<Option<Uuid>>,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<Option<String>>,
    pub content_json: Option<serde_json::Value>,
    pub content_html: Option<String>,
    pub cover_image_url: Option<Option<String>>,
    pub seo: Option<ArticleSeo>,
    pub metadata: Option<serde_json::Value>,
    pub tag_ids: Option<Vec<Uuid>>,
}

/// Payload to transition an article's status (`DRAFT`, `SCHEDULED`, `PUBLISHED`, `ARCHIVED`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateArticleStatusCommand {
    pub status: ArticleStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
}

/// Creates a new article in `DRAFT` status.
pub async fn create_article(
    repo: &dyn ArticleRepository,
    ctx: &TenantContext,
    cmd: CreateArticleCommand,
    allow_http_urls: bool,
) -> Result<Article, AppError> {
    require_content_write(ctx)?;

    let title = cmd.title.trim().to_string();
    if title.is_empty() || title.len() > 255 {
        return Err(AppError::BadRequest(
            "title must be between 1 and 255 characters".to_string(),
        ));
    }

    let slug_source = cmd
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&title);
    let slug = normalize_slug(slug_source, 180);
    if slug.is_empty() {
        return Err(AppError::BadRequest(
            "Could not derive a valid ASCII slug from title or slug".to_string(),
        ));
    }

    let content_json = cmd
        .content_json
        .unwrap_or_else(|| serde_json::json!({ "type": "doc", "content": [] }));
    validate_json_size(&content_json, "content_json", MAX_CONTENT_JSON_BYTES)?;

    let raw_html = cmd.content_html.unwrap_or_default();
    if raw_html.len() > MAX_CONTENT_HTML_BYTES {
        return Err(AppError::BadRequest(format!(
            "content_html exceeds maximum size of {MAX_CONTENT_HTML_BYTES} bytes"
        )));
    }
    let sanitized_html = sanitize_html(&raw_html);
    let (word_count, reading_time_minutes) = compute_reading_metrics(&sanitized_html);

    let cover_image_url = match cmd.cover_image_url {
        Some(ref u) if !u.trim().is_empty() => {
            Some(validate_public_url(u, "cover_image_url", allow_http_urls)?)
        }
        _ => None,
    };

    let seo = cmd.seo.unwrap_or_default();
    validate_seo(&seo, allow_http_urls)?;

    let metadata = cmd.metadata.unwrap_or_else(|| serde_json::json!({}));
    validate_json_size(&metadata, "metadata", MAX_METADATA_JSON_BYTES)?;

    let author_name_snapshot = cmd
        .author_name_snapshot
        .map(|s| s.trim().chars().take(150).collect::<String>())
        .filter(|s| !s.is_empty());

    let excerpt = cmd
        .excerpt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    repo.create(CreateArticleData {
        tenant_id: ctx.tenant_id,
        category_id: cmd.category_id,
        slug,
        title,
        excerpt,
        content_json,
        content_html: sanitized_html,
        cover_image_url,
        author_id: ctx.user_id,
        author_name_snapshot,
        word_count,
        reading_time_minutes,
        seo,
        metadata,
        tag_ids: cmd.tag_ids,
    })
    .await
}

/// Fetches an article by ID within the caller's tenant.
pub async fn get_article(
    repo: &dyn ArticleRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<Article, AppError> {
    require_content_read(ctx)?;
    repo.find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))
}

/// Fetches an article by slug within the caller's tenant (ideal for SSR/SSG consumption).
pub async fn get_article_by_slug(
    repo: &dyn ArticleRepository,
    ctx: &TenantContext,
    slug: &str,
) -> Result<Article, AppError> {
    require_content_read(ctx)?;
    let normalized = slug.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(AppError::BadRequest("slug cannot be empty".to_string()));
    }
    repo.find_by_slug(ctx.tenant_id, &normalized)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article with slug '{normalized}' not found")))
}

/// Searches articles using `ikeda_core::Criteria`.
pub async fn search_articles(
    repo: &dyn ArticleRepository,
    ctx: &TenantContext,
    criteria: &Criteria,
) -> Result<(Vec<Article>, u64), AppError> {
    require_content_read(ctx)?;
    repo.search(ctx.tenant_id, criteria).await
}

/// Updates an existing article.
///
/// Security rule (RBAC):
/// - `content:write` (e.g. `staff`) may ONLY edit articles in `DRAFT` status.
/// - Editing `PUBLISHED`, `SCHEDULED`, or `ARCHIVED` articles requires `content:publish` (`tenant:admin` / `platform:admin`).
/// - Editing a `PUBLISHED` article automatically saves a snapshot in `article_revisions` (keeping last 20)
///   and triggers tenant Deploy Hooks (`article.updated`).
pub async fn update_article(
    repo: &dyn ArticleRepository,
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateArticleCommand,
    allow_http_urls: bool,
) -> Result<Article, AppError> {
    require_content_write(ctx)?;

    let existing = repo
        .find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))?;

    if existing.status != ArticleStatus::Draft && !has_content_publish(ctx) {
        return Err(AppError::Forbidden(format!(
            "Only users with 'content:publish' may edit articles in '{}' status",
            existing.status.as_str()
        )));
    }

    let title = match cmd.title {
        Some(raw) => {
            let t = raw.trim().to_string();
            if t.is_empty() || t.len() > 255 {
                return Err(AppError::BadRequest(
                    "title must be between 1 and 255 characters".to_string(),
                ));
            }
            Some(t)
        }
        None => None,
    };

    let slug = match cmd.slug {
        Some(raw) => {
            let s = normalize_slug(&raw, 180);
            if s.is_empty() {
                return Err(AppError::BadRequest(
                    "slug must contain alphanumeric ASCII characters".to_string(),
                ));
            }
            Some(s)
        }
        None => None,
    };

    if let Some(ref json) = cmd.content_json {
        validate_json_size(json, "content_json", MAX_CONTENT_JSON_BYTES)?;
    }

    let (sanitized_html, word_count, reading_time_minutes) = match cmd.content_html {
        Some(raw_html) => {
            if raw_html.len() > MAX_CONTENT_HTML_BYTES {
                return Err(AppError::BadRequest(format!(
                    "content_html exceeds maximum size of {MAX_CONTENT_HTML_BYTES} bytes"
                )));
            }
            let clean = sanitize_html(&raw_html);
            let (wc, rt) = compute_reading_metrics(&clean);
            (Some(clean), Some(wc), Some(rt))
        }
        None => (None, None, None),
    };

    let cover_image_url = match cmd.cover_image_url {
        Some(Some(u)) if !u.trim().is_empty() => Some(Some(validate_public_url(
            &u,
            "cover_image_url",
            allow_http_urls,
        )?)),
        Some(_) => Some(None),
        None => None,
    };

    if let Some(ref seo) = cmd.seo {
        validate_seo(seo, allow_http_urls)?;
    }

    if let Some(ref metadata) = cmd.metadata {
        validate_json_size(metadata, "metadata", MAX_METADATA_JSON_BYTES)?;
    }

    let excerpt = cmd
        .excerpt
        .map(|opt| opt.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));

    // Snapshot previous state before modifying a PUBLISHED article
    if existing.status == ArticleStatus::Published {
        repo.create_revision(
            CreateRevisionData {
                tenant_id: ctx.tenant_id,
                article_id: existing.id,
                title: existing.title.clone(),
                content_json: existing.content_json.clone(),
                content_html: existing.content_html.clone(),
                edited_by: ctx.user_id,
            },
            MAX_ARTICLE_REVISIONS,
        )
        .await?;
    }

    let updated = repo
        .update(
            ctx.tenant_id,
            id,
            UpdateArticleData {
                category_id: cmd.category_id,
                slug,
                title,
                excerpt,
                content_json: cmd.content_json,
                content_html: sanitized_html,
                cover_image_url,
                word_count,
                reading_time_minutes,
                seo: cmd.seo,
                metadata: cmd.metadata,
                tag_ids: cmd.tag_ids,
            },
        )
        .await?;

    if updated.status == ArticleStatus::Published {
        dispatch_deploy_hooks(
            webhook_repo,
            http_client,
            ctx.tenant_id,
            WebhookEvent::ArticleUpdated,
            updated.id,
            updated.slug.clone(),
        );
    }

    Ok(updated)
}

/// Transitions an article's status (`DRAFT`, `SCHEDULED`, `PUBLISHED`, `ARCHIVED`).
/// Requires `content:publish`.
pub async fn update_article_status(
    repo: &dyn ArticleRepository,
    scheduler: &dyn SchedulerPort,
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateArticleStatusCommand,
) -> Result<Article, AppError> {
    require_content_publish(ctx)?;

    let existing = repo
        .find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))?;

    if !existing.status.can_transition_to(cmd.status) {
        return Err(AppError::BadRequest(format!(
            "Invalid status transition from '{}' to '{}'",
            existing.status.as_str(),
            cmd.status.as_str()
        )));
    }

    let (published_at, scheduled_for) = match cmd.status {
        ArticleStatus::Published => (Some(existing.published_at.unwrap_or_else(Utc::now)), None),
        ArticleStatus::Scheduled => {
            let target = cmd.scheduled_for.ok_or_else(|| {
                AppError::BadRequest(
                    "scheduled_for timestamp is required when transitioning to SCHEDULED"
                        .to_string(),
                )
            })?;
            if target <= Utc::now() {
                return Err(AppError::BadRequest(
                    "scheduled_for must be a future timestamp".to_string(),
                ));
            }
            (existing.published_at, Some(target))
        }
        ArticleStatus::Draft | ArticleStatus::Archived => (existing.published_at, None),
    };

    let updated = repo
        .update_status(ctx.tenant_id, id, cmd.status, published_at, scheduled_for)
        .await?;

    if cmd.status == ArticleStatus::Scheduled {
        if let Some(target_ts) = scheduled_for {
            scheduler
                .schedule_article_publish(ScheduledPublishPayload {
                    tenant_id: ctx.tenant_id,
                    article_id: updated.id,
                    scheduled_for: target_ts,
                })
                .await?;
        }
    } else if cmd.status == ArticleStatus::Published {
        dispatch_deploy_hooks(
            webhook_repo,
            http_client,
            ctx.tenant_id,
            WebhookEvent::ArticlePublished,
            updated.id,
            updated.slug.clone(),
        );
    } else if existing.status == ArticleStatus::Published
        && matches!(cmd.status, ArticleStatus::Draft | ArticleStatus::Archived)
    {
        dispatch_deploy_hooks(
            webhook_repo,
            http_client,
            ctx.tenant_id,
            WebhookEvent::ArticleUnpublished,
            updated.id,
            updated.slug.clone(),
        );
    }

    Ok(updated)
}

/// Soft-deletes an article. Requires `content:publish`. Triggers `article.unpublished` if it was published.
pub async fn delete_article(
    repo: &dyn ArticleRepository,
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<(), AppError> {
    require_content_publish(ctx)?;

    let deleted = repo.soft_delete(ctx.tenant_id, id).await?;
    if deleted.status == ArticleStatus::Published {
        dispatch_deploy_hooks(
            webhook_repo,
            http_client,
            ctx.tenant_id,
            WebhookEvent::ArticleUnpublished,
            deleted.id,
            deleted.slug,
        );
    }
    Ok(())
}

/// Lists historical revisions of an article (`content:read`).
pub async fn list_article_revisions(
    repo: &dyn ArticleRepository,
    ctx: &TenantContext,
    article_id: Uuid,
) -> Result<Vec<ArticleRevision>, AppError> {
    require_content_read(ctx)?;
    let _ = repo
        .find_by_id(ctx.tenant_id, article_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{article_id}' not found")))?;

    repo.list_revisions(ctx.tenant_id, article_id).await
}

/// Restores a previous revision snapshot onto the article (`content:publish`).
pub async fn restore_article_revision(
    repo: &dyn ArticleRepository,
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    ctx: &TenantContext,
    article_id: Uuid,
    revision_id: Uuid,
) -> Result<Article, AppError> {
    require_content_publish(ctx)?;

    let existing = repo
        .find_by_id(ctx.tenant_id, article_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{article_id}' not found")))?;

    let revision = repo
        .find_revision(ctx.tenant_id, article_id, revision_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Revision '{revision_id}' not found")))?;

    // Save current state as a revision before restoring
    repo.create_revision(
        CreateRevisionData {
            tenant_id: ctx.tenant_id,
            article_id: existing.id,
            title: existing.title.clone(),
            content_json: existing.content_json.clone(),
            content_html: existing.content_html.clone(),
            edited_by: ctx.user_id,
        },
        MAX_ARTICLE_REVISIONS,
    )
    .await?;

    let sanitized_html = sanitize_html(&revision.content_html);
    let (word_count, reading_time_minutes) = compute_reading_metrics(&sanitized_html);

    let updated = repo
        .update(
            ctx.tenant_id,
            article_id,
            UpdateArticleData {
                category_id: None,
                slug: None,
                title: Some(revision.title),
                excerpt: None,
                content_json: Some(revision.content_json),
                content_html: Some(sanitized_html),
                cover_image_url: None,
                word_count: Some(word_count),
                reading_time_minutes: Some(reading_time_minutes),
                seo: None,
                metadata: None,
                tag_ids: None,
            },
        )
        .await?;

    if updated.status == ArticleStatus::Published {
        dispatch_deploy_hooks(
            webhook_repo,
            http_client,
            ctx.tenant_id,
            WebhookEvent::ArticleUpdated,
            updated.id,
            updated.slug.clone(),
        );
    }

    Ok(updated)
}

/// Generates a 30-minute signed preview token for an article (`content:read`).
pub async fn generate_preview_token(
    repo: &dyn ArticleRepository,
    preview_service: &PreviewTokenService,
    ctx: &TenantContext,
    article_id: Uuid,
) -> Result<PreviewTokenResponse, AppError> {
    require_content_read(ctx)?;
    let article = repo
        .find_by_id(ctx.tenant_id, article_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Article '{article_id}' not found")))?;

    preview_service.generate(ctx.tenant_id, article.id)
}

/// Resolves an article using a self-signed preview token (no `TenantContext` extractor).
pub async fn resolve_article_preview(
    repo: &dyn ArticleRepository,
    preview_service: &PreviewTokenService,
    token: &str,
) -> Result<Article, AppError> {
    let verified = preview_service.verify(token)?;
    repo.find_by_id(verified.tenant_id, verified.article_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Preview article not found".to_string()))
}

/// Handles a signed Upstash QStash callback to publish a scheduled article (no `TenantContext`).
pub async fn handle_qstash_scheduled_publish(
    repo: &dyn ArticleRepository,
    scheduler: &dyn SchedulerPort,
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    signature_header: &str,
    raw_body: &[u8],
) -> Result<Article, AppError> {
    scheduler.verify_signature(signature_header, raw_body)?;

    let payload: ScheduledPublishPayload = serde_json::from_slice(raw_body)
        .map_err(|e| AppError::BadRequest(format!("Invalid QStash publish payload JSON: {e}")))?;

    let existing = repo
        .find_by_id(payload.tenant_id, payload.article_id)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Scheduled article '{}' not found",
                payload.article_id
            ))
        })?;

    if existing.status != ArticleStatus::Scheduled {
        return Ok(existing);
    }

    let published = repo
        .update_status(
            payload.tenant_id,
            payload.article_id,
            ArticleStatus::Published,
            Some(Utc::now()),
            None,
        )
        .await?;

    dispatch_deploy_hooks(
        webhook_repo,
        http_client,
        payload.tenant_id,
        WebhookEvent::ArticlePublished,
        published.id,
        published.slug.clone(),
    );

    Ok(published)
}
