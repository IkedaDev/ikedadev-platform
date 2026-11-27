use crate::application::preview_token::PreviewTokenService;
use crate::domain::ports::{
    ArticleRepository, MediaRepository, ObjectStoragePort, SchedulerPort, TaxonomyRepository,
    WebhookRepository,
};
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::path::PathBuf;
use std::sync::Arc;

/// Shared application state for `content-api`.
#[derive(Clone)]
pub struct AppState {
    pub article_repo: Arc<dyn ArticleRepository>,
    pub taxonomy_repo: Arc<dyn TaxonomyRepository>,
    pub media_repo: Arc<dyn MediaRepository>,
    pub webhook_repo: Arc<dyn WebhookRepository>,
    pub object_storage: Arc<dyn ObjectStoragePort>,
    pub scheduler: Arc<dyn SchedulerPort>,
    pub preview_tokens: PreviewTokenService,
    pub http_client: reqwest::Client,
    pub allow_http_urls: bool,
    /// Optional local directory for dev-only media storage (None when R2/S3 is configured).
    pub local_media_dir: Option<PathBuf>,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        article_repo: Arc<dyn ArticleRepository>,
        taxonomy_repo: Arc<dyn TaxonomyRepository>,
        media_repo: Arc<dyn MediaRepository>,
        webhook_repo: Arc<dyn WebhookRepository>,
        object_storage: Arc<dyn ObjectStoragePort>,
        scheduler: Arc<dyn SchedulerPort>,
        preview_tokens: PreviewTokenService,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            article_repo,
            taxonomy_repo,
            media_repo,
            webhook_repo,
            object_storage,
            scheduler,
            preview_tokens,
            http_client: reqwest::Client::new(),
            allow_http_urls: false,
            local_media_dir: None,
            api_key_validator,
            jwt_validator: None,
        }
    }

    #[must_use]
    pub fn with_local_media_dir(mut self, dir: PathBuf) -> Self {
        self.local_media_dir = Some(dir);
        self
    }

    #[must_use]
    pub fn with_jwt_validator(mut self, jwt_validator: Arc<dyn JwtValidator>) -> Self {
        self.jwt_validator = Some(jwt_validator);
        self
    }

    #[must_use]
    pub fn with_allow_http_urls(mut self, allow: bool) -> Self {
        self.allow_http_urls = allow;
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
