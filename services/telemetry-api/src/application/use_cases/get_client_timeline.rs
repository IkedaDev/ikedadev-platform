use crate::domain::models::{ClientTimelineQuery, EventKind, TelemetryEvent};
use crate::domain::repositories::EventRepository;
use ikeda_core::error::AppError;
use ikeda_core::response::PaginatedApiResponse;
use std::sync::Arc;
use uuid::Uuid;

/// Use case for retrieving a paginated chronological timeline of events for a specific `client_id`.
pub struct GetClientTimelineUseCase {
    repo: Arc<dyn EventRepository>,
}

impl GetClientTimelineUseCase {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        client_id: Uuid,
        kind: Option<EventKind>,
        page: Option<u64>,
        limit: Option<u64>,
    ) -> Result<PaginatedApiResponse<TelemetryEvent>, AppError> {
        let query = ClientTimelineQuery::new(client_id, kind, page, limit);
        let result = self.repo.get_client_timeline(tenant_id, &query).await?;
        Ok(PaginatedApiResponse::new(
            result.items,
            query.page,
            query.limit,
            result.total_items,
        ))
    }
}
