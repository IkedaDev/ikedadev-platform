use super::variant::CatalogItemVariant;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

/// Maximum character length allowed for an image URL.
pub const MAX_IMAGE_URL_LEN: usize = 2048;

/// Maximum number of secondary images allowed in `gallery_urls`.
pub const MAX_GALLERY_URLS: usize = 8;

/// Validates an external/media image URL.
///
/// Allows `https://` always, and `http://` only when `allow_http` is `true` (local development).
/// Rejects `javascript:`, `data:`, protocol-relative (`//...`), control characters, missing hosts,
/// and URLs exceeding `MAX_IMAGE_URL_LEN` characters.
pub fn validate_image_url(
    raw: &str,
    field_name: &str,
    allow_http: bool,
) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!(
            "Catalog item '{field_name}' cannot be empty"
        )));
    }
    if trimmed.len() > MAX_IMAGE_URL_LEN {
        return Err(AppError::BadRequest(format!(
            "Catalog item '{field_name}' exceeds maximum length of {MAX_IMAGE_URL_LEN} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!(
            "Catalog item '{field_name}' contains invalid control characters"
        )));
    }
    if trimmed.starts_with("//") {
        return Err(AppError::BadRequest(format!(
            "Catalog item '{field_name}' must be an absolute URL (protocol-relative URLs are not allowed)"
        )));
    }

    let parsed = Url::parse(trimmed).map_err(|_| {
        AppError::BadRequest(format!("Catalog item '{field_name}' is not a valid URL"))
    })?;

    match parsed.scheme() {
        "https" => {}
        "http" if allow_http => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "Catalog item '{field_name}' scheme '{other}' is not allowed; use https://"
            )));
        }
    }

    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(AppError::BadRequest(format!(
            "Catalog item '{field_name}' must include a valid host"
        )));
    }

    Ok(trimmed.to_string())
}

/// Validates and normalizes `gallery_urls` (maximum 8 unique valid image URLs).
pub fn validate_gallery_urls(urls: &[String], allow_http: bool) -> Result<Vec<String>, AppError> {
    if urls.len() > MAX_GALLERY_URLS {
        return Err(AppError::BadRequest(format!(
            "Catalog item 'gallery_urls' supports at most {MAX_GALLERY_URLS} images (received {})",
            urls.len()
        )));
    }

    let mut seen = HashSet::with_capacity(urls.len());
    let mut normalized = Vec::with_capacity(urls.len());

    for (idx, raw) in urls.iter().enumerate() {
        let valid = validate_image_url(raw, &format!("gallery_urls[{idx}]"), allow_http)?;
        if !seen.insert(valid.clone()) {
            return Err(AppError::BadRequest(format!(
                "Catalog item 'gallery_urls' contains duplicate URL '{valid}'"
            )));
        }
        normalized.push(valid);
    }

    Ok(normalized)
}

/// Reference to a catalog item included inside a `Bundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BundleComponent {
    /// Target catalog item ID included in this bundle.
    pub item_id: Uuid,
    /// Number of units of the component included (`> 0`).
    pub quantity: i32,
}

/// Polymorphic type-specific attributes for a `CatalogItem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "item_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItemDetails {
    /// Bookable service with mandatory positive duration and optional buffer.
    Service {
        /// Service execution duration in minutes (`> 0`).
        duration_minutes: i32,
        /// Preparation/cleanup buffer in minutes (`>= 0`).
        #[serde(default)]
        buffer_minutes: i32,
    },
    /// Physical or digital product.
    Product {
        /// Optional unit of measure (e.g. `UNIT`, `ML`, `KG`).
        unit_of_measure: Option<String>,
        /// Whether inventory tracking is enabled.
        #[serde(default)]
        track_inventory: bool,
    },
    /// Package combining multiple catalog items.
    Bundle {
        /// Included items and their quantities.
        #[serde(default)]
        items: Vec<BundleComponent>,
    },
}

impl ItemDetails {
    /// Returns the database discriminator string (`SERVICE`, `PRODUCT`, `BUNDLE`).
    #[must_use]
    pub const fn item_type_str(&self) -> &'static str {
        match self {
            Self::Service { .. } => "SERVICE",
            Self::Product { .. } => "PRODUCT",
            Self::Bundle { .. } => "BUNDLE",
        }
    }
}

