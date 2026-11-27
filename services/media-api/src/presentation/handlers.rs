use super::state::AppState;
use crate::application::use_cases::{
    self, ConfirmMediaCommand, PresignMediaCommand, UpdateMediaCommand,
};
use crate::domain::models::{MediaAsset, MediaContext, PresignedUpload, TenantQuotaUsage};
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
pub struct ListMediaQuery {
    pub context: Option<String>,
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeleteMediaQuery {
    #[serde(default)]
    pub permanent: bool,
}

#[derive(Debug, Deserialize)]
pub struct LocalSignedPutQuery {
    pub exp: i64,
    pub sig: String,
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
        (status = 200, description = "Presigned PUT upload URL generated", body = ApiResponse<PresignedUpload>),
        (status = 400, description = "Invalid MIME type or extension mismatch"),
        (status = 403, description = "Forbidden (e.g. staff uploading to branding)"),
        (status = 422, description = "File size or tenant storage quota exceeded")
    )
)]
pub async fn presign_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<PresignMediaCommand>,
) -> Result<ApiResponse<PresignedUpload>, AppError> {
    let presigned = use_cases::presign_media(
        state.media_repo.as_ref(),
        state.object_storage.as_ref(),
        &ctx,
        cmd,
        state.default_quotas,
    )
    .await?;
    Ok(ApiResponse::ok(presigned))
}

#[utoipa::path(
    post,
    path = "/v1/media/confirm",
    tag = "Media",
    request_body = ConfirmMediaCommand,
    responses(
        (status = 200, description = "Media asset marked READY", body = ApiResponse<MediaAsset>),
        (status = 404, description = "Pending media asset not found")
    )
)]
pub async fn confirm_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<ConfirmMediaCommand>,
) -> Result<ApiResponse<MediaAsset>, AppError> {
    let asset = use_cases::confirm_media(state.media_repo.as_ref(), &ctx, cmd).await?;
    Ok(ApiResponse::ok(asset))
}

#[utoipa::path(
    get,
    path = "/v1/media",
    tag = "Media",
    params(
        ("context" = Option<String>, Query, description = "Filter by context (general, branding, catalog, content, avatars)"),
        ("page" = Option<u64>, Query, description = "Page number (default 1)"),
        ("page_size" = Option<u64>, Query, description = "Page size (default 24, max 100)")
    ),
    responses(
        (status = 200, description = "Paginated READY media assets", body = PaginatedApiResponse<MediaAsset>)
    )
)]
pub async fn list_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(query): Query<ListMediaQuery>,
) -> Result<PaginatedApiResponse<MediaAsset>, AppError> {
    let context = match query
        .context
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(MediaContext::parse(raw)?),
        None => None,
    };
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);
    let (items, total) =
        use_cases::list_media(state.media_repo.as_ref(), &ctx, context, page, page_size).await?;
    Ok(PaginatedApiResponse::new(items, page, page_size, total))
}

#[utoipa::path(
    post,
    path = "/v1/media/search",
    tag = "Media",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated media search results", body = PaginatedApiResponse<MediaAsset>)
    )
)]
pub async fn search_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<PaginatedApiResponse<MediaAsset>, AppError> {
    let pagination = criteria.pagination.unwrap_or_default();
    let (items, total) =
        use_cases::search_media(state.media_repo.as_ref(), &ctx, &criteria).await?;
    Ok(PaginatedApiResponse::new(
        items,
        pagination.page,
        pagination.limit,
        total,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/media/quota",
    tag = "Media",
    responses(
        (status = 200, description = "Tenant storage quota usage", body = ApiResponse<TenantQuotaUsage>)
    )
)]
pub async fn get_quota(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<ApiResponse<TenantQuotaUsage>, AppError> {
    let usage = use_cases::get_quota(state.media_repo.as_ref(), &ctx, state.default_quotas).await?;
    Ok(ApiResponse::ok(usage))
}

#[utoipa::path(
    get,
    path = "/v1/media/{id}",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Media asset UUID")),
    responses(
        (status = 200, description = "Media asset detail", body = ApiResponse<MediaAsset>),
        (status = 404, description = "Media asset not found")
    )
)]
pub async fn get_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<MediaAsset>, AppError> {
    let asset = use_cases::get_media(state.media_repo.as_ref(), &ctx, id).await?;
    Ok(ApiResponse::ok(asset))
}

