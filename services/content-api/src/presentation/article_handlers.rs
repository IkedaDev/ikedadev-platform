use super::state::AppState;
use crate::application::preview_token::PreviewTokenResponse;
use crate::application::use_cases::{
    article_use_cases, CreateArticleCommand, UpdateArticleCommand, UpdateArticleStatusCommand,
};
use crate::domain::models::{Article, ArticleRevision, ArticleStatus};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ikeda_core::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/v1/articles",
    tag = "Articles",
    request_body = CreateArticleCommand,
    responses(
        (status = 201, description = "Article created in DRAFT status", body = ApiResponse<Article>),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Missing content:write scope"),
        (status = 409, description = "Duplicate slug within tenant")
    )
)]
pub async fn create_article(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateArticleCommand>,
) -> Result<(StatusCode, ApiResponse<Article>), AppError> {
    let article = article_use_cases::create_article(
        state.article_repo.as_ref(),
        &ctx,
        cmd,
        state.allow_http_urls,
    )
    .await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(article)))
}

#[utoipa::path(
    get,
    path = "/v1/articles/{id}",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    responses(
        (status = 200, description = "Article found", body = ApiResponse<Article>),
        (status = 404, description = "Article not found")
    )
)]
pub async fn get_article(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<Article>, AppError> {
    let article = article_use_cases::get_article(state.article_repo.as_ref(), &ctx, id).await?;
    Ok(ApiResponse::ok(article))
}

#[utoipa::path(
    get,
    path = "/v1/articles/by-slug/{slug}",
    tag = "Articles",
    params(("slug" = String, Path, description = "Article slug")),
    responses(
        (status = 200, description = "Article found with ETag and Cache-Control headers", body = ApiResponse<Article>),
        (status = 304, description = "Not Modified (ETag matched)"),
        (status = 404, description = "Article not found")
    )
)]
pub async fn get_article_by_slug(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    let article =
        article_use_cases::get_article_by_slug(state.article_repo.as_ref(), &ctx, &slug).await?;

    let etag_input = format!(
        "{}:{}:{}",
        article.id,
        article.updated_at.timestamp_micros(),
        article.status.as_str()
    );
    let etag_hash = hex::encode(Sha256::digest(etag_input.as_bytes()));
    let etag_value = format!("\"{}\"", &etag_hash[..24]);

    let cache_control = if article.status == ArticleStatus::Published {
        "public, max-age=60, stale-while-revalidate=300"
    } else {
        "private, no-cache"
    };

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none_match.trim() == etag_value {
            let mut resp = StatusCode::NOT_MODIFIED.into_response();
            if let Ok(val) = HeaderValue::from_str(&etag_value) {
                resp.headers_mut().insert(header::ETAG, val);
            }
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            );
            return Ok(resp);
        }
    }

    let mut resp = ApiResponse::ok(article).into_response();
    if let Ok(val) = HeaderValue::from_str(&etag_value) {
        resp.headers_mut().insert(header::ETAG, val);
    }
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    Ok(resp)
}

#[utoipa::path(
    post,
    path = "/v1/articles/search",
    tag = "Articles",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated article search results", body = PaginatedApiResponse<Article>)
    )
)]
pub async fn search_articles(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<PaginatedApiResponse<Article>, AppError> {
    let pagination = criteria.pagination.unwrap_or_default();
    let (items, total) =
        article_use_cases::search_articles(state.article_repo.as_ref(), &ctx, &criteria).await?;
    Ok(PaginatedApiResponse::new(
        items,
        pagination.page,
        pagination.limit,
        total,
    ))
}

#[utoipa::path(
    patch,
    path = "/v1/articles/{id}",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    request_body = UpdateArticleCommand,
    responses(
        (status = 200, description = "Article updated", body = ApiResponse<Article>),
        (status = 403, description = "Forbidden (staff cannot edit non-DRAFT articles)"),
        (status = 404, description = "Article not found")
    )
)]
pub async fn update_article(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateArticleCommand>,
) -> Result<ApiResponse<Article>, AppError> {
    let article = article_use_cases::update_article(
        state.article_repo.as_ref(),
        state.webhook_repo.clone(),
        state.http_client.clone(),
        &ctx,
        id,
        cmd,
        state.allow_http_urls,
    )
    .await?;
    Ok(ApiResponse::ok(article))
}

