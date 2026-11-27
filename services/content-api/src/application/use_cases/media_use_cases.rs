use crate::application::rbac::{require_content_read, require_content_write};
use crate::domain::models::{MediaAsset, PresignedUpload};
use crate::domain::ports::{CreateMediaData, MediaRepository, ObjectStoragePort};
use crate::domain::services::{extension_for_mime, validate_public_url, MAX_MEDIA_SIZE_BYTES};
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignMediaCommand {
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfirmMediaCommand {
    pub asset_id: Uuid,
    pub file_name: String,
    pub storage_key: String,
    pub public_url: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateMediaCommand {
    pub alt_text: Option<String>,
}

pub async fn presign_media(
    storage: &dyn ObjectStoragePort,
    ctx: &TenantContext,
    cmd: PresignMediaCommand,
) -> Result<PresignedUpload, AppError> {
    require_content_write(ctx)?;

    let file_name = cmd.file_name.trim();
    if file_name.is_empty() || file_name.len() > 255 {
        return Err(AppError::BadRequest(
            "file_name must be between 1 and 255 characters".to_string(),
        ));
    }

    if cmd.size_bytes <= 0 || cmd.size_bytes > MAX_MEDIA_SIZE_BYTES {
        return Err(AppError::BadRequest(format!(
            "size_bytes must be between 1 and {MAX_MEDIA_SIZE_BYTES} bytes (5 MiB)"
        )));
    }

    let ext = extension_for_mime(&cmd.mime_type)?;
    let asset_id = Uuid::new_v4();

    storage
        .generate_presigned_upload(ctx.tenant_id, asset_id, ext, &cmd.mime_type, cmd.size_bytes)
        .await
}

pub async fn confirm_media(
    repo: &dyn MediaRepository,
    storage: &dyn ObjectStoragePort,
    ctx: &TenantContext,
    cmd: ConfirmMediaCommand,
    allow_http_urls: bool,
) -> Result<MediaAsset, AppError> {
    require_content_write(ctx)?;

    let file_name = cmd.file_name.trim().to_string();
    if file_name.is_empty() || file_name.len() > 255 {
        return Err(AppError::BadRequest(
            "file_name must be between 1 and 255 characters".to_string(),
        ));
    }

    if cmd.size_bytes <= 0 || cmd.size_bytes > MAX_MEDIA_SIZE_BYTES {
        return Err(AppError::BadRequest(format!(
            "size_bytes must be between 1 and {MAX_MEDIA_SIZE_BYTES} bytes (5 MiB)"
        )));
    }

    let _ = extension_for_mime(&cmd.mime_type)?;

    let expected_prefix = format!("tenants/{}/media/", ctx.tenant_id);
    let storage_key = cmd.storage_key.trim().to_string();
    if !storage_key.starts_with(&expected_prefix) || storage_key.contains("..") {
        return Err(AppError::BadRequest(
            "storage_key must belong to the authenticated tenant media prefix".to_string(),
        ));
    }

    let public_url = match cmd.public_url {
        Some(ref u) if !u.trim().is_empty() => {
            validate_public_url(u, "public_url", allow_http_urls)?
        }
        _ => storage.public_url_for_key(&storage_key),
    };

    let alt_text = cmd
        .alt_text
        .map(|s| s.trim().chars().take(255).collect::<String>())
        .filter(|s| !s.is_empty());

    repo.create(CreateMediaData {
        id: cmd.asset_id,
        tenant_id: ctx.tenant_id,
        storage_provider: storage.provider_name().to_string(),
        file_name,
        storage_key,
        public_url,
        mime_type: cmd.mime_type.trim().to_ascii_lowercase(),
        size_bytes: cmd.size_bytes,
        alt_text,
        uploaded_by: ctx.user_id,
    })
    .await
}

pub async fn list_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    page: u64,
    page_size: u64,
) -> Result<(Vec<MediaAsset>, u64), AppError> {
    require_content_read(ctx)?;
    let clamped_page = page.max(1);
    let clamped_size = page_size.clamp(1, 100);
    repo.list(ctx.tenant_id, clamped_page, clamped_size).await
}

pub async fn update_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateMediaCommand,
) -> Result<MediaAsset, AppError> {
    require_content_write(ctx)?;
    let alt_text = cmd
        .alt_text
        .map(|s| s.trim().chars().take(255).collect::<String>())
        .filter(|s| !s.is_empty());
    repo.update_alt_text(ctx.tenant_id, id, alt_text).await
}

pub async fn delete_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<(), AppError> {
    require_content_write(ctx)?;
    repo.soft_delete(ctx.tenant_id, id).await
}
