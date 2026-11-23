use super::item_use_cases::normalize_optional_str;
use crate::domain::models::{CatalogItemVariant, NewVariantParams};
use crate::domain::repositories::CatalogItemRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for adding a variant to a catalog item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateVariantCommand {
    pub name: String,
    pub sku: Option<String>,
    pub price_override: Option<i64>,
    pub duration_override: Option<i32>,
    pub is_active: Option<bool>,
}

/// Request payload for partially updating an existing variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateVariantCommand {
    pub name: Option<String>,
    pub sku: Option<String>,
    pub price_override: Option<i64>,
    pub duration_override: Option<i32>,
    pub is_active: Option<bool>,
}

pub struct CreateVariantUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl CreateVariantUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        cmd: CreateVariantCommand,
    ) -> Result<CatalogItemVariant, AppError> {
        if self
            .item_repo
            .find_by_id(tenant_id, item_id)
            .await?
            .is_none()
        {
            return Err(AppError::NotFound(format!(
                "Catalog item '{item_id}' not found"
            )));
        }

        let variant = CatalogItemVariant::new(NewVariantParams {
            tenant_id,
            item_id,
            name: cmd.name,
            sku: normalize_optional_str(cmd.sku),
            price_override: cmd.price_override,
            duration_override: cmd.duration_override,
            is_active: cmd.is_active.unwrap_or(true),
        })?;

        self.item_repo.add_variant(tenant_id, &variant).await
    }
}

pub struct UpdateVariantUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl UpdateVariantUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
        cmd: UpdateVariantCommand,
    ) -> Result<CatalogItemVariant, AppError> {
        let mut variant = self
            .item_repo
            .find_variant_by_id(tenant_id, item_id, variant_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Variant '{variant_id}' not found on item '{item_id}'"
                ))
            })?;

        if let Some(name) = cmd.name {
            variant.name = name.trim().to_string();
        }
        if let Some(sku) = cmd.sku {
            variant.sku = normalize_optional_str(Some(sku));
        }
        if let Some(price_override) = cmd.price_override {
            variant.price_override = Some(price_override);
        }
        if let Some(duration_override) = cmd.duration_override {
            variant.duration_override = Some(duration_override);
        }
        if let Some(is_active) = cmd.is_active {
            variant.is_active = is_active;
        }

        variant.validate()?;
        self.item_repo.update_variant(tenant_id, &variant).await
    }
}

pub struct DeleteVariantUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl DeleteVariantUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<(), AppError> {
        let deleted = self
            .item_repo
            .delete_variant(tenant_id, item_id, variant_id)
            .await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Variant '{variant_id}' not found on item '{item_id}'"
            )));
        }
        Ok(())
    }
}
