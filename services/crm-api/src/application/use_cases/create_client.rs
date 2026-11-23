use crate::domain::models::{Client, NewClientParams};
use crate::domain::repositories::ClientRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for creating a new end-customer client.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateClientCommand {
    /// Full legal or display name of the client.
    pub full_name: String,
    /// Optional contact email (unique per tenant among active clients).
    pub email: Option<String>,
    /// Optional phone number.
    pub phone: Option<String>,
    /// Optional tax identifier (e.g. RUT, VAT; unique per tenant among active clients).
    pub tax_id: Option<String>,
    /// Optional address.
    pub address: Option<String>,
    /// Optional internal notes.
    pub notes: Option<String>,
    /// Optional linked Keycloak user ID.
    pub keycloak_user_id: Option<Uuid>,
    /// Whether the client is active (defaults to `true`).
    pub is_active: Option<bool>,
    /// Optional flexible JSONB metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Application use case: validates and creates a new CRM client for a tenant.
pub struct CreateClientUseCase {
    client_repo: Arc<dyn ClientRepository>,
}

impl CreateClientUseCase {
    /// Constructs a new `CreateClientUseCase`.
    pub fn new(client_repo: Arc<dyn ClientRepository>) -> Self {
        Self { client_repo }
    }

    /// Executes client validation, uniqueness checks, and persistence.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: CreateClientCommand,
    ) -> Result<Client, AppError> {
        let full_name = cmd.full_name.trim().to_string();
        if full_name.is_empty() {
            return Err(AppError::BadRequest(
                "Client 'full_name' cannot be empty".to_string(),
            ));
        }

        let email = normalize_optional_str(cmd.email).map(|s| s.to_lowercase());
        let tax_id = normalize_optional_str(cmd.tax_id);
        let phone = normalize_optional_str(cmd.phone);
        let address = normalize_optional_str(cmd.address);
        let notes = normalize_optional_str(cmd.notes);

        if let Some(ref e) = email {
            if self
                .client_repo
                .find_by_email(tenant_id, e)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict(format!(
                    "A client with email '{e}' already exists in this tenant"
                )));
            }
        }

        if let Some(ref t) = tax_id {
            if self
                .client_repo
                .find_by_tax_id(tenant_id, t)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict(format!(
                    "A client with tax_id '{t}' already exists in this tenant"
                )));
            }
        }

        let metadata = cmd.metadata.unwrap_or_else(|| serde_json::json!({}));
        if !metadata.is_object() {
            return Err(AppError::BadRequest(
                "Client 'metadata' must be a JSON object".to_string(),
            ));
        }

        let client = Client::new(NewClientParams {
            tenant_id,
            full_name,
            email,
            phone,
            tax_id,
            address,
            notes,
            keycloak_user_id: cmd.keycloak_user_id,
            is_active: cmd.is_active.unwrap_or(true),
            metadata,
        });

        self.client_repo.create(tenant_id, &client).await
    }
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
