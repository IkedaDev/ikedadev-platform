pub mod context;
pub mod extractor;
pub mod jwt;
pub mod validator;

#[cfg(test)]
mod tests;

pub use context::{AuthSource, TenantContext};
pub use extractor::{HEADER_API_KEY, HEADER_SCOPES, HEADER_TENANT_ID, HEADER_USER_ID};
pub use jwt::{
    AuthenticatedUser, JwksConfig, JwksJwtValidator, JwtValidator, DEFAULT_JWKS_CACHE_TTL_SECS,
    DEFAULT_TENANT_CLAIM_NAME,
};
pub use validator::{hash_api_key, ApiKeyValidator, AuthenticatedKey};
