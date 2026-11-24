use crate::domain::repositories::BookingRepository;
use axum::extract::FromRef;
use ikeda_core::tenant::{ApiKeyValidator, JwtValidator};
use std::sync::Arc;

/// Shared application state for `bookings-api`.
#[derive(Clone)]
pub struct AppState {
    pub booking_repo: Arc<dyn BookingRepository>,
    pub api_key_validator: Arc<dyn ApiKeyValidator>,
    pub jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl AppState {
    pub fn new(
        booking_repo: Arc<dyn BookingRepository>,
        api_key_validator: Arc<dyn ApiKeyValidator>,
    ) -> Self {
        Self {
            booking_repo,
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
