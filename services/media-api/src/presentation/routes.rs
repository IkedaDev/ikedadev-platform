use super::handlers::{self, *};
use super::state::AppState;
use crate::application::use_cases::{ConfirmMediaCommand, PresignMediaCommand, UpdateMediaCommand};
use crate::domain::models::{
    MediaAsset, MediaContext, MediaStatus, PresignedUpload, TenantQuotaUsage,
};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Method},
    routing::{get, post, put},
    Router,
};
use ikeda_core::prelude::*;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
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
        handlers::presign_media,
        handlers::confirm_media,
        handlers::list_media,
        handlers::search_media,
        handlers::get_quota,
        handlers::get_media,
        handlers::update_media,
        handlers::delete_media,
        handlers::restore_media
    ),
    components(
        schemas(
            HealthStatus,
            ApiResponse<HealthStatus>,
            ApiResponse<PresignedUpload>,
            ApiResponse<MediaAsset>,
            PaginatedApiResponse<MediaAsset>,
            ApiResponse<TenantQuotaUsage>,
            MediaAsset,
            MediaContext,
            MediaStatus,
            PresignedUpload,
            TenantQuotaUsage,
            PresignMediaCommand,
            ConfirmMediaCommand,
            UpdateMediaCommand,
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
        (name = "Media", description = "Multi-tenant transversal media library, quotas, and presigned uploads (Cloudflare R2 / BYOS)")
    ),
    info(
        title = "IkedaDev Media API",
        version = "0.1.0",
        description = "Transversal multi-tenant Media Library microservice with PostgreSQL RLS, per-tenant quotas, RBAC context protection, Soft Delete + restore, and Cloudflare R2 / BYOS Object Storage."
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
        service: "media-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub fn create_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/v1/media/presign", post(presign_media))
        .route("/v1/media/confirm", post(confirm_media))
        .route("/v1/media/search", post(search_media))
        .route("/v1/media/quota", get(get_quota))
        .route("/v1/media", get(list_media))
        .route(
            "/v1/media/{id}",
            get(get_media).patch(update_media).delete(delete_media),
        )
        .route("/v1/media/{id}/restore", post(restore_media))
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024));

    // Mount dev-only local upload/serve endpoints ONLY when LocalDevObjectStorage is explicitly enabled
    if state.local_storage.is_some() {
        let max_upload = usize::try_from(state.default_quotas.max_file_size_bytes)
            .unwrap_or(5 * 1024 * 1024)
            .saturating_add(64 * 1024);

        let local_cors = match HeaderValue::from_str(state.local_cors_origin.trim()) {
            Ok(origin) => CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::PUT, Method::GET, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE]),
            Err(_) => CorsLayer::new()
                .allow_methods([Method::PUT, Method::GET, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE]),
        };

        let local_routes = Router::new()
            .route(
                "/v1/media/local/{tenant_id}/{context}/{filename}",
                put(upload_local_media).get(serve_local_media),
            )
            .layer(DefaultBodyLimit::max(max_upload))
            .layer(local_cors);

        router = router.merge(local_routes);
    }

    router.with_state(state)
}
