use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// End-customer entity belonging strictly to a single tenant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Client {
    /// Unique client identifier.
    pub id: Uuid,
    /// Owning tenant identifier (enforced via PostgreSQL RLS).
    pub tenant_id: Uuid,
    /// Full legal or display name of the client.
    pub full_name: String,
    /// Optional contact email (unique per tenant when active).
    pub email: Option<String>,
    /// Optional contact phone number.
    pub phone: Option<String>,
    /// Optional tax identifier (e.g. Chilean RUT, DNI, VAT; unique per tenant when active).
    pub tax_id: Option<String>,
    /// Optional physical or billing address.
    pub address: Option<String>,
    /// Optional internal notes.
    pub notes: Option<String>,
    /// Optional Keycloak user ID if the end-customer has portal access.
    pub keycloak_user_id: Option<Uuid>,
    /// Whether the client record is currently active.
    pub is_active: bool,
    /// Flexible tenant-specific JSONB metadata.
    pub metadata: serde_json::Value,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Parameters for instantiating a new `Client` entity.
#[derive(Debug, Clone)]
pub struct NewClientParams {
    pub tenant_id: Uuid,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub tax_id: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub keycloak_user_id: Option<Uuid>,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

impl Client {
    /// Constructs a new active `Client` entity with generated UUID and UTC timestamps.
    #[must_use]
    pub fn new(params: NewClientParams) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id: params.tenant_id,
            full_name: params.full_name,
            email: params.email,
            phone: params.phone,
            tax_id: params.tax_id,
            address: params.address,
            notes: params.notes,
            keycloak_user_id: params.keycloak_user_id,
            is_active: params.is_active,
            metadata: params.metadata,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    /// Returns `true` if this client has been soft-deleted.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Marks the client as soft-deleted.
    pub fn mark_deleted(&mut self) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
    }
}