#[utoipa::path(
    patch,
    path = "/v1/media/{id}",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Media asset UUID")),
    request_body = UpdateMediaCommand,
    responses(
        (status = 200, description = "Media asset updated", body = ApiResponse<MediaAsset>),
        (status = 403, description = "Forbidden (staff cannot modify branding context)"),
        (status = 404, description = "Media asset not found")
    )
)]
pub async fn update_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateMediaCommand>,
) -> Result<ApiResponse<MediaAsset>, AppError> {
    let asset = use_cases::update_media(state.media_repo.as_ref(), &ctx, id, cmd).await?;
    Ok(ApiResponse::ok(asset))
}

#[utoipa::path(
    delete,
    path = "/v1/media/{id}",
    tag = "Media",
    params(
        ("id" = Uuid, Path, description = "Media asset UUID"),
        ("permanent" = Option<bool>, Query, description = "If true, deletes physical object from storage and removes DB row")
    ),
    responses(
        (status = 204, description = "Media asset deleted (soft or permanent)"),
        (status = 403, description = "Missing media:delete scope"),
        (status = 404, description = "Media asset not found")
    )
)]
pub async fn delete_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteMediaQuery>,
) -> Result<StatusCode, AppError> {
    use_cases::delete_media(
        state.media_repo.as_ref(),
        state.object_storage.as_ref(),
        &ctx,
        id,
        query.permanent,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/media/{id}/restore",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Media asset UUID")),
    responses(
        (status = 200, description = "Soft-deleted media asset restored", body = ApiResponse<MediaAsset>),
        (status = 403, description = "Missing media:delete scope"),
        (status = 404, description = "Deleted media asset not found")
    )
)]
pub async fn restore_media(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse<MediaAsset>, AppError> {
    let asset = use_cases::restore_media(state.media_repo.as_ref(), &ctx, id).await?;
    Ok(ApiResponse::ok(asset))
}

fn validate_local_filename(filename: &str) -> Result<&'static str, AppError> {
    let (stem, ext) = filename
        .rsplit_once('.')
        .ok_or_else(|| AppError::BadRequest("Invalid media filename".to_string()))?;
    Uuid::parse_str(stem)
        .map_err(|_| AppError::BadRequest("Filename must use a valid UUID stem".to_string()))?;
    match ext {
        "webp" => Ok("image/webp"),
        "jpg" => Ok("image/jpeg"),
        "png" => Ok("image/png"),
        "avif" => Ok("image/avif"),
        _ => Err(AppError::BadRequest(
            "Unsupported image extension".to_string(),
        )),
    }
}

/// Dev-only HMAC-verified local PUT endpoint (mounted ONLY when `ALLOW_LOCAL_MEDIA_STORAGE=true`).
pub async fn upload_local_media(
    State(state): State<AppState>,
    Path((tenant_id, context_str, filename)): Path<(Uuid, String, String)>,
    Query(query): Query<LocalSignedPutQuery>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let local = state
        .local_storage
        .as_ref()
        .ok_or_else(|| AppError::NotFound("Local media storage is disabled".to_string()))?;

    let context = MediaContext::parse(&context_str)?;
    let _mime = validate_local_filename(&filename)?;
    let storage_key = format!("tenants/{tenant_id}/{}/{filename}", context.as_str());

    local.verify_upload_signature(&storage_key, query.exp, &query.sig)?;

    if body.is_empty()
        || i64::try_from(body.len()).unwrap_or(i64::MAX) > state.default_quotas.max_file_size_bytes
    {
        return Err(AppError::UnprocessableEntity {
            message: "Uploaded file size is empty or exceeds max_file_size_bytes".to_string(),
            errors: Vec::new(),
        });
    }

    let dir = local
        .storage_dir()
        .join(tenant_id.to_string())
        .join(context.as_str());
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to create local media directory: {e}"))
    })?;

    tokio::fs::write(dir.join(&filename), &body)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to write local media file: {e}"))
        })?;

    Ok(StatusCode::OK)
}

/// Dev-only local GET endpoint serving uploaded images on localhost.
pub async fn serve_local_media(
    State(state): State<AppState>,
    Path((tenant_id, context_str, filename)): Path<(Uuid, String, String)>,
) -> Result<Response, AppError> {
    let local = state
        .local_storage
        .as_ref()
        .ok_or_else(|| AppError::NotFound("Local media storage is disabled".to_string()))?;

    let context = MediaContext::parse(&context_str)?;
    let mime = validate_local_filename(&filename)?;
    let path = local
        .storage_dir()
        .join(tenant_id.to_string())
        .join(context.as_str())
        .join(&filename);

    let bytes = tokio::fs::read(&path)
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
