use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// User belonging to a specific tenant and synchronized with Keycloak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TenantUser {
    /// Internal identity record UUID.
    pub id: Uuid,
    /// Owning tenant identifier (enforced via PostgreSQL RLS).
    pub tenant_id: Uuid,
    /// Corresponding user UUID in Keycloak (`sub` claim in JWTs).
    pub keycloak_user_id: Uuid,
    /// User email address.
    pub email: String,
    /// User full name.
    pub full_name: String,
    /// Assigned roles/scopes.
    pub roles: Vec<String>,
    /// Whether the user account is active.
    pub is_active: bool,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

impl TenantUser {
    /// Instantiates a new active `TenantUser`.
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        keycloak_user_id: Uuid,
        email: String,
        full_name: String,
        roles: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            keycloak_user_id,
            email,
            full_name,
            roles,
            is_active: true,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    /// Returns `true` if soft-deleted.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Marks user as inactive and soft-deleted.
    pub fn mark_deleted(&mut self) {
        let now = Utc::now();
        self.is_active = false;
        self.deleted_at = Some(now);
        self.updated_at = now;
    }
}
