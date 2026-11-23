use crate::domain::models::Category;
use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Port defining RLS-scoped persistence operations for `Category` entities.
#[async_trait]
pub trait CategoryRepository: Send + Sync {
    async fn create(&self, tenant_id: Uuid, category: &Category) -> Result<Category, AppError>;

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Category>, AppError>;

    async fn find_by_slug(&self, tenant_id: Uuid, slug: &str)
        -> Result<Option<Category>, AppError>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Category>, AppError>;

    async fn update(&self, tenant_id: Uuid, category: &Category) -> Result<Category, AppError>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;
}
