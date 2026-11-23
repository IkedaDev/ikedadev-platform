use crate::domain::repositories::{CatalogItemRepository, CategoryRepository};
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state for `catalog-api`.
#[derive(Clone)]
pub struct AppState {
    pub category_repo: Arc<dyn CategoryRepository>,
    pub item_repo: Arc<dyn CatalogItemRepository>,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    pub fn new(
        category_repo: Arc<dyn CategoryRepository>,
        item_repo: Arc<dyn CatalogItemRepository>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            category_repo,
            item_repo,
            api_key_validator,
            jwt_validator: None,
        }
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
