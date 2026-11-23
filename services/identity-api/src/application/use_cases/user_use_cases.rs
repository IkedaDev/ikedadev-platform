use crate::domain::models::TenantUser;
use crate::domain::repositories::TenantUserRepository;
use crate::domain::services::{CreateIdpUserInput, IdentityProviderPort};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for provisioning a new tenant user in Keycloak and `tenant_users`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTenantUserCommand {
    /// User email address (unique per tenant).
    pub email: String,
    /// User full name.
    pub full_name: String,
    /// Optional initial temporary password.
    pub temporary_password: Option<String>,
    /// Roles to assign in Keycloak and store locally.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Whether Keycloak should send a password-setup/verification email.
    #[serde(default)]
    pub send_invite_email: bool,
}

/// Use case: provisions a user in Keycloak (with `tenant_id` attribute) and saves them in `tenant_users`.
pub struct CreateTenantUserUseCase {
    user_repo: Arc<dyn TenantUserRepository>,
    idp: Arc<dyn IdentityProviderPort>,
}

impl CreateTenantUserUseCase {
    pub fn new(
        user_repo: Arc<dyn TenantUserRepository>,
        idp: Arc<dyn IdentityProviderPort>,
    ) -> Self {
        Self { user_repo, idp }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: CreateTenantUserCommand,
    ) -> Result<TenantUser, AppError> {
        let email = cmd.email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AppError::BadRequest(
                "User 'email' must be a valid email address".to_string(),
            ));
        }

        let full_name = cmd.full_name.trim().to_string();
        if full_name.is_empty() {
            return Err(AppError::BadRequest(
                "User 'full_name' cannot be empty".to_string(),
            ));
        }

        // 1. Provision in Keycloak with tenant_id attribute + roles
        let keycloak_user_id = self
            .idp
            .create_user(CreateIdpUserInput {
                tenant_id,
                email: email.clone(),
                full_name: full_name.clone(),
                temporary_password: cmd.temporary_password,
                roles: cmd.roles.clone(),
                send_invite_email: cmd.send_invite_email,
            })
            .await?;

        // 2. Persist in tenant_users with PostgreSQL RLS
        let user = TenantUser::new(tenant_id, keycloak_user_id, email, full_name, cmd.roles);
        match self.user_repo.create(tenant_id, &user).await {
            Ok(saved) => Ok(saved),
            Err(err) => {
                // Best-effort compensating rollback in Keycloak if local DB insert fails
                if let Err(idp_err) = self.idp.disable_user(keycloak_user_id).await {
                    tracing::error!(
                        keycloak_user_id = %keycloak_user_id,
                        error = %idp_err,
                        "Compensating Keycloak disable failed after DB error"
                    );
                }
                Err(err)
            }
        }
    }
}

/// Use case: lists all active users for the authenticated tenant.
pub struct ListTenantUsersUseCase {
    user_repo: Arc<dyn TenantUserRepository>,
}

impl ListTenantUsersUseCase {
    pub fn new(user_repo: Arc<dyn TenantUserRepository>) -> Self {
        Self { user_repo }
    }

    pub async fn execute(&self, tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError> {
        self.user_repo.find_by_tenant(tenant_id).await
    }
}

/// Use case: disables a user in Keycloak and performs a soft delete in `tenant_users`.
pub struct DeleteTenantUserUseCase {
    user_repo: Arc<dyn TenantUserRepository>,
    idp: Arc<dyn IdentityProviderPort>,
}

impl DeleteTenantUserUseCase {
    pub fn new(
        user_repo: Arc<dyn TenantUserRepository>,
        idp: Arc<dyn IdentityProviderPort>,
    ) -> Self {
        Self { user_repo, idp }
    }

    pub async fn execute(&self, tenant_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let user = self
            .user_repo
            .find_by_id(tenant_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User with ID '{user_id}' not found")))?;

        // 1. Disable account in Keycloak first
        self.idp.disable_user(user.keycloak_user_id).await?;

        // 2. Soft delete in tenant_users
        let deleted = self.user_repo.soft_delete(tenant_id, user_id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "User with ID '{user_id}' not found"
            )));
        }

        Ok(())
    }
}
