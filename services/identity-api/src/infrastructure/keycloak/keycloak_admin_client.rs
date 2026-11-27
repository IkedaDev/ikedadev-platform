use crate::domain::services::{CreateIdpUserInput, IdentityProviderPort};
use async_trait::async_trait;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Configuration for authenticating against the Keycloak Admin REST API using `client_credentials`.
#[derive(Debug, Clone)]
pub struct KeycloakAdminConfig {
    /// Base URL of Keycloak (e.g. `http://localhost:8180`).
    pub base_url: String,
    /// Target Realm name (single-realm multi-tenant strategy, e.g. `"ikeda"`).
    pub realm: String,
    /// Confidential service-account client ID with realm-management roles.
    pub client_id: String,
    /// Confidential service-account client secret.
    pub client_secret: String,
}

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeycloakCredential {
    #[serde(rename = "type")]
    cred_type: String,
    value: String,
    temporary: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateKeycloakUserBody {
    username: String,
    email: String,
    first_name: String,
    last_name: String,
    enabled: bool,
    email_verified: bool,
    attributes: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credentials: Option<Vec<KeycloakCredential>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct KeycloakRoleRepresentation {
    id: String,
    name: String,
}

/// Production adapter for the Keycloak Admin REST API using `reqwest`.
pub struct KeycloakAdminClient {
    config: KeycloakAdminConfig,
    http: reqwest::Client,
    token_cache: Arc<RwLock<Option<CachedToken>>>,
}

impl KeycloakAdminClient {
    /// Creates a new `KeycloakAdminClient` with a 15-second HTTP timeout.
    pub fn new(config: KeycloakAdminConfig) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to initialize Keycloak HTTP client: {e}"
                ))
            })?;

        Ok(Self {
            config,
            http,
            token_cache: Arc::new(RwLock::new(None)),
        })
    }

    async fn get_service_token(&self) -> Result<String, AppError> {
        {
            let guard = self.token_cache.read().await;
            if let Some(ref cached) = *guard {
                if Instant::now() < cached.expires_at {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        let token_url = format!(
            "{}/realms/{}/protocol/openid-connect/token",
            self.config.base_url.trim_end_matches('/'),
            self.config.realm
        );

        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
        ];

        let resp = self
            .http
            .post(&token_url)
            .form(&form)
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to contact Keycloak token endpoint: {e}"
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::InternalServerError(format!(
                "Keycloak token request failed ({status}): {body}"
            )));
        }

        let parsed: TokenResponse = resp.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Invalid Keycloak token response: {e}"))
        })?;

        // Refresh 30 seconds early to avoid race with expiry
        let ttl = parsed.expires_in.saturating_sub(30).max(5);
        let access_token = parsed.access_token;

        let mut guard = self.token_cache.write().await;
        *guard = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at: Instant::now() + Duration::from_secs(ttl),
        });

        Ok(access_token)
    }

    async fn assign_realm_roles(
        &self,
        token: &str,
        keycloak_user_id: Uuid,
        roles: &[String],
    ) -> Result<(), AppError> {
        if roles.is_empty() {
            return Ok(());
        }

        let base = self.config.base_url.trim_end_matches('/');
        let realm = &self.config.realm;

        let mut role_reps = Vec::with_capacity(roles.len());
        for role_name in roles {
            let encoded_role = percent_encode_path_segment(role_name.trim());
            let role_url = format!("{base}/admin/realms/{realm}/roles/{encoded_role}");
            let resp = self
                .http
                .get(&role_url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| {
                    AppError::InternalServerError(format!(
                        "Failed to lookup Keycloak role '{role_name}': {e}"
                    ))
                })?;

            if !resp.status().is_success() {
                return Err(AppError::BadRequest(format!(
                    "Role '{role_name}' does not exist in Keycloak realm '{realm}'"
                )));
            }

            let rep: KeycloakRoleRepresentation = resp.json().await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to parse Keycloak role: {e}"))
            })?;
            role_reps.push(rep);
        }

        let assign_url =
            format!("{base}/admin/realms/{realm}/users/{keycloak_user_id}/role-mappings/realm");
        let resp = self
            .http
            .post(&assign_url)
            .bearer_auth(token)
            .json(&role_reps)
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to assign Keycloak roles: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::InternalServerError(format!(
                "Keycloak role assignment failed ({status}): {body}"
            )));
        }

        Ok(())
    }
}

