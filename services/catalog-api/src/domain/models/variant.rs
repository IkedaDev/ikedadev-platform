use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Configurable variant of a `CatalogItem` (e.g. "60 min", "90 min", "Size M").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CatalogItemVariant {
    /// Unique variant identifier.
    pub id: Uuid,
    /// Owning tenant identifier.
    pub tenant_id: Uuid,
    /// Parent catalog item identifier.
    pub item_id: Uuid,
    /// Variant display name.
    pub name: String,
    /// Optional SKU unique per tenant among active variants.
    pub sku: Option<String>,
    /// Optional price override in minimal currency units (`>= 0`).
    pub price_override: Option<i64>,
    /// Optional duration override in minutes (`> 0`).
    pub duration_override: Option<i32>,
    /// Whether this variant is active.
    pub is_active: bool,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Parameters for instantiating a `CatalogItemVariant`.
#[derive(Debug, Clone)]
pub struct NewVariantParams {
    pub tenant_id: Uuid,
    pub item_id: Uuid,
    pub name: String,
    pub sku: Option<String>,
    pub price_override: Option<i64>,
    pub duration_override: Option<i32>,
    pub is_active: bool,
}

impl CatalogItemVariant {
    /// Creates and validates a new `CatalogItemVariant`.
    pub fn new(params: NewVariantParams) -> Result<Self, AppError> {
        let name = params.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Variant 'name' cannot be empty".to_string(),
            ));
        }

        if let Some(price) = params.price_override {
            if price < 0 {
                return Err(AppError::BadRequest(
                    "Variant 'price_override' must be >= 0".to_string(),
                ));
            }
        }

        if let Some(duration) = params.duration_override {
            if duration <= 0 {
                return Err(AppError::BadRequest(
                    "Variant 'duration_override' must be > 0".to_string(),
                ));
            }
        }

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id: params.tenant_id,
            item_id: params.item_id,
            name,
            sku: params.sku,
            price_override: params.price_override,
            duration_override: params.duration_override,
            is_active: params.is_active,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        })
    }

    /// Validates all domain invariants on this variant.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.name.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Variant 'name' cannot be empty".to_string(),
            ));
        }
        if let Some(price) = self.price_override {
            if price < 0 {
                return Err(AppError::BadRequest(
                    "Variant 'price_override' must be >= 0".to_string(),
                ));
            }
        }
        if let Some(duration) = self.duration_override {
            if duration <= 0 {
                return Err(AppError::BadRequest(
                    "Variant 'duration_override' must be > 0".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Returns `true` if soft-deleted.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Marks the variant as soft-deleted.
    pub fn mark_deleted(&mut self) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
    }
}
