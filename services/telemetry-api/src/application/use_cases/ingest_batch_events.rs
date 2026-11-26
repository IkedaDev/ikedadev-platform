use super::ingest_event::IngestEventCommand;
use crate::domain::models::TelemetryEvent;
use crate::domain::repositories::EventRepository;
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Command to ingest a batch of 1..=100 telemetry events in a single transaction.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchCommand {
    pub events: Vec<IngestEventCommand>,
}

/// Result payload for a batch ingestion operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchResult {
    pub accepted: usize,
}

/// Use case for validating and bulk-inserting a batch of telemetry events.
pub struct IngestBatchEventsUseCase {
    repo: Arc<dyn EventRepository>,
}

impl IngestBatchEventsUseCase {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        fallback_actor_id: Option<Uuid>,
        fallback_request_id: Option<String>,
        cmd: IngestBatchCommand,
    ) -> Result<IngestBatchResult, AppError> {
        let inputs = cmd
            .events
            .into_iter()
            .map(|item| {
                item.into_new_event(tenant_id, fallback_actor_id, fallback_request_id.clone())
            })
            .collect();

        let validated_events = TelemetryEvent::try_new_batch(inputs)?;
        let accepted = self.repo.insert_batch(tenant_id, &validated_events).await?;
        Ok(IngestBatchResult { accepted })
    }
}
