use crate::domain::models::{CatalogItem, ItemDetails, NewCatalogItemParams};
use crate::domain::repositories::{CatalogItemRepository, CategoryRepository};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::PaginatedApiResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Maximum number of item IDs accepted in a single batch lookup request.
pub const MAX_BATCH_IDS: usize = 100;

/// Request payload for creating a new catalog item (`SERVICE`, `PRODUCT`, or `BUNDLE`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCatalogItemCommand {
    pub category_id: Option<Uuid>,
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub base_price: i64,
    pub currency: Option<String>,
    pub tax_rate_bps: Option<i32>,
    pub tax_included: Option<bool>,
    #[serde(flatten)]
    pub details: ItemDetails,
    pub is_active: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

/// Request payload for partially updating a catalog item.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateCatalogItemCommand {
    pub category_id: Option<Uuid>,
    pub sku: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub base_price: Option<i64>,
    pub currency: Option<String>,
    pub tax_rate_bps: Option<i32>,
    pub tax_included: Option<bool>,
    pub details: Option<ItemDetails>,
    pub is_active: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

/// Request payload for batch item lookup (`POST /v1/items/batch`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchCatalogItemsQuery {
    /// List of catalog item UUIDs to fetch (maximum 100).
    pub ids: Vec<Uuid>,
}

pub(crate) fn normalize_optional_str(input: Option<String>) -> Option<String> {
    input.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub struct CreateCatalogItemUseCase {
    category_repo: Arc<dyn CategoryRepository>,
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl CreateCatalogItemUseCase {
    pub fn new(
        category_repo: Arc<dyn CategoryRepository>,
        item_repo: Arc<dyn CatalogItemRepository>,
    ) -> Self {
        Self {
            category_repo,
            item_repo,
        }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: CreateCatalogItemCommand,
    ) -> Result<CatalogItem, AppError> {
        let sku = normalize_optional_str(cmd.sku);

        if let Some(ref s) = sku {
            if self.item_repo.find_by_sku(tenant_id, s).await?.is_some() {
                return Err(AppError::Conflict(format!(
                    "A catalog item with SKU '{s}' already exists in this tenant"
                )));
            }
        }

        if let Some(cat_id) = cmd.category_id {
            if self
                .category_repo
                .find_by_id(tenant_id, cat_id)
                .await?
                .is_none()
            {
                return Err(AppError::NotFound(format!("Category '{cat_id}' not found")));
            }
        }

        let item = CatalogItem::new(NewCatalogItemParams {
            tenant_id,
            category_id: cmd.category_id,
            sku,
            name: cmd.name,
            description: normalize_optional_str(cmd.description),
            base_price: cmd.base_price,
            currency: cmd.currency.unwrap_or_else(|| "CLP".to_string()),
            tax_rate_bps: cmd.tax_rate_bps.unwrap_or(0),
            tax_included: cmd.tax_included.unwrap_or(true),
            details: cmd.details,
            is_active: cmd.is_active.unwrap_or(true),
            metadata: cmd.metadata.unwrap_or_else(|| serde_json::json!({})),
        })?;

        // Verify all bundle component item_ids exist in the same tenant
        if let ItemDetails::Bundle { ref items } = item.details {
            if !items.is_empty() {
                let comp_ids: Vec<Uuid> = items.iter().map(|c| c.item_id).collect();
                let found = self
                    .item_repo
                    .find_by_ids_batch(tenant_id, &comp_ids)
                    .await?;
                let found_ids: HashSet<Uuid> = found.iter().map(|i| i.id).collect();
                for req_id in comp_ids {
                    if !found_ids.contains(&req_id) {
                        return Err(AppError::NotFound(format!(
                            "Bundle component item '{req_id}' not found in tenant"
                        )));
                    }
                }
            }
        }

        self.item_repo.create(tenant_id, &item).await
    }
}

pub struct GetCatalogItemUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl GetCatalogItemUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, item_id: Uuid) -> Result<CatalogItem, AppError> {
        self.item_repo
            .find_by_id(tenant_id, item_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Catalog item with ID '{item_id}' not found"))
            })
    }
}

pub struct GetCatalogItemsBatchUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl GetCatalogItemsBatchUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        ids: Vec<Uuid>,
    ) -> Result<Vec<CatalogItem>, AppError> {
        if ids.len() > MAX_BATCH_IDS {
            return Err(AppError::BadRequest(format!(
                "Batch lookup supports at most {MAX_BATCH_IDS} IDs per request (received {})",
                ids.len()
            )));
        }

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        self.item_repo.find_by_ids_batch(tenant_id, &ids).await
    }
}

pub struct SearchCatalogItemsUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl SearchCatalogItemsUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        criteria: Criteria,
    ) -> Result<PaginatedApiResponse<CatalogItem>, AppError> {
        let pagination = criteria.pagination.unwrap_or_default();
        let page = pagination.page.max(1);
        let limit = pagination.sql_limit();

        let result = self
            .item_repo
            .search_by_criteria(tenant_id, &criteria)
            .await?;

        Ok(PaginatedApiResponse::new(
            result.items,
            page,
            limit,
            result.total_items,
        ))
    }
}

pub struct UpdateCatalogItemUseCase {
    category_repo: Arc<dyn CategoryRepository>,
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl UpdateCatalogItemUseCase {
    pub fn new(
        category_repo: Arc<dyn CategoryRepository>,
        item_repo: Arc<dyn CatalogItemRepository>,
    ) -> Self {
        Self {
            category_repo,
            item_repo,
        }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        cmd: UpdateCatalogItemCommand,
    ) -> Result<CatalogItem, AppError> {
        let mut item = self
            .item_repo
            .find_by_id(tenant_id, item_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Catalog item with ID '{item_id}' not found"))
            })?;

        if let Some(cat_id) = cmd.category_id {
            if self
                .category_repo
                .find_by_id(tenant_id, cat_id)
                .await?
                .is_none()
            {
                return Err(AppError::NotFound(format!("Category '{cat_id}' not found")));
            }
            item.category_id = Some(cat_id);
        }

        if let Some(raw_sku) = cmd.sku {
            let normalized = normalize_optional_str(Some(raw_sku));
            if let Some(ref s) = normalized {
                if let Some(existing) = self.item_repo.find_by_sku(tenant_id, s).await? {
                    if existing.id != item_id {
                        return Err(AppError::Conflict(format!(
                            "A catalog item with SKU '{s}' already exists in this tenant"
                        )));
                    }
                }
            }
            item.sku = normalized;
        }

        if let Some(name) = cmd.name {
            item.name = name.trim().to_string();
        }

        if let Some(description) = cmd.description {
            item.description = normalize_optional_str(Some(description));
        }

        if let Some(base_price) = cmd.base_price {
            item.base_price = base_price;
        }

        if let Some(currency) = cmd.currency {
            item.currency = currency.trim().to_uppercase();
        }

        if let Some(tax_rate_bps) = cmd.tax_rate_bps {
            item.tax_rate_bps = tax_rate_bps;
        }

        if let Some(tax_included) = cmd.tax_included {
            item.tax_included = tax_included;
        }

        if let Some(details) = cmd.details {
            item.details = details;
        }

        if let Some(is_active) = cmd.is_active {
            item.is_active = is_active;
        }

        if let Some(metadata) = cmd.metadata {
            item.metadata = metadata;
        }

        item.validate()?;

        if let ItemDetails::Bundle { ref items } = item.details {
            if !items.is_empty() {
                let comp_ids: Vec<Uuid> = items.iter().map(|c| c.item_id).collect();
                let found = self
                    .item_repo
                    .find_by_ids_batch(tenant_id, &comp_ids)
                    .await?;
                let found_ids: HashSet<Uuid> = found.iter().map(|i| i.id).collect();
                for req_id in comp_ids {
                    if !found_ids.contains(&req_id) {
                        return Err(AppError::NotFound(format!(
                            "Bundle component item '{req_id}' not found in tenant"
                        )));
                    }
                }
            }
        }

        self.item_repo.update(tenant_id, &item).await
    }
}

pub struct DeleteCatalogItemUseCase {
    item_repo: Arc<dyn CatalogItemRepository>,
}

impl DeleteCatalogItemUseCase {
    pub fn new(item_repo: Arc<dyn CatalogItemRepository>) -> Self {
        Self { item_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, item_id: Uuid) -> Result<(), AppError> {
        let deleted = self.item_repo.soft_delete(tenant_id, item_id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Catalog item with ID '{item_id}' not found"
            )));
        }
        Ok(())
    }
}
