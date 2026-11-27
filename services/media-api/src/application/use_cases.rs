use crate::application::rbac::{
    require_context_write_permission, require_media_delete, require_media_read, require_media_write,
};
use crate::domain::models::{
    MediaAsset, MediaContext, PresignedUpload, TenantQuotaLimits, TenantQuotaUsage,
};
use crate::domain::ports::{
    ConfirmMediaData, CreatePendingMediaData, MediaRepository, ObjectStoragePort, UpdateMediaData,
};
use crate::domain::services::{
    build_safe_storage_key, compute_quota_usage, enforce_quota_limits, extension_for_allowed_mime,
    validate_file_name_and_extension, validate_metadata_size,
};
use ikeda_core::criteria::Criteria;
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
    #[serde(default)]
    pub context: Option<MediaContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfirmMediaCommand {
    pub asset_id: Uuid,
    pub alt_text: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateMediaCommand {
    pub alt_text: Option<Option<String>>,
    pub context: Option<MediaContext>,
    pub metadata: Option<serde_json::Value>,
}

/// Validates MIME, extension, file size, tenant quota, and RBAC on `context`,
/// persists a `PENDING` media record, and returns a presigned PUT upload URL.
pub async fn presign_media(
    repo: &dyn MediaRepository,
    storage: &dyn ObjectStoragePort,
    ctx: &TenantContext,
    cmd: PresignMediaCommand,
    default_limits: TenantQuotaLimits,
) -> Result<PresignedUpload, AppError> {
    let context = cmd.context.unwrap_or(MediaContext::General);
    require_context_write_permission(ctx, context)?;

    let ext = extension_for_allowed_mime(&cmd.mime_type)?;
    let safe_file_name = validate_file_name_and_extension(&cmd.file_name, ext)?;

    let (used_bytes, _count, limits) = repo.get_usage(ctx.tenant_id, default_limits).await?;
    enforce_quota_limits(cmd.size_bytes, used_bytes, limits)?;

    let asset_id = Uuid::new_v4();
    let storage_key = build_safe_storage_key(ctx.tenant_id, context, asset_id, ext);
    let normalized_mime = cmd.mime_type.trim().to_ascii_lowercase();

    let presigned = storage
        .generate_presigned_put(&storage_key, &normalized_mime, cmd.size_bytes)
        .await?;

    repo.create_pending(CreatePendingMediaData {
        id: asset_id,
        tenant_id: ctx.tenant_id,
        storage_provider: storage.provider_name().to_string(),
        context,
        file_name: safe_file_name,
        storage_key: storage_key.clone(),
        public_url: presigned.public_url.clone(),
        mime_type: normalized_mime,
        size_bytes: cmd.size_bytes,
        uploaded_by: ctx.user_id,
    })
    .await?;

    Ok(PresignedUpload {
        asset_id,
        storage_provider: storage.provider_name().to_string(),
        context,
        storage_key,
        upload_url: presigned.upload_url,
        public_url: presigned.public_url,
        expires_in_secs: presigned.expires_in_secs,
    })
}

/// Transitions a `PENDING` asset to `READY` and stores editorial metadata (`alt_text`, dimensions, JSONB).
pub async fn confirm_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    cmd: ConfirmMediaCommand,
) -> Result<MediaAsset, AppError> {
    require_media_write(ctx)?;

    let existing = repo
        .find_by_id(ctx.tenant_id, cmd.asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{}' not found", cmd.asset_id)))?;

    require_context_write_permission(ctx, existing.context)?;

    if let Some(w) = cmd.width {
        if w <= 0 {
            return Err(AppError::BadRequest("width must be > 0".to_string()));
        }
    }
    if let Some(h) = cmd.height {
        if h <= 0 {
            return Err(AppError::BadRequest("height must be > 0".to_string()));
        }
    }
    if let Some(ref meta) = cmd.metadata {
        validate_metadata_size(meta)?;
    }

    let alt_text = cmd
        .alt_text
        .map(|s| s.trim().chars().take(255).collect::<String>())
        .filter(|s| !s.is_empty());

    repo.confirm(
        ctx.tenant_id,
        cmd.asset_id,
        ConfirmMediaData {
            alt_text,
            width: cmd.width,
            height: cmd.height,
            metadata: cmd.metadata,
        },
    )
    .await
}

pub async fn list_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    context: Option<MediaContext>,
    page: u64,
    page_size: u64,
) -> Result<(Vec<MediaAsset>, u64), AppError> {
    require_media_read(ctx)?;
    repo.list(ctx.tenant_id, context, page.max(1), page_size.clamp(1, 100))
        .await
}

pub async fn search_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    criteria: &Criteria,
) -> Result<(Vec<MediaAsset>, u64), AppError> {
    require_media_read(ctx)?;
    repo.search(ctx.tenant_id, criteria).await
}

pub async fn get_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<MediaAsset, AppError> {
    require_media_read(ctx)?;
    repo.find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))
}

pub async fn get_quota(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    default_limits: TenantQuotaLimits,
) -> Result<TenantQuotaUsage, AppError> {
    require_media_read(ctx)?;
    let (used_bytes, asset_count, limits) = repo.get_usage(ctx.tenant_id, default_limits).await?;
    Ok(compute_quota_usage(used_bytes, asset_count, limits))
}

pub async fn update_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateMediaCommand,
) -> Result<MediaAsset, AppError> {
    require_media_write(ctx)?;

    let existing = repo
        .find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

    // Must have permission on BOTH current context and target context (protects `branding`)
    require_context_write_permission(ctx, existing.context)?;
    if let Some(target_ctx) = cmd.context {
        require_context_write_permission(ctx, target_ctx)?;
    }

    if let Some(ref meta) = cmd.metadata {
        validate_metadata_size(meta)?;
    }

    let alt_text = cmd.alt_text.map(|opt| {
        opt.map(|s| s.trim().chars().take(255).collect::<String>())
            .filter(|s| !s.is_empty())
    });

    repo.update(
        ctx.tenant_id,
        id,
        UpdateMediaData {
            alt_text,
            context: cmd.context,
            metadata: cmd.metadata,
        },
    )
    .await
}

/// Deletes a media asset (`media:delete` required).
/// - `permanent == false`: Soft Delete (grace period, keeps object in storage).
/// - `permanent == true`: Deletes physical object via `ObjectStoragePort::delete_object` and removes DB record.
pub async fn delete_media(
    repo: &dyn MediaRepository,
    storage: &dyn ObjectStoragePort,
    ctx: &TenantContext,
    id: Uuid,
    permanent: bool,
) -> Result<(), AppError> {
    require_media_delete(ctx)?;

    if permanent {
        let existing = repo
            .find_by_id_including_deleted(ctx.tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

        storage.delete_object(&existing.storage_key).await?;
        let _ = repo.hard_delete(ctx.tenant_id, id).await?;
        Ok(())
    } else {
        let _ = repo.soft_delete(ctx.tenant_id, id).await?;
        Ok(())
    }
}

/// Restores a soft-deleted media asset (`media:delete` required).
pub async fn restore_media(
    repo: &dyn MediaRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<MediaAsset, AppError> {
    require_media_delete(ctx)?;
    repo.restore(ctx.tenant_id, id).await
}
