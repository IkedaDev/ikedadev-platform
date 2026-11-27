use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Stored media asset metadata in the tenant's media library.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MediaAsset {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub storage_provider: String,
    pub file_name: String,
    pub storage_key: String,
    pub public_url: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub alt_text: Option<String>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Result of generating a presigned upload URL from `ObjectStoragePort`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignedUpload {
    pub asset_id: Uuid,
    pub storage_provider: String,
    pub storage_key: String,
    pub upload_url: String,
    pub public_url: String,
    pub expires_in_seconds: u64,
}
