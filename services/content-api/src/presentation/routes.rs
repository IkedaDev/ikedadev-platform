use super::article_handlers::{self, *};
use super::media_handlers::{self, *};
use super::state::AppState;
use super::taxonomy_handlers::{self, *};
use super::webhook_handlers::{self, *};
use crate::application::preview_token::PreviewTokenResponse;
use crate::application::use_cases::{
    ConfirmMediaCommand, CreateArticleCommand, CreateCategoryCommand, CreateTagCommand,
    CreateWebhookCommand, PresignMediaCommand, UpdateArticleCommand, UpdateArticleStatusCommand,
    UpdateCategoryCommand, UpdateMediaCommand, UpdateWebhookCommand, WebhookTestResult,
};
use crate::domain::models::{
    Article, ArticleRevision, ArticleSeo, ArticleStatus, ContentCategory, ContentTag, MediaAsset,
    PresignedUpload, PublishWebhook, WebhookEvent,
};
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
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

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        article_handlers::create_article,
        article_handlers::get_article,
        article_handlers::get_article_by_slug,
        article_handlers::search_articles,
        article_handlers::update_article,
        article_handlers::update_article_status,
        article_handlers::delete_article,
        article_handlers::list_article_revisions,
        article_handlers::restore_article_revision,
        article_handlers::generate_preview_token,
        article_handlers::preview_article,
        article_handlers::qstash_publish_callback,
        taxonomy_handlers::create_category,
        taxonomy_handlers::list_categories,
        taxonomy_handlers::update_category,
        taxonomy_handlers::delete_category,
        taxonomy_handlers::create_tag,
        taxonomy_handlers::list_tags,
        taxonomy_handlers::delete_tag,
        media_handlers::presign_media,
        media_handlers::confirm_media,
        media_handlers::list_media,
        media_handlers::update_media,
        media_handlers::delete_media,
        webhook_handlers::create_webhook,
        webhook_handlers::list_webhooks,
        webhook_handlers::update_webhook,
        webhook_handlers::delete_webhook,
        webhook_handlers::test_webhook
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<Article>,
            PaginatedApiResponse<Article>,
            ApiResponse<Vec<ArticleRevision>>,
            ApiResponse<PreviewTokenResponse>,
            ApiResponse<ContentCategory>,
            ApiResponse<Vec<ContentCategory>>,
            ApiResponse<ContentTag>,
            ApiResponse<Vec<ContentTag>>,
            ApiResponse<PresignedUpload>,
            ApiResponse<MediaAsset>,
            PaginatedApiResponse<MediaAsset>,
            ApiResponse<PublishWebhook>,
            ApiResponse<Vec<PublishWebhook>>,
            ApiResponse<WebhookTestResult>,
            Article,
            ArticleStatus,
            ArticleSeo,
            ArticleRevision,
            PreviewTokenResponse,
            ContentCategory,
            ContentTag,
            MediaAsset,
            PresignedUpload,
            PublishWebhook,
            WebhookEvent,
            WebhookTestResult,
            CreateArticleCommand,
            UpdateArticleCommand,
            UpdateArticleStatusCommand,
            CreateCategoryCommand,
            UpdateCategoryCommand,
            CreateTagCommand,
            PresignMediaCommand,
            ConfirmMediaCommand,
            UpdateMediaCommand,
            CreateWebhookCommand,
            UpdateWebhookCommand,
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
        (name = "Articles", description = "Multi-tenant editorial articles with TipTap JSON, sanitized HTML, revisions, and preview tokens"),
        (name = "Taxonomy", description = "Editorial categories and tags per tenant"),
        (name = "Media", description = "Provider-agnostic Media Library (Cloudflare R2 / BYOS S3 presigned uploads)"),
        (name = "Deploy Hooks", description = "SSG Deploy Hooks (Cloudflare Pages, Vercel, Netlify) with anti-SSRF protection"),
        (name = "Internal", description = "Signed Upstash QStash internal callbacks")
    ),
    info(
        title = "IkedaDev Content API",
        version = "0.1.0",
        description = "Multi-tenant Headless CMS supporting SSR (ETag/Cache-Control) and SSG (Deploy Hooks), TipTap editorial content, ammonia HTML sanitization, Media Library, draft previews, and revision history."
    )
)]
pub struct ApiDoc;

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
        service: "content-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Builds and configures the Content API presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        // Articles (static paths before dynamic `{id}` for clean matching)
        .route("/v1/articles", post(create_article))
        .route("/v1/articles/search", post(search_articles))
        .route("/v1/articles/preview", get(preview_article))
        .route("/v1/articles/by-slug/{slug}", get(get_article_by_slug))
        .route(
            "/v1/articles/{id}",
            get(get_article)
                .patch(update_article)
                .delete(delete_article),
        )
        .route("/v1/articles/{id}/status", patch(update_article_status))
        .route("/v1/articles/{id}/revisions", get(list_article_revisions))
        .route(
            "/v1/articles/{id}/revisions/{revision_id}/restore",
            post(restore_article_revision),
        )
        .route(
            "/v1/articles/{id}/preview-token",
            post(generate_preview_token),
        )
        // Internal QStash callback (verified via Upstash-Signature)
        .route("/v1/internal/qstash/publish", post(qstash_publish_callback))
        // Taxonomy
        .route("/v1/categories", post(create_category).get(list_categories))
        .route(
            "/v1/categories/{id}",
            patch(update_category).delete(delete_category),
        )
        .route("/v1/tags", post(create_tag).get(list_tags))
        .route("/v1/tags/{id}", delete(delete_tag))
        // Media Library
        .route("/v1/media/presign", post(presign_media))
        .route("/v1/media/confirm", post(confirm_media))
        .route(
            "/v1/media/local/{tenant_id}/{filename}",
            put(upload_local_media)
                .get(serve_local_media)
                .layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route("/v1/media", get(list_media))
        .route("/v1/media/{id}", patch(update_media).delete(delete_media))
        // SSG Deploy Hooks
        .route("/v1/webhooks", post(create_webhook).get(list_webhooks))
        .route(
            "/v1/webhooks/{id}",
            patch(update_webhook).delete(delete_webhook),
        )
        .route("/v1/webhooks/{id}/test", post(test_webhook))
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        // Body limit: 1 MiB to safely accommodate rich TipTap JSON + HTML payloads
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}
