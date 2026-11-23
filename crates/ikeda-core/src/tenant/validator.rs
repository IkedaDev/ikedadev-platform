use crate::error::AppError;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default TTL in seconds for cached API key validations (60 seconds).
pub const DEFAULT_API_KEY_CACHE_TTL_SECS: u64 = 60;

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

/// Generic in-memory TTL caching decorator wrapping any `ApiKeyValidator`.
///
/// Caches only valid `AuthenticatedKey` resolutions for `ttl` (default 60s) to minimize
/// database round-trips to `identity_db` without retaining invalid/random hashes in memory.
pub struct CachedApiKeyValidator<V: ApiKeyValidator> {
    inner: V,
    ttl: Duration,
    cache: RwLock<HashMap<String, (Instant, AuthenticatedKey)>>,
}

impl<V: ApiKeyValidator> CachedApiKeyValidator<V> {
    /// Wraps an `ApiKeyValidator` with the default 60-second TTL cache.
    #[must_use]
    pub fn new(inner: V) -> Self {
        Self::with_ttl(inner, Duration::from_secs(DEFAULT_API_KEY_CACHE_TTL_SECS))
    }

    /// Wraps an `ApiKeyValidator` with a custom TTL duration.
    #[must_use]
    pub fn with_ttl(inner: V, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl<V: ApiKeyValidator> ApiKeyValidator for CachedApiKeyValidator<V> {
    async fn validate_key_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        // 1. Fast path: check read-locked in-memory cache
        {
            let guard = self.cache.read().await;
            if let Some((inserted_at, auth)) = guard.get(key_hash) {
                if inserted_at.elapsed() < self.ttl {
                    return Ok(Some(auth.clone()));
                }
            }
        }

        // 2. Cache miss or expired entry: delegate to underlying validator
        let result = self.inner.validate_key_hash(key_hash).await?;

        // 3. Update cache (store valid keys; evict stale entries if now revoked/invalid)
        let mut guard = self.cache.write().await;
        if let Some(ref auth) = result {
            guard.insert(key_hash.to_string(), (Instant::now(), auth.clone()));
        } else {
            guard.remove(key_hash);
        }

        Ok(result)
    }
}

/// Computes the lowercase hex-encoded SHA-256 digest of a raw API key string.
#[must_use]
pub fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}
