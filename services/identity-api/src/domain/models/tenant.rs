use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Platform tenant entity representing a business customer of `ikedadev-platform`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Tenant {
    /// Unique tenant identifier (propagated as `tenant_id` across all microservices).
    pub id: Uuid,
    /// Unique URL-safe slug (e.g. `"anami-studio"`).
    pub slug: String,
    /// Display name of the tenant organization.
    pub name: String,
    /// Lifecycle status (e.g. `"ACTIVE"`, `"SUSPENDED"`).
    pub status: String,
    /// Flexible tenant-level JSONB configuration (branding, enabled modules, quotas).
    pub config: serde_json::Value,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Tenant {
    /// Instantiates a new active `Tenant`.
    #[must_use]
    pub fn new(slug: String, name: String, config: serde_json::Value) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            slug,
            name,
            status: "ACTIVE".to_string(),
            config,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}
