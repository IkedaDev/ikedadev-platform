use super::create_client::normalize_optional_str;
use crate::domain::models::Client;
use crate::domain::repositories::ClientRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request payload for partially updating an existing client.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateClientCommand {
    /// Optional updated full name.
    pub full_name: Option<String>,
    /// Optional updated email (unique per tenant among active clients).
    pub email: Option<String>,
    /// Optional updated phone number.
    pub phone: Option<String>,
    /// Optional updated tax ID (unique per tenant among active clients).
    pub tax_id: Option<String>,
    /// Optional updated address.
    pub address: Option<String>,
    /// Optional updated notes.
    pub notes: Option<String>,
    /// Optional updated linked Keycloak user ID.
    pub keycloak_user_id: Option<Uuid>,
    /// Optional updated active flag.
    pub is_active: Option<bool>,
    /// Optional updated JSONB metadata object.
    pub metadata: Option<serde_json::Value>,
}

/// Application use case: partially updates a client while preserving per-tenant uniqueness.
pub struct UpdateClientUseCase {
    client_repo: Arc<dyn ClientRepository>,
}

impl UpdateClientUseCase {
    /// Constructs a new `UpdateClientUseCase`.
    pub fn new(client_repo: Arc<dyn ClientRepository>) -> Self {
        Self { client_repo }
    }

    /// Executes partial client update.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        client_id: Uuid,
        cmd: UpdateClientCommand,
    ) -> Result<Client, AppError> {
        let mut client = self
            .client_repo
            .find_by_id(tenant_id, client_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Client with ID '{client_id}' not found")))?;

        if let Some(name) = cmd.full_name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(AppError::BadRequest(
                    "Client 'full_name' cannot be empty".to_string(),
                ));
            }
            client.full_name = trimmed.to_string();
        }

        if let Some(raw_email) = cmd.email {
            let normalized = normalize_optional_str(Some(raw_email)).map(|s| s.to_lowercase());
            if let Some(ref email) = normalized {
                if let Some(existing) = self.client_repo.find_by_email(tenant_id, email).await? {
                    if existing.id != client_id {
                        return Err(AppError::Conflict(format!(
                            "A client with email '{email}' already exists in this tenant"
                        )));
                    }
                }
            }
            client.email = normalized;
        }

        if let Some(raw_tax_id) = cmd.tax_id {
            let normalized = normalize_optional_str(Some(raw_tax_id));
            if let Some(ref tax_id) = normalized {
                if let Some(existing) = self.client_repo.find_by_tax_id(tenant_id, tax_id).await? {
                    if existing.id != client_id {
                        return Err(AppError::Conflict(format!(
                            "A client with tax_id '{tax_id}' already exists in this tenant"
                        )));
                    }
                }
            }
            client.tax_id = normalized;
        }

        if let Some(phone) = cmd.phone {
            client.phone = normalize_optional_str(Some(phone));
        }

        if let Some(address) = cmd.address {
            client.address = normalize_optional_str(Some(address));
        }

        if let Some(notes) = cmd.notes {
            client.notes = normalize_optional_str(Some(notes));
        }

        if let Some(keycloak_user_id) = cmd.keycloak_user_id {
            client.keycloak_user_id = Some(keycloak_user_id);
        }

        if let Some(is_active) = cmd.is_active {
            client.is_active = is_active;
        }

        if let Some(metadata) = cmd.metadata {
            if !metadata.is_object() {
                return Err(AppError::BadRequest(
                    "Client 'metadata' must be a JSON object".to_string(),
                ));
            }
            client.metadata = metadata;
        }

        self.client_repo.update(tenant_id, &client).await
    }
}
