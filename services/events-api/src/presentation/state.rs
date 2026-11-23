use crate::domain::repositories::{CalendarRepository, EventRepository};
use axum::extract::FromRef;
use ikeda_core::tenant::ApiKeyValidator;
use std::sync::Arc;

/// Shared application state containing injectable ports and services.
#[derive(Clone)]
pub struct AppState {
    /// Injected calendar repository port.
    pub calendar_repo: Arc<dyn CalendarRepository>,
    /// Injected event repository port.
    pub event_repo: Arc<dyn EventRepository>,
    /// Injected API key validator port for authentication.
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
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
        }
    }
}

impl FromRef<AppState> for Arc<dyn ApiKeyValidator> {
    fn from_ref(state: &AppState) -> Self {
        state.api_key_validator.clone()
    }
}
