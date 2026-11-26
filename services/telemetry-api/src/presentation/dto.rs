use crate::domain::models::EventKind;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Health status response payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// Query parameters for `GET /v1/clients/{client_id}/timeline`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
pub struct ClientTimelineQueryParams {
    /// 1-based page number (defaults to 1).
    pub page: Option<u64>,
    /// Maximum items per page (1..=100, defaults to 20).
    pub limit: Option<u64>,
    /// Optional event kind filter (`TRACKING` or `AUDIT`).
    pub kind: Option<EventKind>,
}
