use super::article_handlers::{self, *};
use super::state::AppState;
use super::taxonomy_handlers::{self, *};
use super::webhook_handlers::{self, *};
use crate::application::preview_token::PreviewTokenResponse;
use crate::application::use_cases::{
    CreateArticleCommand, CreateCategoryCommand, CreateTagCommand, CreateWebhookCommand,
    UpdateArticleCommand, UpdateArticleStatusCommand, UpdateCategoryCommand, UpdateWebhookCommand,
    WebhookTestResult,
};
use crate::domain::models::{
    Article, ArticleRevision, ArticleSeo, ArticleStatus, ContentCategory, ContentTag,
    PublishWebhook, WebhookEvent,
};
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post},
    Router,
};
use ikeda_core::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

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
            PublishWebhook,
            WebhookEvent,
            WebhookTestResult,
            CreateArticleCommand,
            UpdateArticleCommand,
            UpdateArticleStatusCommand,
            CreateCategoryCommand,
            UpdateCategoryCommand,
            CreateTagCommand,
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
        (name = "Deploy Hooks", description = "SSG Deploy Hooks (Cloudflare Pages, Vercel, Netlify) with anti-SSRF protection"),
        (name = "Internal", description = "Signed Upstash QStash internal callbacks")
    ),
    info(
        title = "IkedaDev Content API",
        version = "0.1.0",
        description = "Multi-tenant Headless CMS supporting SSR (ETag/Cache-Control) and SSG (Deploy Hooks), TipTap editorial content, ammonia HTML sanitization, draft previews, and revision history."
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

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        // Articles
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
        // Internal QStash callback
        .route("/v1/internal/qstash/publish", post(qstash_publish_callback))
        // Taxonomy
        .route("/v1/categories", post(create_category).get(list_categories))
        .route(
            "/v1/categories/{id}",
            patch(update_category).delete(delete_category),
        )
        .route("/v1/tags", post(create_tag).get(list_tags))
        .route("/v1/tags/{id}", delete(delete_tag))
        // SSG Deploy Hooks
        .route("/v1/webhooks", post(create_webhook).get(list_webhooks))
        .route(
            "/v1/webhooks/{id}",
            patch(update_webhook).delete(delete_webhook),
        )
        .route("/v1/webhooks/{id}/test", post(test_webhook))
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}
