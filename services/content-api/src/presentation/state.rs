use crate::application::preview_token::PreviewTokenService;
use crate::domain::ports::{
    ArticleRepository, SchedulerPort, TaxonomyRepository, WebhookRepository,
};
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state for `content-api`.
#[derive(Clone)]
pub struct AppState {
    pub article_repo: Arc<dyn ArticleRepository>,
    pub taxonomy_repo: Arc<dyn TaxonomyRepository>,
    pub webhook_repo: Arc<dyn WebhookRepository>,
    pub scheduler: Arc<dyn SchedulerPort>,
    pub preview_tokens: PreviewTokenService,
    pub http_client: reqwest::Client,
    pub allow_http_urls: bool,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    pub fn new(
        article_repo: Arc<dyn ArticleRepository>,
        taxonomy_repo: Arc<dyn TaxonomyRepository>,
        webhook_repo: Arc<dyn WebhookRepository>,
        scheduler: Arc<dyn SchedulerPort>,
        preview_tokens: PreviewTokenService,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            article_repo,
            taxonomy_repo,
            webhook_repo,
            scheduler,
            preview_tokens,
            http_client: reqwest::Client::new(),
            allow_http_urls: false,
            api_key_validator,
            jwt_validator: None,
        }
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
