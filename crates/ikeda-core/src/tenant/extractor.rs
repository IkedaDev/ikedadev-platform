use super::context::{AuthSource, TenantContext};
use super::validator::{hash_api_key, ApiKeyValidator};
use crate::error::AppError;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use std::sync::Arc;

/// Header name constants for multi-tenant extraction.
pub const HEADER_TENANT_ID: &str = "x-tenant-id";
pub const HEADER_USER_ID: &str = "x-user-id";
pub const HEADER_SCOPES: &str = "x-scopes";
pub const HEADER_API_KEY: &str = "x-api-key";

impl<S> FromRequestParts<S> for TenantContext
where
    S: Send + Sync,
    Arc<dyn ApiKeyValidator>: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Strict API Key evaluation via ApiKeyValidator port
        if let Some(api_key_header) = parts.headers.get(HEADER_API_KEY) {
            let raw_key = api_key_header.to_str().map_err(|_| {
                AppError::BadRequest("Header 'X-Api-Key' contains invalid characters".to_string())
            })?;

            let key_hash = hash_api_key(raw_key.trim());
            let validator = Arc::<dyn ApiKeyValidator>::from_ref(state);

            let auth = validator.validate_key_hash(&key_hash).await?;
            if let Some(auth_key) = auth {
                return Ok(Self {
                    tenant_id: auth_key.tenant_id,
                    user_id: None,
                    scopes: auth_key.scopes,
                    auth_source: AuthSource::ApiKey,
                });
            }

            return Err(AppError::Unauthorized(
                "Invalid or expired API key".to_string(),
            ));
        }

        // 2. Authorization Bearer header (reserved for JWT/JWKS in future phases)
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) {
            let auth_str = auth_header.to_str().map_err(|_| {
                AppError::Unauthorized(
                    "Authorization header contains invalid characters".to_string(),
                )
            })?;

            if let Some(_token) = auth_str.strip_prefix("Bearer ") {
                return Err(AppError::Unauthorized(
                    "Bearer token authentication via JWT/JWKS is not yet configured".to_string(),
                ));
            }

            return Err(AppError::Unauthorized(
                "Invalid Authorization scheme: expected 'Bearer <token>'".to_string(),
            ));
        }

        // 3. Optional Development / Local testing fallback headers (dev-auth feature only)
        #[cfg(feature = "dev-auth")]
        {
            if let Some(tenant_header) = parts.headers.get(HEADER_TENANT_ID) {
                let tenant_str = tenant_header.to_str().map_err(|_| {
                    AppError::BadRequest(
                        "Header 'X-Tenant-Id' contains invalid characters".to_string(),
                    )
                })?;

                let tenant_id = uuid::Uuid::parse_str(tenant_str.trim()).map_err(|_| {
                    AppError::BadRequest(format!(
                        "Header 'X-Tenant-Id' contains an invalid UUID: '{tenant_str}'"
                    ))
                })?;

                let user_id = if let Some(user_header) = parts.headers.get(HEADER_USER_ID) {
                    let user_str = user_header.to_str().map_err(|_| {
                        AppError::BadRequest(
                            "Header 'X-User-Id' contains invalid characters".to_string(),
                        )
                    })?;
                    Some(uuid::Uuid::parse_str(user_str.trim()).map_err(|_| {
                        AppError::BadRequest(format!(
                            "Header 'X-User-Id' contains an invalid UUID: '{user_str}'"
                        ))
                    })?)
                } else {
                    None
                };

                let scopes = if let Some(scopes_header) = parts.headers.get(HEADER_SCOPES) {
                    let scopes_str = scopes_header.to_str().map_err(|_| {
                        AppError::BadRequest(
                            "Header 'X-Scopes' contains invalid characters".to_string(),
                        )
                    })?;
                    scopes_str
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect()
                } else {
                    Vec::new()
                };

                return Ok(Self {
                    tenant_id,
                    user_id,
                    scopes,
                    auth_source: AuthSource::DevHeaders,
                });
            }
        }

        // 4. Missing or invalid credentials
        Err(AppError::Unauthorized(
            "Missing or invalid authentication credentials".to_string(),
        ))
    }
}
