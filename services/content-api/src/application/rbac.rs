use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;

/// Checks if the caller has editorial publish/admin privileges.
/// `staff` / `tenant:member` and plain `content:write` are NOT granted publish rights.
#[must_use]
pub fn has_content_publish(ctx: &TenantContext) -> bool {
    ctx.has_scope("content:publish")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
}

/// Checks if the caller has editorial write privileges (creating/editing drafts, taxonomies, media).
#[must_use]
pub fn has_content_write(ctx: &TenantContext) -> bool {
    has_content_publish(ctx)
        || ctx.has_scope("content:write")
        || ctx.has_scope("staff")
        || ctx.has_scope("tenant:member")
}

/// Checks if the caller has editorial read privileges.
#[must_use]
pub fn has_content_read(ctx: &TenantContext) -> bool {
    has_content_write(ctx) || ctx.has_scope("content:read")
}

/// Enforces `content:read` (or `staff`/`tenant:member`/`tenant:admin`/`platform:admin`).
pub fn require_content_read(ctx: &TenantContext) -> Result<(), AppError> {
    if has_content_read(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'content:read'".to_string(),
        ))
    }
}

/// Enforces `content:write` (or `staff`/`tenant:member`/`tenant:admin`/`platform:admin`).
pub fn require_content_write(ctx: &TenantContext) -> Result<(), AppError> {
    if has_content_write(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'content:write'".to_string(),
        ))
    }
}

/// Enforces `content:publish` (or `tenant:admin`/`platform:admin`).
pub fn require_content_publish(ctx: &TenantContext) -> Result<(), AppError> {
    if has_content_publish(ctx) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'content:publish' (tenant:admin or platform:admin required)"
                .to_string(),
        ))
    }
}
