use super::state::AppState;
use crate::application::use_cases::{
    media_use_cases, ConfirmMediaCommand, PresignMediaCommand, UpdateMediaCommand,
};
use crate::domain::models::{MediaAsset, PresignedUpload};
use crate::domain::services::MAX_MEDIA_SIZE_BYTES;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ikeda_core::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct MediaListQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
}

fn default_page() -> u64 {
    1
}

fn default_page_size() -> u64 {
    24
}

#[utoipa::path(
    post,
    path = "/v1/media/presign",
    tag = "Media",
    request_body = PresignMediaCommand,
    responses(
        (status = 200, description = "Presigned upload URL generated", body = ApiResponse<PresignedUpload>),
        (status = 400, description = "Invalid MIME type or file size > 5 MiB")
    )
)]
pub async fn presign_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<PresignMediaCommand>,
) -> Result<ApiResponse<PresignedUpload>, AppError> {
    let presigned =
        media_use_cases::presign_media(state.object_storage.as_ref(), &ctx, cmd).await?;
    Ok(ApiResponse::ok(presigned))
}

#[utoipa::path(
    post,
    path = "/v1/media/confirm",
    tag = "Media",
    request_body = ConfirmMediaCommand,
    responses(
        (status = 201, description = "Media asset registered", body = ApiResponse<MediaAsset>)
    )
)]
pub async fn confirm_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<ConfirmMediaCommand>,
) -> Result<(StatusCode, ApiResponse<MediaAsset>), AppError> {
    let asset = media_use_cases::confirm_media(
        state.media_repo.as_ref(),
        state.object_storage.as_ref(),
        &ctx,
        cmd,
        state.allow_http_urls,
    )
    .await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(asset)))
}

#[utoipa::path(
    get,
    path = "/v1/media",
    tag = "Media",
    params(
        ("page" = Option<u64>, Query, description = "Page number (default 1)"),
        ("page_size" = Option<u64>, Query, description = "Page size (default 24, max 100)")
    ),
    responses(
        (status = 200, description = "Paginated media library", body = PaginatedApiResponse<MediaAsset>)
    )
)]
pub async fn list_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(query): Query<MediaListQuery>,
) -> Result<PaginatedApiResponse<MediaAsset>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);
    let (items, total) =
        media_use_cases::list_media(state.media_repo.as_ref(), &ctx, page, page_size).await?;
    Ok(PaginatedApiResponse::new(items, page, page_size, total))
}

#[utoipa::path(
    patch,
    path = "/v1/media/{id}",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Media asset UUID")),
    request_body = UpdateMediaCommand,
    responses(
        (status = 200, description = "Media asset updated", body = ApiResponse<MediaAsset>),
        (status = 404, description = "Media asset not found")
    )
)]
pub async fn update_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateMediaCommand>,
) -> Result<ApiResponse<MediaAsset>, AppError> {
    let asset = media_use_cases::update_media(state.media_repo.as_ref(), &ctx, id, cmd).await?;
    Ok(ApiResponse::ok(asset))
}

#[utoipa::path(
    delete,
    path = "/v1/media/{id}",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Media asset UUID")),
    responses(
        (status = 204, description = "Media asset soft-deleted"),
        (status = 404, description = "Media asset not found")
    )
)]
pub async fn delete_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    media_use_cases::delete_media(state.media_repo.as_ref(), &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Validates a local dev media filename (`<uuid>.<webp|jpg|png|avif>`) and returns its MIME type.
fn validate_local_media_filename(filename: &str) -> Result<&'static str, AppError> {
    let (stem, ext) = filename
        .rsplit_once('.')
        .ok_or_else(|| AppError::BadRequest("Invalid media filename".to_string()))?;
    Uuid::parse_str(stem).map_err(|_| {
        AppError::BadRequest("Media filename stem must be a valid UUID".to_string())
    })?;
    match ext {
        "webp" => Ok("image/webp"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "png" => Ok("image/png"),
        "avif" => Ok("image/avif"),
        _ => Err(AppError::BadRequest(
            "Unsupported local media file extension".to_string(),
        )),
    }
}

/// Local development endpoint for receiving presigned PUT uploads when R2 is not configured.
pub async fn upload_local_media(
    State(state): State<AppState>,
    Path((tenant_id, filename)): Path<(Uuid, String)>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let base_dir = state
        .local_media_dir
        .as_ref()
        .ok_or_else(|| AppError::NotFound("Local media storage is disabled".to_string()))?;

    let _mime = validate_local_media_filename(&filename)?;
    if body.is_empty() || i64::try_from(body.len()).unwrap_or(i64::MAX) > MAX_MEDIA_SIZE_BYTES {
        return Err(AppError::BadRequest(
            "Media payload must be between 1 byte and 5 MiB".to_string(),
        ));
    }

    let tenant_dir = base_dir.join(tenant_id.to_string());
    tokio::fs::create_dir_all(&tenant_dir).await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to create local media directory: {e}"))
    })?;

    let target = tenant_dir.join(&filename);
    tokio::fs::write(&target, &body).await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to write local media file: {e}"))
    })?;

    Ok(StatusCode::OK)
}

/// Local development endpoint for serving uploaded media images via localhost when R2 is not configured.
pub async fn serve_local_media(
    State(state): State<AppState>,
    Path((tenant_id, filename)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let base_dir = state
        .local_media_dir
        .as_ref()
        .ok_or_else(|| AppError::NotFound("Local media storage is disabled".to_string()))?;

    let mime = validate_local_media_filename(&filename)?;
    let target = base_dir.join(tenant_id.to_string()).join(&filename);

    let bytes = tokio::fs::read(&target)
        .await
        .map_err(|_| AppError::NotFound("Local media file not found".to_string()))?;

    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok(resp)
}
