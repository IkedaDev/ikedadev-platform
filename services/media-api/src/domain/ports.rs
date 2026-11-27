use crate::domain::models::{MediaAsset, MediaContext, TenantQuotaLimits};
use async_trait::async_trait;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreatePendingMediaData {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub storage_provider: String,
    pub context: MediaContext,
    pub file_name: String,
    pub storage_key: String,
    pub public_url: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub uploaded_by: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct ConfirmMediaData {
    pub alt_text: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct UpdateMediaData {
    pub alt_text: Option<Option<String>>,
    pub context: Option<MediaContext>,
    pub metadata: Option<serde_json::Value>,
}

/// Output from `ObjectStoragePort::generate_presigned_put`.
#[derive(Debug, Clone)]
pub struct GeneratedPresignedPut {
    pub upload_url: String,
    pub public_url: String,
    pub expires_in_secs: u64,
}

/// Provider-agnostic Object Storage port (Cloudflare R2 default, BYOS-ready for AWS S3 / GCS / MinIO).
#[async_trait]
pub trait ObjectStoragePort: Send + Sync {
    /// Canonical provider name persisted in `media_assets.storage_provider` (e.g., `R2_PLATFORM`).
    fn provider_name(&self) -> &'static str;

    /// Generates a presigned PUT URL for `storage_key`.
    async fn generate_presigned_put(
        &self,
        storage_key: &str,
        mime_type: &str,
        size_bytes: i64,
    ) -> Result<GeneratedPresignedPut, AppError>;

    /// Physically deletes an object from storage (invoked only on permanent deletion).
    async fn delete_object(&self, storage_key: &str) -> Result<(), AppError>;
}

/// Multi-tenant repository for media assets and tenant quotas.
#[async_trait]
pub trait MediaRepository: Send + Sync {
    async fn create_pending(&self, data: CreatePendingMediaData) -> Result<MediaAsset, AppError>;
    async fn confirm(
        &self,
        tenant_id: Uuid,
        asset_id: Uuid,
        data: ConfirmMediaData,
    ) -> Result<MediaAsset, AppError>;
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<MediaAsset>, AppError>;
    async fn find_by_id_including_deleted(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MediaAsset>, AppError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        context: Option<MediaContext>,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<MediaAsset>, u64), AppError>;
    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<(Vec<MediaAsset>, u64), AppError>;
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateMediaData,
    ) -> Result<MediaAsset, AppError>;
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError>;
    async fn restore(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError>;
    async fn hard_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError>;
    async fn get_usage(
        &self,
        tenant_id: Uuid,
        defaults: TenantQuotaLimits,
    ) -> Result<(i64, i64, TenantQuotaLimits), AppError>;
}
