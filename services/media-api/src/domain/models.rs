use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Allowed functional contexts for categorizing tenant media assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MediaContext {
    General,
    Branding,
    Catalog,
    Content,
    Avatars,
}

impl MediaContext {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Branding => "branding",
            Self::Catalog => "catalog",
            Self::Content => "content",
            Self::Avatars => "avatars",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "general" => Ok(Self::General),
            "branding" => Ok(Self::Branding),
            "catalog" => Ok(Self::Catalog),
            "content" => Ok(Self::Content),
            "avatars" => Ok(Self::Avatars),
            other => Err(AppError::BadRequest(format!(
                "Invalid media context '{other}'. Allowed: general, branding, catalog, content, avatars"
            ))),
        }
    }
}

/// Lifecycle state of an uploaded media asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MediaStatus {
    Pending,
    Ready,
}

impl MediaStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Ready => "READY",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "PENDING" => Ok(Self::Pending),
            "READY" => Ok(Self::Ready),
            other => Err(AppError::BadRequest(format!(
                "Invalid media status: '{other}'"
            ))),
        }
    }
}

/// Stored media asset entity.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MediaAsset {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub storage_provider: String,
    pub context: MediaContext,
    pub status: MediaStatus,
    pub file_name: String,
    pub storage_key: String,
    pub public_url: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub alt_text: Option<String>,
    pub metadata: serde_json::Value,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Result returned by `POST /v1/media/presign`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignedUpload {
    pub asset_id: Uuid,
    pub storage_provider: String,
    pub context: MediaContext,
    pub storage_key: String,
    pub upload_url: String,
    pub public_url: String,
    pub expires_in_secs: u64,
}

/// Tenant quota limits (from `tenant_quotas` or env defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantQuotaLimits {
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
}

/// Aggregated tenant storage usage returned by `GET /v1/media/quota`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantQuotaUsage {
    pub used_bytes: i64,
    pub max_bytes: i64,
    pub available_bytes: i64,
    pub max_file_size_bytes: i64,
    pub asset_count: i64,
    pub usage_percent: f64,
}
