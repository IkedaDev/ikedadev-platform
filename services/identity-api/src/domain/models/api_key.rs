use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Internal persistence model for an API key (contains only SHA-256 hash, never plaintext).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Safe public representation of an API key (exposes prefix and metadata only, never hash or secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiKeySummary {
    /// Unique API key record ID.
    pub id: Uuid,
    /// Owning tenant ID.
    pub tenant_id: Uuid,
    /// Descriptive label for this key.
    pub name: String,
    /// Non-sensitive key prefix for identification (e.g. `"ik_live_a1b2c3d4"`).
    pub key_prefix: String,
    /// Granted permission scopes.
    pub scopes: Vec<String>,
    /// Whether this key is currently active.
    pub is_active: bool,
    /// Optional expiration timestamp in UTC.
    pub expires_at: Option<DateTime<Utc>>,
    /// Last recorded usage timestamp.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Revocation timestamp (`None` if not revoked).
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<ApiKeyRecord> for ApiKeySummary {
    fn from(r: ApiKeyRecord) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            key_prefix: r.key_prefix,
            scopes: r.scopes,
            is_active: r.is_active,
            expires_at: r.expires_at,
            last_used_at: r.last_used_at,
            created_at: r.created_at,
            revoked_at: r.revoked_at,
        }
    }
}

/// Response returned ONLY ONCE upon API key creation, containing the plaintext key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreatedApiKey {
    /// Key metadata summary.
    pub key: ApiKeySummary,
    /// Plaintext API key secret. Store securely — it will never be shown again.
    pub raw_key: String,
}
