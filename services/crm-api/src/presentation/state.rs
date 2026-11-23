use crate::domain::repositories::ClientRepository;
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state for `crm-api`.
#[derive(Clone)]
pub struct AppState {
    /// Injected client repository port.
    pub client_repo: Arc<dyn ClientRepository>,
    /// Injected API key validator port for M2M authentication.
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    /// Optional JWT/JWKS validator port for Keycloak Bearer authentication.
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    /// Creates a new `AppState` instance.
    pub fn new(
        client_repo: Arc<dyn ClientRepository>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            client_repo,
            api_key_validator,
            jwt_validator: None,
        }
    }

    /// Attaches an optional JWT/JWKS validator.
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
