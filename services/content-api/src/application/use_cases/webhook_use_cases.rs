use crate::application::rbac::require_content_publish;
use crate::domain::models::{PublishWebhook, WebhookEvent};
use crate::domain::ports::{CreateWebhookData, UpdateWebhookData, WebhookRepository};
use crate::domain::services::{validate_webhook_events, validate_webhook_target_url};
use chrono::Utc;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateWebhookCommand {
    pub name: String,
    pub target_url: String,
    #[serde(default)]
    pub events: Option<Vec<String>>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateWebhookCommand {
    pub name: Option<String>,
    pub target_url: Option<String>,
    pub events: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookTestResult {
    pub webhook_id: Uuid,
    pub status_code: i16,
    pub ok: bool,
}

pub async fn create_webhook(
    repo: &dyn WebhookRepository,
    ctx: &TenantContext,
    cmd: CreateWebhookCommand,
) -> Result<PublishWebhook, AppError> {
    require_content_publish(ctx)?;

    let name = cmd.name.trim().to_string();
    if name.is_empty() || name.len() > 150 {
        return Err(AppError::BadRequest(
            "Webhook name must be between 1 and 150 characters".to_string(),
        ));
    }

    let target_url = validate_webhook_target_url(&cmd.target_url)?;
    let events = match cmd.events {
        Some(ev) => validate_webhook_events(&ev)?,
        None => vec![
            "article.published".to_string(),
            "article.updated".to_string(),
            "article.unpublished".to_string(),
        ],
    };

    repo.create(CreateWebhookData {
        tenant_id: ctx.tenant_id,
        name,
        target_url,
        events,
        is_active: cmd.is_active.unwrap_or(true),
    })
    .await
}

pub async fn list_webhooks(
    repo: &dyn WebhookRepository,
    ctx: &TenantContext,
) -> Result<Vec<PublishWebhook>, AppError> {
    require_content_publish(ctx)?;
    repo.list(ctx.tenant_id).await
}

pub async fn update_webhook(
    repo: &dyn WebhookRepository,
    ctx: &TenantContext,
    id: Uuid,
    cmd: UpdateWebhookCommand,
) -> Result<PublishWebhook, AppError> {
    require_content_publish(ctx)?;

    let name = match cmd.name {
        Some(raw) => {
            let n = raw.trim().to_string();
            if n.is_empty() || n.len() > 150 {
                return Err(AppError::BadRequest(
                    "Webhook name must be between 1 and 150 characters".to_string(),
                ));
            }
            Some(n)
        }
        None => None,
    };

    let target_url = match cmd.target_url {
        Some(raw) => Some(validate_webhook_target_url(&raw)?),
        None => None,
    };

    let events = match cmd.events {
        Some(ev) => Some(validate_webhook_events(&ev)?),
        None => None,
    };

    repo.update(
        ctx.tenant_id,
        id,
        UpdateWebhookData {
            name,
            target_url,
            events,
            is_active: cmd.is_active,
        },
    )
    .await
}

pub async fn delete_webhook(
    repo: &dyn WebhookRepository,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<(), AppError> {
    require_content_publish(ctx)?;
    repo.soft_delete(ctx.tenant_id, id).await
}

/// Sends a test POST request to a configured Deploy Hook and records the status code.
pub async fn test_webhook(
    repo: &dyn WebhookRepository,
    http_client: &reqwest::Client,
    ctx: &TenantContext,
    id: Uuid,
) -> Result<WebhookTestResult, AppError> {
    require_content_publish(ctx)?;

    let webhook = repo
        .find_by_id(ctx.tenant_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Webhook '{id}' not found")))?;

    let safe_url = validate_webhook_target_url(&webhook.target_url)?;
    let payload = serde_json::json!({
        "event": "webhook.test",
        "tenant_id": ctx.tenant_id,
        "webhook_id": webhook.id,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let status_code: i16 = match http_client
        .post(&safe_url)
        .timeout(Duration::from_secs(5))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) => i16::try_from(res.status().as_u16()).unwrap_or(500),
        Err(err) => {
            tracing::warn!(webhook_id = %webhook.id, error = %err, "Deploy hook test delivery failed");
            0
        }
    };

    repo.record_delivery(ctx.tenant_id, webhook.id, status_code)
        .await?;

    Ok(WebhookTestResult {
        webhook_id: webhook.id,
        status_code,
        ok: (200..300).contains(&status_code),
    })
}

/// Fires all active tenant Deploy Hooks subscribed to `event` asynchronously (fire-and-forget).
pub fn dispatch_deploy_hooks(
    webhook_repo: Arc<dyn WebhookRepository>,
    http_client: reqwest::Client,
    tenant_id: Uuid,
    event: WebhookEvent,
    article_id: Uuid,
    slug: String,
) {
    tokio::spawn(async move {
        let event_name = event.as_str();
        let hooks = match webhook_repo
            .find_active_for_event(tenant_id, event_name)
            .await
        {
            Ok(h) => h,
            Err(err) => {
                tracing::warn!(
                    tenant_id = %tenant_id,
                    event = event_name,
                    error = %err,
                    "Failed to load publish webhooks for dispatch"
                );
                return;
            }
        };

        for hook in hooks {
            let safe_url = match validate_webhook_target_url(&hook.target_url) {
                Ok(u) => u,
                Err(err) => {
                    tracing::warn!(webhook_id = %hook.id, error = %err, "Skipping blocked webhook URL");
                    continue;
                }
            };

            let payload = serde_json::json!({
                "event": event_name,
                "tenant_id": tenant_id,
                "article_id": article_id,
                "slug": slug,
                "timestamp": Utc::now().to_rfc3339(),
            });

            let status_code: i16 = match http_client
                .post(&safe_url)
                .timeout(Duration::from_secs(5))
                .json(&payload)
                .send()
                .await
            {
                Ok(res) => i16::try_from(res.status().as_u16()).unwrap_or(500),
                Err(err) => {
                    tracing::warn!(
                        webhook_id = %hook.id,
                        event = event_name,
                        error = %err,
                        "Deploy hook delivery failed"
                    );
                    0
                }
            };

            if let Err(err) = webhook_repo
                .record_delivery(tenant_id, hook.id, status_code)
                .await
            {
                tracing::warn!(
                    webhook_id = %hook.id,
                    error = %err,
                    "Failed to record webhook delivery status"
                );
            }
        }
    });
}
