use crate::domain::models::{CatalogItem, CatalogItemVariant};
use async_trait::async_trait;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Paginated search result returned by `CatalogItemRepository::search_by_criteria`.
#[derive(Debug, Clone)]
pub struct CatalogItemSearchPage {
    pub items: Vec<CatalogItem>,
    pub total_items: u64,
}

/// Port defining RLS-scoped persistence operations for `CatalogItem`, `CatalogItemVariant`,
/// and `bundle_items`.
#[async_trait]
pub trait CatalogItemRepository: Send + Sync {
    async fn create(&self, tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError>;

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<CatalogItem>, AppError>;

    async fn find_by_sku(
        &self,
        tenant_id: Uuid,
        sku: &str,
    ) -> Result<Option<CatalogItem>, AppError>;

    async fn find_by_ids_batch(
        &self,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> Result<Vec<CatalogItem>, AppError>;

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<CatalogItemSearchPage, AppError>;

    async fn update(&self, tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;

    async fn add_variant(
        &self,
        tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError>;

    async fn find_variant_by_id(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<Option<CatalogItemVariant>, AppError>;

    async fn update_variant(
        &self,
        tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError>;

    async fn delete_variant(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<bool, AppError>;
}
