use super::routes::{create_router, ApiDoc};
use super::state::AppState;
use crate::application::tests::MockCalendarRepository;
use crate::application::use_cases::{
    CreateCalendarCommand, CreateEventCommand, CreateEventExceptionCommand, UpdateCalendarCommand,
    UpdateEventCommand,
};
use crate::domain::models::{Calendar, Event, EventException, TimeRange};
use crate::domain::repositories::{CalendarRepository, EventRepository};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::tenant::{ApiKeyValidator, AuthenticatedKey, HEADER_API_KEY};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(Default)]
struct PresentationMockEventRepo {
    events: Mutex<Vec<Event>>,
    exceptions: Mutex<Vec<EventException>>,
}

#[async_trait]
impl EventRepository for PresentationMockEventRepo {
    async fn create(&self, _tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut lock = self.events.lock().unwrap();
        // Check idempotency via external_resource_id
        if let Some(ext) = &event.external_resource_id {
            if let Some(existing) = lock
                .iter()
                .find(|e| e.external_resource_id.as_deref() == Some(ext))
            {
                return Ok(existing.clone());
            }
        }
        lock.push(event.clone());
        Ok(event.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, AppError> {
        let lock = self.events.lock().unwrap();
        Ok(lock
            .iter()
            .find(|e| e.tenant_id == tenant_id && e.id == id && !e.is_deleted())
            .cloned())
    }

    async fn find_in_range(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        range: &TimeRange,
    ) -> Result<Vec<Event>, AppError> {
        let lock = self.events.lock().unwrap();
        Ok(lock
            .iter()
            .filter(|e| {
                e.tenant_id == tenant_id
                    && e.calendar_id == calendar_id
                    && !e.is_deleted()
                    && (e.time_range.overlaps(range) || e.is_recurring())
            })
            .cloned()
            .collect())
    }

    async fn find_by_criteria(
        &self,
        tenant_id: Uuid,
        calendar_id: Uuid,
        _criteria: &Criteria,
    ) -> Result<Vec<Event>, AppError> {
        let lock = self.events.lock().unwrap();
        Ok(lock
            .iter()
            .filter(|e| e.tenant_id == tenant_id && e.calendar_id == calendar_id && !e.is_deleted())
            .cloned()
            .collect())
    }

    async fn find_exceptions(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<EventException>, AppError> {
        let lock = self.exceptions.lock().unwrap();
        Ok(lock
            .iter()
            .filter(|ex| ex.tenant_id == tenant_id && ex.event_id == event_id)
            .cloned()
            .collect())
    }

    async fn add_exception(
        &self,
        _tenant_id: Uuid,
        exception: &EventException,
    ) -> Result<EventException, AppError> {
        self.exceptions.lock().unwrap().push(exception.clone());
        Ok(exception.clone())
    }

    async fn update(&self, _tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut lock = self.events.lock().unwrap();
        if let Some(pos) = lock.iter().position(|e| e.id == event.id) {
            lock[pos] = event.clone();
            Ok(event.clone())
        } else {
            Err(AppError::NotFound("Event not found".into()))
        }
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.events.lock().unwrap();
        if let Some(e) = lock
            .iter_mut()
            .find(|e| e.tenant_id == tenant_id && e.id == id)
        {
            e.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

struct PresentationMockApiKeyValidator {
    tenant_id: Uuid,
}

#[async_trait]
impl ApiKeyValidator for PresentationMockApiKeyValidator {
    async fn validate_key_hash(
        &self,
        _key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        Ok(Some(AuthenticatedKey {
            tenant_id: self.tenant_id,
            scopes: vec!["*".into()],
        }))
    }
}

fn create_test_state_with_tenant(tenant_id: Uuid) -> AppState {
    AppState::new(
        Arc::new(MockCalendarRepository::default()),
        Arc::new(PresentationMockEventRepo::default()),
        Arc::new(PresentationMockApiKeyValidator { tenant_id }),
    )
}

fn create_test_state() -> AppState {
    create_test_state_with_tenant(Uuid::new_v4())
}

#[tokio::test]
async fn test_health_check_endpoint() {
    let app = create_router(create_test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_openapi_json_endpoint() {
    let app = create_router(create_test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Events API");
    assert_eq!(openapi.info.version, "0.1.0");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/calendars"));
    assert!(openapi.paths.paths.contains_key("/v1/calendars/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/events"));
    assert!(openapi.paths.paths.contains_key("/v1/events/{id}"));
    assert!(openapi
        .paths
        .paths
        .contains_key("/v1/events/{id}/exceptions"));
    assert!(openapi.paths.paths.contains_key("/v1/events/search"));
    assert!(openapi
        .paths
        .paths
        .contains_key("/v1/calendars/{id}/availability"));
}

#[tokio::test]
async fn test_create_and_get_and_patch_calendar_via_http() {
    let tenant_id = Uuid::new_v4();
    let state = create_test_state_with_tenant(tenant_id);
    let app = create_router(state);

    let cmd = CreateCalendarCommand {
        name: "Meeting Room A".to_string(),
        timezone: Some("UTC".to_string()),
        allow_overlaps: Some(true),
        working_hours: None,
    };
    let payload = serde_json::to_string(&cmd).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/calendars")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, "test-api-key")
        .body(Body::from(payload))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_create_event_with_idempotency_key_via_http() {
    let tenant_id = Uuid::new_v4();
    let state = create_test_state_with_tenant(tenant_id);

    // Pre-create calendar
    let cal = Calendar::new(tenant_id, "Resource 1", "UTC", true, None);
    state.calendar_repo.create(tenant_id, &cal).await.unwrap();

    let app = create_router(state);

    let now = Utc::now();
    let cmd = CreateEventCommand {
        calendar_id: cal.id,
        title: "Team Retrospective".to_string(),
        description: None,
        starts_at: now + Duration::hours(1),
        ends_at: now + Duration::hours(2),
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };
    let payload = serde_json::to_string(&cmd).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, "test-api-key")
        .header("idempotency-key", "req-unique-key-1234")
        .body(Body::from(payload))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_create_event_exception_via_http() {
    let tenant_id = Uuid::new_v4();
    let state = create_test_state_with_tenant(tenant_id);

    let now = Utc::now();
    let recurring_event = Event::new(
        tenant_id,
        Uuid::new_v4(),
        "Weekly All Hands",
        None,
        TimeRange::new(now, now + Duration::hours(1)).unwrap(),
        "UTC",
        Some("FREQ=WEEKLY;COUNT=10".to_string()),
        crate::domain::models::EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    state
        .event_repo
        .create(tenant_id, &recurring_event)
        .await
        .unwrap();

    let app = create_router(state);

    let cmd = CreateEventExceptionCommand {
        original_starts_at: now + Duration::days(7),
        is_cancelled: true,
        rescheduled_starts_at: None,
        rescheduled_ends_at: None,
    };
    let payload = serde_json::to_string(&cmd).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/events/{}/exceptions", recurring_event.id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, "test-api-key")
        .body(Body::from(payload))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}
