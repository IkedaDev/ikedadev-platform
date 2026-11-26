use crate::domain::models::{EventKind, NewTelemetryEvent, TelemetryEvent};
use crate::domain::repositories::EventRepository;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Command to ingest a single telemetry (`TRACKING` or `AUDIT`) event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestEventCommand {
    pub kind: EventKind,
    pub event_name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub client_id: Option<Uuid>,
    #[serde(default)]
    pub actor_id: Option<Uuid>,
    #[serde(default)]
    pub anonymous_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<Uuid>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

impl IngestEventCommand {
    /// Converts the command into a [`NewTelemetryEvent`] applying fallback `actor_id` and `request_id`.
    #[must_use]
    pub fn into_new_event(
        self,
        tenant_id: Uuid,
        fallback_actor_id: Option<Uuid>,
        fallback_request_id: Option<String>,
    ) -> NewTelemetryEvent {
        NewTelemetryEvent {
            tenant_id,
            kind: self.kind,
            event_name: self.event_name,
            source: self.source,
            client_id: self.client_id,
            actor_id: self.actor_id.or(fallback_actor_id),
            anonymous_id: self.anonymous_id,
            session_id: self.session_id,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            request_id: self.request_id.or(fallback_request_id),
            properties: self.properties,
            context: self.context,
            occurred_at: self.occurred_at,
        }
    }
}

/// Use case for validating and persisting a single telemetry event.
pub struct IngestEventUseCase {
    repo: Arc<dyn EventRepository>,
}

impl IngestEventUseCase {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        fallback_actor_id: Option<Uuid>,
        fallback_request_id: Option<String>,
        cmd: IngestEventCommand,
    ) -> Result<TelemetryEvent, AppError> {
        let event = TelemetryEvent::try_new(cmd.into_new_event(
            tenant_id,
            fallback_actor_id,
            fallback_request_id,
        ))?;
        self.repo.insert(tenant_id, &event).await
    }
}
