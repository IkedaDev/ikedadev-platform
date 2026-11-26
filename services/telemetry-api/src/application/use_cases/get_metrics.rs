use crate::domain::models::{EventKind, EventMetrics, MetricsQuery};
use crate::domain::repositories::EventRepository;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Command to compute aggregated telemetry metrics over a date range.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetMetricsCommand {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    #[serde(default)]
    pub kind: Option<EventKind>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub top_limit: Option<i64>,
}

/// Use case for retrieving aggregated telemetry metrics (`total_events`, `unique_clients`, `unique_sessions`, `top_events`).
pub struct GetMetricsUseCase {
    repo: Arc<dyn EventRepository>,
}

impl GetMetricsUseCase {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        cmd: GetMetricsCommand,
    ) -> Result<EventMetrics, AppError> {
        let query = MetricsQuery::try_new(cmd.from, cmd.to, cmd.kind, cmd.source, cmd.top_limit)?;
        self.repo.get_metrics(tenant_id, &query).await
    }
}
