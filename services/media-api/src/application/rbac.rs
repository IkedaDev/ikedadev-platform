use crate::domain::models::MediaContext;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;

/// Checks if the caller is a tenant or platform administrator.
#[must_use]
pub fn is_tenant_admin(ctx: &TenantContext) -> bool {
    ctx.has_scope("tenant:admin") || ctx.has_scope("platform:admin")
}

/// Checks if the caller has `media:delete` privileges (`tenant:admin` / `platform:admin` / `media:delete`).
/// `staff` / `tenant:member` without `media:delete` returns `false`.
#[must_use]
pub fn has_media_delete(ctx: &TenantContext) -> bool {
    is_tenant_admin(ctx) || ctx.has_scope("media:delete")
}

/// Checks if the caller has `media:write` privileges.
#[must_use]
pub fn has_media_write(ctx: &TenantContext) -> bool {
    has_media_delete(ctx)
        || ctx.has_scope("media:write")
        || ctx.has_scope("staff")
        || ctx.has_scope("tenant:member")
}

/// Checks if the caller has `media:read` privileges.
#[must_use]
pub fn has_media_read(ctx: &TenantContext) -> bool {
    has_media_write(ctx) || ctx.has_scope("media:read")
}

pub fn require_media_read(ctx: &TenantContext) -> Result<(), AppError> {
    if has_media_read(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'media:read'".to_string(),
        ))
    }
}

pub fn require_media_write(ctx: &TenantContext) -> Result<(), AppError> {
    if has_media_write(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'media:write'".to_string(),
        ))
    }
}

pub fn require_media_delete(ctx: &TenantContext) -> Result<(), AppError> {
    if has_media_delete(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'media:delete' (tenant:admin or platform:admin required)"
                .to_string(),
        ))
    }
}

/// Extra AppSec rule: modifying or uploading assets in `branding` context requires `tenant:admin` or `platform:admin`.
pub fn require_context_write_permission(
    ctx: &TenantContext,
    context: MediaContext,
) -> Result<(), AppError> {
    require_media_write(ctx)?;
    if context == MediaContext::Branding && !is_tenant_admin(ctx) {
        return Err(AppError::Forbidden(
            "Only tenant:admin or platform:admin may upload or modify 'branding' media assets"
                .to_string(),
        ));
    }
    Ok(())
}
