use super::handlers::{
    get_client_timeline, get_metrics_summary, health_check, ingest_batch_events, ingest_event,
    search_events,
};
use super::openapi::ApiDoc;
use super::state::AppState;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Builds and configures the Telemetry presentation router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/events", post(ingest_event))
        .route(
            "/v1/events/batch",
            post(ingest_batch_events).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route("/v1/events/search", post(search_events))
        .route("/v1/clients/{client_id}/timeline", get(get_client_timeline))
        .route("/v1/metrics/summary", post(get_metrics_summary))
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
