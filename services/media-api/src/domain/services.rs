use crate::domain::models::{MediaContext, TenantQuotaLimits, TenantQuotaUsage};
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Default maximum storage quota per tenant (500 MiB).
pub const DEFAULT_MAX_STORAGE_BYTES: i64 = 524_288_000;
/// Default maximum single file upload size (5 MiB).
pub const DEFAULT_MAX_FILE_SIZE_BYTES: i64 = 5_242_880;
/// Maximum serialized byte size for `metadata` JSONB (16 KiB).
pub const MAX_METADATA_JSON_BYTES: usize = 16 * 1024;

/// Strictly validates MIME type against the allowed image whitelist and returns its canonical extension.
/// Rejects SVGs, HTML, scripts, executables, and unknown types.
pub fn extension_for_allowed_mime(mime_type: &str) -> Result<&'static str, AppError> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/webp" => Ok("webp"),
        "image/jpeg" => Ok("jpg"),
        "image/png" => Ok("png"),
        "image/avif" => Ok("avif"),
        other => Err(AppError::BadRequest(format!(
            "Unsupported MIME type '{other}'. Allowed: image/webp, image/jpeg, image/png, image/avif"
        ))),
    }
}

/// Validates that if the client-provided `file_name` includes an extension, it matches the validated MIME type.
pub fn validate_file_name_and_extension(
    raw_file_name: &str,
    expected_ext: &str,
) -> Result<String, AppError> {
    // Strip any directory separators / path traversal segments and control chars
    let leaf = raw_file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim();

    let sanitized: String = leaf
        .chars()
        .filter(|c| !c.is_control() && *c != '\0')
        .take(255)
        .collect();

    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        return Err(AppError::BadRequest(
            "file_name must be a valid non-empty file name".to_string(),
        ));
    }

    if let Some((_, raw_ext)) = sanitized.rsplit_once('.') {
        let ext_lower = raw_ext.to_ascii_lowercase();
        let matches = match expected_ext {
            "jpg" => ext_lower == "jpg" || ext_lower == "jpeg",
            other => ext_lower == other,
        };
        if !matches {
            return Err(AppError::BadRequest(format!(
                "File extension '.{ext_lower}' does not match MIME type (expected '.{expected_ext}')"
            )));
        }
    }

    Ok(sanitized)
}

/// Builds a strictly safe, backend-controlled storage key immune to path traversal.
/// Always produces `tenants/{tenant_id}/{context}/{asset_id}.{ext}`.
#[must_use]
pub fn build_safe_storage_key(
    tenant_id: Uuid,
    context: MediaContext,
    asset_id: Uuid,
    validated_ext: &str,
) -> String {
    format!(
        "tenants/{}/{}/{}.{}",
        tenant_id,
        context.as_str(),
        asset_id,
        validated_ext
    )
}

/// Validates serialized JSONB metadata size.
pub fn validate_metadata_size(metadata: &serde_json::Value) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(metadata)
        .map_err(|e| AppError::BadRequest(format!("Invalid metadata JSON: {e}")))?
        .len();
    if bytes > MAX_METADATA_JSON_BYTES {
        return Err(AppError::BadRequest(format!(
            "metadata exceeds maximum size of {MAX_METADATA_JSON_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Checks single-file size and total tenant quota before allowing a presigned upload.
/// Returns `422 Unprocessable Entity` when limits are exceeded.
pub fn enforce_quota_limits(
    size_bytes: i64,
    used_bytes: i64,
    limits: TenantQuotaLimits,
) -> Result<(), AppError> {
    if size_bytes <= 0 {
        return Err(AppError::BadRequest(
            "size_bytes must be greater than 0".to_string(),
        ));
    }
    if size_bytes > limits.max_file_size_bytes {
        return Err(AppError::UnprocessableEntity {
            message: format!(
                "File size ({size_bytes} bytes) exceeds maximum allowed file size ({} bytes)",
                limits.max_file_size_bytes
            ),
            errors: Vec::new(),
        });
    }
    let projected = used_bytes.saturating_add(size_bytes);
    if projected > limits.max_storage_bytes {
        return Err(AppError::UnprocessableEntity {
            message: format!(
                "Tenant storage quota exceeded: used {used_bytes} + requested {size_bytes} > limit {} bytes",
                limits.max_storage_bytes
            ),
            errors: Vec::new(),
        });
    }
    Ok(())
}

/// Computes `TenantQuotaUsage` summary from raw totals and limits.
#[must_use]
pub fn compute_quota_usage(
    used_bytes: i64,
    asset_count: i64,
    limits: TenantQuotaLimits,
) -> TenantQuotaUsage {
    let safe_used = used_bytes.max(0);
    let max_bytes = limits.max_storage_bytes.max(1);
    let available_bytes = (max_bytes - safe_used).max(0);
    let pct = ((safe_used as f64) / (max_bytes as f64)) * 100.0;
    let usage_percent = (pct * 100.0).round() / 100.0;

    TenantQuotaUsage {
        used_bytes: safe_used,
        max_bytes,
        available_bytes,
        max_file_size_bytes: limits.max_file_size_bytes,
        asset_count: asset_count.max(0),
        usage_percent,
    }
}
