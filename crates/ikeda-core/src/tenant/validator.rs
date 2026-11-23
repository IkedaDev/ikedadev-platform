use crate::error::AppError;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Resolved authentication details for an authorized API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedKey {
    /// The tenant associated with this API key.
    pub tenant_id: Uuid,
    /// Authorized scopes/permissions.
    pub scopes: Vec<String>,
}

/// Asynchronous port for validating API key hashes against a persistence store.
#[async_trait::async_trait]
pub trait ApiKeyValidator: Send + Sync {
    /// Validates a SHA-256 key hash and returns the authorized tenant and scopes if valid and active.
    async fn validate_key_hash(&self, key_hash: &str)
        -> Result<Option<AuthenticatedKey>, AppError>;
}

/// Computes the lowercase hex-encoded SHA-256 digest of a raw API key string.
#[must_use]
pub fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}
