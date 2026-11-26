use crate::domain::models::TelemetryEvent;
use crate::domain::repositories::EventRepository;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::PaginatedApiResponse;
use std::sync::Arc;
use uuid::Uuid;

/// Use case for executing dynamic paginated `Criteria` searches across telemetry events.
pub struct SearchEventsUseCase {
    repo: Arc<dyn EventRepository>,
}

impl SearchEventsUseCase {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        criteria: Criteria,
    ) -> Result<PaginatedApiResponse<TelemetryEvent>, AppError> {
        let pagination = criteria.pagination.unwrap_or_default();
        let page_num = pagination.page.max(1);
        let limit = pagination.limit.clamp(1, 100);
        let page = self.repo.search_by_criteria(tenant_id, &criteria).await?;
        Ok(PaginatedApiResponse::new(
            page.items,
            page_num,
            limit,
            page.total_items,
        ))
    }
}
