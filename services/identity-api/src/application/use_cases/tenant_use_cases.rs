use crate::domain::models::Tenant;
use crate::domain::repositories::TenantRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for provisioning a new platform tenant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTenantCommand {
    /// Unique lowercase URL-safe slug (e.g. `"anami-studio"`).
    pub slug: String,
    /// Organization display name.
    pub name: String,
    /// Optional JSONB configuration object.
    pub config: Option<serde_json::Value>,
}

/// Request payload for updating an existing tenant.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateTenantCommand {
    /// Optional updated display name.
    pub name: Option<String>,
    /// Optional updated status (`ACTIVE`, `SUSPENDED`, `INACTIVE`).
    pub status: Option<String>,
    /// Optional updated JSONB configuration object.
    pub config: Option<serde_json::Value>,
}

/// Use case: creates a new platform tenant.
pub struct CreateTenantUseCase {
    tenant_repo: Arc<dyn TenantRepository>,
}

impl CreateTenantUseCase {
    pub fn new(tenant_repo: Arc<dyn TenantRepository>) -> Self {
        Self { tenant_repo }
    }

    pub async fn execute(&self, cmd: CreateTenantCommand) -> Result<Tenant, AppError> {
        let slug = cmd.slug.trim().to_lowercase();
        if slug.is_empty()
            || !slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(AppError::BadRequest(
                "Tenant 'slug' must be non-empty and contain only lowercase alphanumeric characters or hyphens".to_string(),
            ));
        }

        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Tenant 'name' cannot be empty".to_string(),
            ));
        }

        if self.tenant_repo.find_by_slug(&slug).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "Tenant with slug '{slug}' already exists"
            )));
        }

        let config = cmd.config.unwrap_or_else(|| serde_json::json!({}));
        if !config.is_object() {
            return Err(AppError::BadRequest(
                "Tenant 'config' must be a JSON object".to_string(),
            ));
        }

        let tenant = Tenant::new(slug, name, config);
        self.tenant_repo.create(&tenant).await
    }
}

/// Use case: retrieves an active tenant by ID.
pub struct GetTenantUseCase {
    tenant_repo: Arc<dyn TenantRepository>,
}

impl GetTenantUseCase {
    pub fn new(tenant_repo: Arc<dyn TenantRepository>) -> Self {
        Self { tenant_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid) -> Result<Tenant, AppError> {
        self.tenant_repo
            .find_by_id(tenant_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tenant '{tenant_id}' not found")))
    }
}

/// Use case: updates an existing tenant's display name, status, or config.
pub struct UpdateTenantUseCase {
    tenant_repo: Arc<dyn TenantRepository>,
}

impl UpdateTenantUseCase {
    pub fn new(tenant_repo: Arc<dyn TenantRepository>) -> Self {
        Self { tenant_repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: UpdateTenantCommand,
    ) -> Result<Tenant, AppError> {
        let mut tenant = self
            .tenant_repo
            .find_by_id(tenant_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tenant '{tenant_id}' not found")))?;

        if let Some(name) = cmd.name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "Tenant 'name' cannot be empty".to_string(),
                ));
            }
            tenant.name = trimmed.to_string();
        }

        if let Some(status) = cmd.status {
            let upper = status.trim().to_uppercase();
            if !matches!(upper.as_str(), "ACTIVE" | "SUSPENDED" | "INACTIVE") {
                return Err(AppError::BadRequest(format!(
                    "Invalid tenant status '{status}'. Expected ACTIVE, SUSPENDED, or INACTIVE"
                )));
            }
            tenant.status = upper;
        }

        if let Some(config) = cmd.config {
            if !config.is_object() {
                return Err(AppError::BadRequest(
                    "Tenant 'config' must be a JSON object".to_string(),
                ));
            }
            tenant.config = config;
        }

        self.tenant_repo.update(&tenant).await
    }
}
