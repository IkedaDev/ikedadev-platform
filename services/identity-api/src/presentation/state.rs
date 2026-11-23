use crate::domain::repositories::{ApiKeyRepository, TenantRepository, TenantUserRepository};
use crate::domain::services::IdentityProviderPort;
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state for `identity-api`.
#[derive(Clone)]
pub struct AppState {
    pub tenant_repo: Arc<dyn TenantRepository>,
    pub user_repo: Arc<dyn TenantUserRepository>,
    pub api_key_repo: Arc<dyn ApiKeyRepository>,
    pub idp: Arc<dyn IdentityProviderPort>,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    /// Creates a new `AppState` with all required ports.
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn TenantUserRepository>,
        api_key_repo: Arc<dyn ApiKeyRepository>,
        idp: Arc<dyn IdentityProviderPort>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            tenant_repo,
            user_repo,
            api_key_repo,
            idp,
            api_key_validator,
            jwt_validator: None,
        }
    }

    /// Attaches an optional JWT/JWKS validator for Bearer token authentication.
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
