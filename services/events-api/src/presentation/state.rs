use crate::domain::repositories::{CalendarRepository, EventRepository};
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state containing injectable ports and services.
#[derive(Clone)]
pub struct AppState {
    /// Injected calendar repository port.
    pub calendar_repo: Arc<dyn CalendarRepository>,
    /// Injected event repository port.
    pub event_repo: Arc<dyn EventRepository>,
    /// Injected API key validator port for M2M authentication.
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    /// Optional injected JWT validator port for Keycloak/OIDC Bearer authentication.
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    /// Creates a new AppState instance with injected dependencies.
    pub fn new(
        calendar_repo: Arc<dyn CalendarRepository>,
        event_repo: Arc<dyn EventRepository>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            calendar_repo,
            event_repo,
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
