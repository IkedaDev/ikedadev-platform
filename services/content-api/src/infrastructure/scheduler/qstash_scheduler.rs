use crate::domain::ports::{ScheduledPublishPayload, SchedulerPort};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use ikeda_core::error::AppError;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct QStashConfig {
    pub base_url: String,
    pub token: String,
    pub current_signing_key: String,
    pub next_signing_key: Option<String>,
    pub callback_url: String,
}

#[derive(Debug, Deserialize)]
struct QStashPublishResponse {
    #[serde(rename = "messageId")]
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct UpstashSignatureClaims {
    iss: Option<String>,
    body: Option<String>,
    #[allow(dead_code)]
    exp: Option<i64>,
}

/// Upstash QStash adapter implementing `SchedulerPort` with `Upstash-Signature` verification.
pub struct QStashScheduler {
    client: Client,
    config: QStashConfig,
}

impl QStashScheduler {
    pub fn new(client: Client, config: QStashConfig) -> Self {
        Self { client, config }
    }

    fn verify_with_key(
        signing_key: &str,
        signature_header: &str,
        raw_body: &[u8],
    ) -> Result<(), AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.leeway = 5;

        let token_data = decode::<UpstashSignatureClaims>(
            signature_header.trim(),
            &DecodingKey::from_secret(signing_key.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::Unauthorized("Invalid Upstash-Signature JWT".to_string()))?;

        if let Some(ref iss) = token_data.claims.iss {
            if iss != "Upstash" {
                return Err(AppError::Unauthorized(
                    "Invalid Upstash-Signature issuer".to_string(),
                ));
            }
        }

        if let Some(ref expected_body_hash) = token_data.claims.body {
            let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(raw_body));
            let trimmed_expected = expected_body_hash.trim_end_matches('=');
            if computed != trimmed_expected {
                return Err(AppError::Unauthorized(
                    "Upstash-Signature body digest mismatch".to_string(),
                ));
            }
        }

        Ok(())
    }
}

#[async_trait]
impl SchedulerPort for QStashScheduler {
    async fn schedule_article_publish(
        &self,
        payload: ScheduledPublishPayload,
    ) -> Result<String, AppError> {
        let delay_seconds = (payload.scheduled_for - Utc::now()).num_seconds().max(0);
        let publish_url = format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.callback_url
        );

        let response = self
            .client
            .post(&publish_url)
            .bearer_auth(&self.config.token)
            .header("Upstash-Delay", format!("{delay_seconds}s"))
            .header("Upstash-Retries", "3")
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to schedule publish via QStash: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown QStash error".to_string());
            return Err(AppError::InternalServerError(format!(
                "QStash rejected scheduled publish ({status}): {body}"
            )));
        }

        let res: QStashPublishResponse = response.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse QStash response: {e}"))
        })?;
        Ok(res.message_id)
    }

    fn verify_signature(&self, signature_header: &str, raw_body: &[u8]) -> Result<(), AppError> {
        if signature_header.trim().is_empty() {
            return Err(AppError::Unauthorized(
                "Missing Upstash-Signature header".to_string(),
            ));
        }

        if Self::verify_with_key(&self.config.current_signing_key, signature_header, raw_body)
            .is_ok()
        {
            return Ok(());
        }

        if let Some(ref next_key) = self.config.next_signing_key {
            if !next_key.trim().is_empty()
                && Self::verify_with_key(next_key, signature_header, raw_body).is_ok()
            {
                return Ok(());
            }
        }

        Err(AppError::Unauthorized(
            "Upstash-Signature verification failed".to_string(),
        ))
    }
}

/// No-op / local dev scheduler when `QSTASH_TOKEN` is not configured.
/// Still validates HS256 `Upstash-Signature` using a configured dev key if provided.
#[derive(Debug, Clone)]
pub struct NoopScheduler {
    signing_key: String,
}

impl Default for NoopScheduler {
    fn default() -> Self {
        Self {
            signing_key: "dev-qstash-signing-key".to_string(),
        }
    }
}

impl NoopScheduler {
    pub fn new(signing_key: impl Into<String>) -> Self {
        Self {
            signing_key: signing_key.into(),
        }
    }
}

#[async_trait]
impl SchedulerPort for NoopScheduler {
    async fn schedule_article_publish(
        &self,
        payload: ScheduledPublishPayload,
    ) -> Result<String, AppError> {
        tracing::info!(
            tenant_id = %payload.tenant_id,
            article_id = %payload.article_id,
            scheduled_for = %payload.scheduled_for,
            "NoopScheduler recorded scheduled article publish (QSTASH_TOKEN not set)"
        );
        Ok(format!("noop_msg_{}", Uuid::new_v4()))
    }

    fn verify_signature(&self, signature_header: &str, raw_body: &[u8]) -> Result<(), AppError> {
        QStashScheduler::verify_with_key(&self.signing_key, signature_header, raw_body)
    }
}
