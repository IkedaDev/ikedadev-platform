use crate::domain::models::{validate_slug, Category, NewCategoryParams};
use crate::domain::repositories::CategoryRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for creating a catalog category.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCategoryCommand {
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    pub is_active: Option<bool>,
}

/// Request payload for partially updating a catalog category.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateCategoryCommand {
    pub parent_id: Option<Uuid>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub display_order: Option<i32>,
    pub is_active: Option<bool>,
}

pub struct CreateCategoryUseCase {
    category_repo: Arc<dyn CategoryRepository>,
}

impl CreateCategoryUseCase {
    pub fn new(category_repo: Arc<dyn CategoryRepository>) -> Self {
        Self { category_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: CreateCategoryCommand,
    ) -> Result<Category, AppError> {
        if let Some(parent_id) = cmd.parent_id {
            if self
                .category_repo
                .find_by_id(tenant_id, parent_id)
                .await?
                .is_none()
            {
                return Err(AppError::NotFound(format!(
                    "Parent category '{parent_id}' not found"
                )));
            }
        }

        let category = Category::new(NewCategoryParams {
            tenant_id,
            parent_id: cmd.parent_id,
            name: cmd.name,
            slug: cmd.slug,
            description: cmd.description,
            display_order: cmd.display_order,
            is_active: cmd.is_active.unwrap_or(true),
        })?;

        if self
            .category_repo
            .find_by_slug(tenant_id, &category.slug)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "Category with slug '{}' already exists in this tenant",
                category.slug
            )));
        }

        self.category_repo.create(tenant_id, &category).await
    }
}

pub struct ListCategoriesUseCase {
    category_repo: Arc<dyn CategoryRepository>,
}

impl ListCategoriesUseCase {
    pub fn new(category_repo: Arc<dyn CategoryRepository>) -> Self {
        Self { category_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid) -> Result<Vec<Category>, AppError> {
        self.category_repo.list(tenant_id).await
    }
}

pub struct UpdateCategoryUseCase {
    category_repo: Arc<dyn CategoryRepository>,
}

impl UpdateCategoryUseCase {
    pub fn new(category_repo: Arc<dyn CategoryRepository>) -> Self {
        Self { category_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        category_id: Uuid,
        cmd: UpdateCategoryCommand,
    ) -> Result<Category, AppError> {
        let mut category = self
            .category_repo
            .find_by_id(tenant_id, category_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Category '{category_id}' not found")))?;

        if let Some(parent_id) = cmd.parent_id {
            if parent_id == category_id {
                return Err(AppError::BadRequest(
                    "Category cannot be its own parent".to_string(),
                ));
            }
            if self
                .category_repo
                .find_by_id(tenant_id, parent_id)
                .await?
                .is_none()
            {
                return Err(AppError::NotFound(format!(
                    "Parent category '{parent_id}' not found"
                )));
            }
            category.parent_id = Some(parent_id);
        }

        if let Some(name) = cmd.name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "Category 'name' cannot be empty".to_string(),
                ));
            }
            category.name = trimmed.to_string();
        }

        if let Some(slug_raw) = cmd.slug {
            let slug = slug_raw.trim().to_lowercase();
            validate_slug(&slug)?;
            if let Some(existing) = self.category_repo.find_by_slug(tenant_id, &slug).await? {
                if existing.id != category_id {
                    return Err(AppError::Conflict(format!(
                        "Category with slug '{slug}' already exists in this tenant"
                    )));
                }
            }
            category.slug = slug;
        }

        if let Some(description) = cmd.description {
            category.description = Some(description);
        }

        if let Some(display_order) = cmd.display_order {
            category.display_order = display_order;
        }

        if let Some(is_active) = cmd.is_active {
            category.is_active = is_active;
        }

        self.category_repo.update(tenant_id, &category).await
    }
}

pub struct DeleteCategoryUseCase {
    category_repo: Arc<dyn CategoryRepository>,
}

impl DeleteCategoryUseCase {
    pub fn new(category_repo: Arc<dyn CategoryRepository>) -> Self {
        Self { category_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, category_id: Uuid) -> Result<(), AppError> {
        let deleted = self
            .category_repo
            .soft_delete(tenant_id, category_id)
            .await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Category '{category_id}' not found"
            )));
        }
        Ok(())
    }
}
