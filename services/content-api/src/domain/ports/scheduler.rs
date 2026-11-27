use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payload sent to Upstash QStash for scheduled article publication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledPublishPayload {
    pub tenant_id: Uuid,
    pub article_id: Uuid,
    pub scheduled_for: DateTime<Utc>,
}

/// Port for scheduling delayed editorial publications and verifying incoming scheduler callbacks.
#[async_trait]
pub trait SchedulerPort: Send + Sync {
    /// Schedules a delayed callback to publish `payload.article_id` at `payload.scheduled_for`.
    async fn schedule_article_publish(
        &self,
        payload: ScheduledPublishPayload,
    ) -> Result<String, AppError>;

    /// Verifies the cryptographic signature header (`Upstash-Signature`) against the raw request body.
    fn verify_signature(&self, signature_header: &str, raw_body: &[u8]) -> Result<(), AppError>;
}
