use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Hierarchical catalog category scoped to a single tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Category {
    /// Unique category identifier.
    pub id: Uuid,
    /// Owning tenant identifier.
    pub tenant_id: Uuid,
    /// Optional parent category for nested taxonomies.
    pub parent_id: Option<Uuid>,
    /// Display name.
    pub name: String,
    /// URL-safe unique slug within the tenant.
    pub slug: String,
    /// Optional description.
    pub description: Option<String>,
    /// Display sort order (ascending).
    pub display_order: i32,
    /// Whether this category is active.
    pub is_active: bool,
    /// Creation timestamp in UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTime<Utc>,
    /// Soft deletion timestamp (`None` if active).
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Parameters for creating a new `Category`.
#[derive(Debug, Clone)]
pub struct NewCategoryParams {
    pub tenant_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub display_order: i32,
    pub is_active: bool,
}

impl Category {
    /// Creates and validates a new active `Category`.
    pub fn new(params: NewCategoryParams) -> Result<Self, AppError> {
        let name = params.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Category 'name' cannot be empty".to_string(),
            ));
        }

        let slug = params.slug.trim().to_lowercase();
        validate_slug(&slug)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id: params.tenant_id,
            parent_id: params.parent_id,
            name,
            slug,
            description: params.description,
            display_order: params.display_order,
            is_active: params.is_active,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        })
    }

    /// Returns `true` if soft-deleted.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Marks the category as soft-deleted.
    pub fn mark_deleted(&mut self) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
    }
}

/// Validates a lowercase URL-safe slug (`[a-z0-9-]`).
pub fn validate_slug(slug: &str) -> Result<(), AppError> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::BadRequest(
            "Category 'slug' must be non-empty and contain only lowercase alphanumeric characters or hyphens".to_string(),
        ));
    }
    Ok(())
}