#[utoipa::path(
    patch,
    path = "/v1/articles/{id}/status",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    request_body = UpdateArticleStatusCommand,
    responses(
        (status = 200, description = "Article status updated", body = ApiResponse<Article>),
        (status = 403, description = "Missing content:publish scope"),
        (status = 404, description = "Article not found")
    )
)]
pub async fn update_article_status(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateArticleStatusCommand>,
) -> Result<ApiResponse<Article>, AppError> {
    let article = article_use_cases::update_article_status(
        state.article_repo.as_ref(),
        state.scheduler.as_ref(),
        state.webhook_repo.clone(),
        state.http_client.clone(),
        &ctx,
        id,
        cmd,
    )
    .await?;
    Ok(ApiResponse::ok(article))
}

#[utoipa::path(
    delete,
    path = "/v1/articles/{id}",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    responses(
        (status = 204, description = "Article soft-deleted"),
        (status = 403, description = "Missing content:publish scope"),
        (status = 404, description = "Article not found")
    )
)]
pub async fn delete_article(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    article_use_cases::delete_article(
        state.article_repo.as_ref(),
        state.webhook_repo.clone(),
        state.http_client.clone(),
        &ctx,
        id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/articles/{id}/revisions",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    responses(
        (status = 200, description = "Article revision history", body = ApiResponse<Vec<ArticleRevision>>)
    )
)]
pub async fn list_article_revisions(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<Vec<ArticleRevision>>, AppError> {
    let revisions =
        article_use_cases::list_article_revisions(state.article_repo.as_ref(), &ctx, id).await?;
    Ok(ApiResponse::ok(revisions))
}

#[utoipa::path(
    post,
    path = "/v1/articles/{id}/revisions/{revision_id}/restore",
    tag = "Articles",
    params(
        ("id" = Uuid, Path, description = "Article UUID"),
        ("revision_id" = Uuid, Path, description = "Revision UUID")
    ),
    responses(
        (status = 200, description = "Revision restored", body = ApiResponse<Article>),
        (status = 403, description = "Missing content:publish scope")
    )
)]
pub async fn restore_article_revision(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse<Article>, AppError> {
    let article = article_use_cases::restore_article_revision(
        state.article_repo.as_ref(),
        state.webhook_repo.clone(),
        state.http_client.clone(),
        &ctx,
        id,
        revision_id,
    )
    .await?;
    Ok(ApiResponse::ok(article))
}

#[utoipa::path(
    post,
    path = "/v1/articles/{id}/preview-token",
    tag = "Articles",
    params(("id" = Uuid, Path, description = "Article UUID")),
    responses(
        (status = 200, description = "Signed temporary preview token (30 min)", body = ApiResponse<PreviewTokenResponse>)
    )
)]
pub async fn generate_preview_token(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<PreviewTokenResponse>, AppError> {
    let token_res = article_use_cases::generate_preview_token(
        state.article_repo.as_ref(),
        &state.preview_tokens,
        &ctx,
        id,
    )
    .await?;
    Ok(ApiResponse::ok(token_res))
}

/// Self-signed draft preview endpoint (no `TenantContext` extractor; validates HMAC/JWT token).
#[utoipa::path(
    get,
    path = "/v1/articles/preview",
    tag = "Articles",
    params(("token" = String, Query, description = "Signed temporary preview token")),
    responses(
        (status = 200, description = "Draft article resolved from signed preview token", body = ApiResponse<Article>),
        (status = 401, description = "Invalid or expired preview token"),
        (status = 404, description = "Article not found")
    )
)]
pub async fn preview_article(
    State(state): State<AppState>,
    Query(query): Query<PreviewQuery>,
) -> Result<ApiResponse<Article>, AppError> {
    let article = article_use_cases::resolve_article_preview(
        state.article_repo.as_ref(),
        &state.preview_tokens,
        &query.token,
    )
    .await?;
    Ok(ApiResponse::ok(article))
}

/// Internal Upstash QStash callback endpoint (no `TenantContext`; verifies `Upstash-Signature`).
#[utoipa::path(
    post,
    path = "/v1/internal/qstash/publish",
    tag = "Internal",
    request_body(content = serde_json::Value, description = "Scheduled publish payload signed by Upstash QStash"),
    responses(
        (status = 200, description = "Scheduled article published", body = ApiResponse<Article>),
        (status = 401, description = "Invalid Upstash-Signature")
    )
)]
pub async fn qstash_publish_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<ApiResponse<Article>, AppError> {
    let signature = headers
        .get("Upstash-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let article = article_use_cases::handle_qstash_scheduled_publish(
        state.article_repo.as_ref(),
        state.scheduler.as_ref(),
        state.webhook_repo.clone(),
        state.http_client.clone(),
        signature,
        &body,
    )
    .await?;

    Ok(ApiResponse::ok(article))
}
