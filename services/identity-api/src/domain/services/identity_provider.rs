use async_trait::async_trait;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Input payload for provisioning a user in the external Identity Provider (e.g. Keycloak).
#[derive(Debug, Clone)]
pub struct CreateIdpUserInput {
    /// Owning tenant ID injected as the `tenant_id` user attribute in Keycloak.
    pub tenant_id: Uuid,
    /// User email (also used as username in Keycloak).
    pub email: String,
    /// User full name (split into firstName / lastName for Keycloak).
    pub full_name: String,
    /// Optional initial temporary password.
    pub temporary_password: Option<String>,
    /// Realm roles to assign to the user.
    pub roles: Vec<String>,
    /// Whether to trigger a password-setup / email-verification action email.
    pub send_invite_email: bool,
}

/// Decoupled domain port for interacting with an external Identity Provider (Keycloak Admin REST API).
#[async_trait]
pub trait IdentityProviderPort: Send + Sync {
    /// Provisions a user in the configured Realm with `tenant_id` attribute and assigned roles,
    /// returning the created external user UUID (`keycloak_user_id`).
    async fn create_user(&self, input: CreateIdpUserInput) -> Result<Uuid, AppError>;

    /// Disables a user account in the Identity Provider so they can no longer authenticate.
    async fn disable_user(&self, keycloak_user_id: Uuid) -> Result<(), AppError>;

    /// Triggers required action emails (e.g. `UPDATE_PASSWORD`, `VERIFY_EMAIL`) for a user.
    async fn trigger_action_email(
        &self,
        keycloak_user_id: Uuid,
        actions: &[String],
    ) -> Result<(), AppError>;
}