fn percent_encode_path_segment(input: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn split_full_name(full_name: &str) -> (String, String) {
    let mut parts = full_name.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("").trim().to_string();
    let last = parts.next().unwrap_or("").trim().to_string();
    (first, last)
}

#[async_trait]
impl IdentityProviderPort for KeycloakAdminClient {
    async fn create_user(&self, input: CreateIdpUserInput) -> Result<Uuid, AppError> {
        let token = self.get_service_token().await?;
        let base = self.config.base_url.trim_end_matches('/');
        let realm = &self.config.realm;

        let (first_name, last_name) = split_full_name(&input.full_name);

        let mut attributes = HashMap::new();
        attributes.insert("tenant_id".to_string(), vec![input.tenant_id.to_string()]);

        let credentials = input.temporary_password.map(|pwd| {
            vec![KeycloakCredential {
                cred_type: "password".to_string(),
                value: pwd,
                temporary: true,
            }]
        });

        let payload = CreateKeycloakUserBody {
            username: input.email.clone(),
            email: input.email,
            first_name,
            last_name,
            enabled: true,
            email_verified: false,
            attributes,
            credentials,
        };

        let users_url = format!("{base}/admin/realms/{realm}/users");
        let resp = self
            .http
            .post(&users_url)
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to create user in Keycloak: {e}"))
            })?;

        if resp.status() == reqwest::StatusCode::CONFLICT {
            return Err(AppError::Conflict(
                "A user with this email already exists in Keycloak".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::InternalServerError(format!(
                "Keycloak user creation failed ({status}): {body}"
            )));
        }

        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AppError::InternalServerError(
                    "Keycloak response missing Location header with created user ID".to_string(),
                )
            })?;

        let raw_id = location.rsplit('/').next().ok_or_else(|| {
            AppError::InternalServerError(format!(
                "Unable to parse user ID from Keycloak Location header: '{location}'"
            ))
        })?;

        let keycloak_user_id = Uuid::parse_str(raw_id).map_err(|_| {
            AppError::InternalServerError(format!(
                "Keycloak returned non-UUID user identifier: '{raw_id}'"
            ))
        })?;

        if let Err(err) = self
            .assign_realm_roles(&token, keycloak_user_id, &input.roles)
            .await
        {
            if let Err(rollback_err) = self.disable_user(keycloak_user_id).await {
                tracing::error!(
                    keycloak_user_id = %keycloak_user_id,
                    error = %rollback_err,
                    "Compensating Keycloak disable failed after role assignment error"
                );
            }
            return Err(err);
        }

        if input.send_invite_email {
            let actions = vec!["UPDATE_PASSWORD".to_string(), "VERIFY_EMAIL".to_string()];
            if let Err(err) = self.trigger_action_email(keycloak_user_id, &actions).await {
                if let Err(rollback_err) = self.disable_user(keycloak_user_id).await {
                    tracing::error!(
                        keycloak_user_id = %keycloak_user_id,
                        error = %rollback_err,
                        "Compensating Keycloak disable failed after action email error"
                    );
                }
                return Err(err);
            }
        }

        Ok(keycloak_user_id)
    }

    async fn disable_user(&self, keycloak_user_id: Uuid) -> Result<(), AppError> {
        let token = self.get_service_token().await?;
        let url = format!(
            "{}/admin/realms/{}/users/{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.realm,
            keycloak_user_id
        );

        let resp = self
            .http
            .put(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "enabled": false }))
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to disable user in Keycloak: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::InternalServerError(format!(
                "Keycloak disable user failed ({status}): {body}"
            )));
        }

        Ok(())
    }

    async fn trigger_action_email(
        &self,
        keycloak_user_id: Uuid,
        actions: &[String],
    ) -> Result<(), AppError> {
        let token = self.get_service_token().await?;
        let url = format!(
            "{}/admin/realms/{}/users/{}/execute-actions-email",
            self.config.base_url.trim_end_matches('/'),
            self.config.realm,
            keycloak_user_id
        );

        let resp = self
            .http
            .put(&url)
            .bearer_auth(&token)
            .json(actions)
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to trigger Keycloak action email: {e}"
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::InternalServerError(format!(
                "Keycloak execute-actions-email failed ({status}): {body}"
            )));
        }

        Ok(())
    }
}

/// In-memory mock implementation of `IdentityProviderPort` for unit/integration tests
/// and local development without a running Keycloak instance.
#[derive(Default)]
pub struct MockIdentityProvider {
    pub created_users: Mutex<Vec<(Uuid, CreateIdpUserInput)>>,
    pub disabled_users: Mutex<Vec<Uuid>>,
    pub triggered_emails: Mutex<Vec<(Uuid, Vec<String>)>>,
}

#[async_trait]
impl IdentityProviderPort for MockIdentityProvider {
    async fn create_user(&self, input: CreateIdpUserInput) -> Result<Uuid, AppError> {
        let kc_id = Uuid::new_v4();
        let mut guard = self
            .created_users
            .lock()
            .map_err(|e| AppError::InternalServerError(format!("Mock IdP lock poisoned: {e}")))?;
        guard.push((kc_id, input));
        Ok(kc_id)
    }

    async fn disable_user(&self, keycloak_user_id: Uuid) -> Result<(), AppError> {
        let mut guard = self
            .disabled_users
            .lock()
            .map_err(|e| AppError::InternalServerError(format!("Mock IdP lock poisoned: {e}")))?;
        guard.push(keycloak_user_id);
        Ok(())
    }

    async fn trigger_action_email(
        &self,
        keycloak_user_id: Uuid,
        actions: &[String],
    ) -> Result<(), AppError> {
        let mut guard = self
            .triggered_emails
            .lock()
            .map_err(|e| AppError::InternalServerError(format!("Mock IdP lock poisoned: {e}")))?;
        guard.push((keycloak_user_id, actions.to_vec()));
        Ok(())
    }
}