/// Unified commercial catalog item (Service, Product, or Bundle) scoped to a single tenant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CatalogItem {
    /// Unique catalog item identifier.
    pub id: Uuid,
    /// Owning tenant identifier.
    pub tenant_id: Uuid,
    /// Optional category ID.
    pub category_id: Option<Uuid>,
    /// Optional SKU unique per tenant among active items.
    pub sku: Option<String>,
    /// Display name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Optional primary/cover image URL (`https://`, or `http://` in local dev).
    pub image_url: Option<String>,
    /// Optional secondary gallery image URLs (maximum 8, unique).
    #[serde(default)]
    pub gallery_urls: Vec<String>,
    /// Base price in minimal currency units (e.g. CLP or cents; `>= 0`).
    pub base_price: i64,
    /// ISO 4217 3-letter uppercase currency code (e.g. `CLP`, `USD`).
    pub currency: String,
    /// Tax rate in basis points (`0..=10000`, e.g. `1900` = 19%).
    pub tax_rate_bps: i32,
    /// Whether `base_price` already includes taxes.
    pub tax_included: bool,
    /// Type-specific polymorphic fields (`item_type` discriminator flattened into JSON).
    #[serde(flatten)]
    pub details: ItemDetails,
    /// Whether this item is active and sellable.
    pub is_active: bool,
    /// Flexible tenant-specific JSONB metadata.
    pub metadata: serde_json::Value,
    /// Associated active variants (populated on detail/batch queries).
    #[serde(default)]
    pub variants: Vec<CatalogItemVariant>,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Parameters for instantiating a new `CatalogItem`.
#[derive(Debug, Clone)]
pub struct NewCatalogItemParams {
    pub tenant_id: Uuid,
    pub category_id: Option<Uuid>,
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub gallery_urls: Vec<String>,
    pub base_price: i64,
    pub currency: String,
    pub tax_rate_bps: i32,
    pub tax_included: bool,
    pub details: ItemDetails,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

impl CatalogItem {
    /// Constructs and validates a new active `CatalogItem` (strict `https://` URL policy).
    pub fn new(params: NewCatalogItemParams) -> Result<Self, AppError> {
        Self::new_with_options(params, false)
    }

    /// Constructs and validates a new active `CatalogItem` with configurable HTTP URL policy.
    pub fn new_with_options(
        params: NewCatalogItemParams,
        allow_http_urls: bool,
    ) -> Result<Self, AppError> {
        let now = Utc::now();
        let image_url = params.image_url.and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let gallery_urls = validate_gallery_urls(&params.gallery_urls, allow_http_urls)?;

        let item = Self {
            id: Uuid::new_v4(),
            tenant_id: params.tenant_id,
            category_id: params.category_id,
            sku: params.sku,
            name: params.name.trim().to_string(),
            description: params.description,
            image_url,
            gallery_urls,
            base_price: params.base_price,
            currency: params.currency.trim().to_uppercase(),
            tax_rate_bps: params.tax_rate_bps,
            tax_included: params.tax_included,
            details: params.details,
            is_active: params.is_active,
            metadata: params.metadata,
            variants: Vec::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        item.validate_with_options(allow_http_urls)?;
        Ok(item)
    }

    /// Validates all domain invariants for this catalog item (strict `https://` URL policy).
    pub fn validate(&self) -> Result<(), AppError> {
        self.validate_with_options(false)
    }

    /// Validates all domain invariants for this catalog item with configurable HTTP URL policy.
    pub fn validate_with_options(&self, allow_http_urls: bool) -> Result<(), AppError> {
        if self.name.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Catalog item 'name' cannot be empty".to_string(),
            ));
        }

        if let Some(ref url) = self.image_url {
            validate_image_url(url, "image_url", allow_http_urls)?;
        }

        validate_gallery_urls(&self.gallery_urls, allow_http_urls)?;

        if self.base_price < 0 {
            return Err(AppError::BadRequest(
                "Catalog item 'base_price' must be >= 0".to_string(),
            ));
        }

        if self.currency.len() != 3 || !self.currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(AppError::BadRequest(format!(
                "Catalog item 'currency' must be a 3-letter uppercase ISO 4217 code (got '{}')",
                self.currency
            )));
        }

        if !(0..=10_000).contains(&self.tax_rate_bps) {
            return Err(AppError::BadRequest(
                "Catalog item 'tax_rate_bps' must be between 0 and 10000".to_string(),
            ));
        }

        if !self.metadata.is_object() {
            return Err(AppError::BadRequest(
                "Catalog item 'metadata' must be a JSON object".to_string(),
            ));
        }

        match &self.details {
            ItemDetails::Service {
                duration_minutes,
                buffer_minutes,
            } => {
                if *duration_minutes <= 0 {
                    return Err(AppError::BadRequest(
                        "Service 'duration_minutes' must be > 0".to_string(),
                    ));
                }
                if *buffer_minutes < 0 {
                    return Err(AppError::BadRequest(
                        "Service 'buffer_minutes' must be >= 0".to_string(),
                    ));
                }
            }
            ItemDetails::Product { .. } => {}
            ItemDetails::Bundle { items } => {
                let mut seen = HashSet::new();
                for comp in items {
                    if comp.item_id == self.id {
                        return Err(AppError::BadRequest(
                            "A bundle cannot contain itself as a component".to_string(),
                        ));
                    }
                    if comp.quantity <= 0 {
                        return Err(AppError::BadRequest(
                            "Bundle component 'quantity' must be > 0".to_string(),
                        ));
                    }
                    if !seen.insert(comp.item_id) {
                        return Err(AppError::BadRequest(format!(
                            "Duplicate component item_id '{}' in bundle",
                            comp.item_id
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns `true` if soft-deleted.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Marks the item as soft-deleted.
    pub fn mark_deleted(&mut self) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
    }
}
