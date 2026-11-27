use crate::domain::models::TenantQuotaLimits;
use crate::domain::ports::{MediaRepository, ObjectStoragePort};
use crate::domain::services::{DEFAULT_MAX_FILE_SIZE_BYTES, DEFAULT_MAX_STORAGE_BYTES};
use crate::infrastructure::LocalDevObjectStorage;
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub media_repo: Arc<dyn MediaRepository>,
    pub object_storage: Arc<dyn ObjectStoragePort>,
    pub default_quotas: TenantQuotaLimits,
    /// Present ONLY when R2 is unconfigured AND `ALLOW_LOCAL_MEDIA_STORAGE=true`.
    pub local_storage: Option<Arc<LocalDevObjectStorage>>,
    pub local_cors_origin: String,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    pub fn new(
        media_repo: Arc<dyn MediaRepository>,
        object_storage: Arc<dyn ObjectStoragePort>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            media_repo,
            object_storage,
            default_quotas: TenantQuotaLimits {
                max_storage_bytes: DEFAULT_MAX_STORAGE_BYTES,
                max_file_size_bytes: DEFAULT_MAX_FILE_SIZE_BYTES,
            },
            local_storage: None,
            local_cors_origin: "http://localhost:5173".to_string(),
            api_key_validator,
            jwt_validator: None,
        }
    }

    #[must_use]
    pub fn with_default_quotas(mut self, quotas: TenantQuotaLimits) -> Self {
        self.default_quotas = quotas;
        self
    }

    #[must_use]
    pub fn with_local_storage(
        mut self,
        local_storage: Arc<LocalDevObjectStorage>,
        cors_origin: impl Into<String>,
    ) -> Self {
        self.local_storage = Some(local_storage);
        self.local_cors_origin = cors_origin.into();
        self
    }

    #[must_use]
    pub fn with_jwt_validator(mut self, jwt_validator: Arc<dyn JwtValidator>) -> Self {
        self.jwt_validator = Some(jwt_validator);
        self
    }
}

impl FromRef<AppState> for Arc<dyn ApiKeyValidator> {
    fn from_ref(state: &AppState) -> Self {
        state.api_key_validator.clone()
    }
}

impl FromRef<AppState> for Option<Arc<dyn JwtValidator>> {
    fn from_ref(state: &AppState) -> Self {
        state.jwt_validator.clone()
    }
}
