use crate::domain::models::PresignedUpload;
use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Provider-agnostic Object Storage port (Cloudflare R2 default, BYOS-ready for AWS S3 / GCS / MinIO).
#[async_trait]
pub trait ObjectStoragePort: Send + Sync {
    /// Returns the canonical provider identifier stored in `media_assets.storage_provider` (e.g. `R2_PLATFORM`).
    fn provider_name(&self) -> &'static str;

    /// Generates a presigned PUT URL for uploading an image object under `tenants/{tenant_id}/media/{asset_id}.{ext}`.
    async fn generate_presigned_upload(
        &self,
        tenant_id: Uuid,
        asset_id: Uuid,
        extension: &str,
        mime_type: &str,
        size_bytes: i64,
    ) -> Result<PresignedUpload, AppError>;

    /// Builds the public URL for a previously uploaded storage key.
    fn public_url_for_key(&self, storage_key: &str) -> String;
}
