use crate::application::rbac::{require_content_read, require_content_write};
use crate::domain::models::{ContentCategory, ContentTag};
use crate::domain::ports::{
    CreateCategoryData, CreateTagData, TaxonomyRepository, UpdateCategoryData,
};
use crate::domain::services::normalize_slug;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCategoryCommand {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateCategoryCommand {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Option<String>>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTagCommand {
    pub name: String,
    pub slug: Option<String>,
}

pub async fn create_category(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
    cmd: CreateCategoryCommand,
) -> Result<ContentCategory, AppError> {
    require_content_write(ctx)?;

    let name = cmd.name.trim().to_string();
    if name.is_empty() || name.len() > 150 {
        return Err(AppError::BadRequest(
            "Category name must be between 1 and 150 characters".to_string(),
        ));
    }

    let slug_src = cmd
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&name);
    let slug = normalize_slug(slug_src, 150);
    if slug.is_empty() {
        return Err(AppError::BadRequest("Invalid category slug".to_string()));
    }

    let description = cmd
        .description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());

    repo.create_category(CreateCategoryData {
        tenant_id: ctx.tenant_id,
        name,
        slug,
        description,
        display_order: cmd.display_order,
    })
    .await
}

pub async fn list_categories(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
) -> Result<Vec<ContentCategory>, AppError> {
    require_content_read(ctx)?;
    repo.list_categories(ctx.tenant_id).await
}

pub async fn update_category(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateCategoryCommand,
) -> Result<ContentCategory, AppError> {
    require_content_write(ctx)?;

    let name = match cmd.name {
        Some(raw) => {
            let n = raw.trim().to_string();
            if n.is_empty() || n.len() > 150 {
                return Err(AppError::BadRequest(
                    "Category name must be between 1 and 150 characters".to_string(),
                ));
            }
            Some(n)
        }
        None => None,
    };

    let slug = match cmd.slug {
        Some(raw) => {
            let s = normalize_slug(&raw, 150);
            if s.is_empty() {
                return Err(AppError::BadRequest("Invalid category slug".to_string()));
            }
            Some(s)
        }
        None => None,
    };

    let description = cmd
        .description
        .map(|opt| opt.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()));

    repo.update_category(
        ctx.tenant_id,
        id,
        UpdateCategoryData {
            name,
            slug,
            description,
            display_order: cmd.display_order,
        },
    )
    .await
}

pub async fn delete_category(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<(), AppError> {
    require_content_write(ctx)?;
    repo.soft_delete_category(ctx.tenant_id, id).await
}

pub async fn create_tag(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
    cmd: CreateTagCommand,
) -> Result<ContentTag, AppError> {
    require_content_write(ctx)?;

    let name = cmd.name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest(
            "Tag name must be between 1 and 100 characters".to_string(),
        ));
    }

    let slug_src = cmd
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&name);
    let slug = normalize_slug(slug_src, 100);
    if slug.is_empty() {
        return Err(AppError::BadRequest("Invalid tag slug".to_string()));
    }

    repo.create_tag(CreateTagData {
        tenant_id: ctx.tenant_id,
        name,
        slug,
    })
    .await
}

pub async fn list_tags(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
) -> Result<Vec<ContentTag>, AppError> {
    require_content_read(ctx)?;
    repo.list_tags(ctx.tenant_id).await
}

pub async fn delete_tag(
    repo: &dyn TaxonomyRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<(), AppError> {
    require_content_write(ctx)?;
    repo.soft_delete_tag(ctx.tenant_id, id).await
}
