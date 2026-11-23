use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, TimeZone, Utc};
use events_api::application::use_cases::{
    CreateCalendarCommand, CreateEventCommand, CreateEventExceptionCommand, UpdateCalendarCommand,
    UpdateEventCommand,
};
use events_api::domain::models::*;
use events_api::domain::repositories::{CalendarRepository, EventRepository};
use events_api::domain::services::scheduler::{NotificationScheduler, WebhookEventPayload};
use events_api::infrastructure::{MockNotificationScheduler, SqlxApiKeyValidator};
use events_api::presentation::{create_router, AppState};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::tenant::{hash_api_key, ApiKeyValidator, AuthenticatedKey, HEADER_API_KEY};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Default)]
struct SharedMockCalendarRepo {
    calendars: Mutex<Vec<Calendar>>,
}

#[async_trait::async_trait]
impl CalendarRepository for SharedMockCalendarRepo {
    async fn create(&self, _tenant_id: Uuid, calendar: &Calendar) -> Result<Calendar, AppError> {
        self.calendars.lock().unwrap().push(calendar.clone());
        Ok(calendar.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Calendar>, AppError> {
        let lock = self.calendars.lock().unwrap();
        Ok(lock
            .iter()
            .find(|c| c.tenant_id == tenant_id && c.id == id && !c.is_deleted())
            .cloned())
    }

    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<Calendar>, AppError> {
        let lock = self.calendars.lock().unwrap();
        Ok(lock
            .iter()
            .filter(|c| c.tenant_id == tenant_id && !c.is_deleted())
            .cloned()
            .collect())
    }

    async fn update(&self, tenant_id: Uuid, calendar: &Calendar) -> Result<Calendar, AppError> {
        let mut lock = self.calendars.lock().unwrap();
        if let Some(pos) = lock
            .iter()
            .position(|c| c.tenant_id == tenant_id && c.id == calendar.id)
        {
            lock[pos] = calendar.clone();
            Ok(calendar.clone())
        } else {
            Err(AppError::NotFound("Calendar not found".into()))
        }
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.calendars.lock().unwrap();
        if let Some(c) = lock
            .iter_mut()
            .find(|c| c.tenant_id == tenant_id && c.id == id)
        {
            c.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Default)]
struct SharedMockEventRepo {
    events: Mutex<Vec<Event>>,
    exceptions: Mutex<Vec<EventException>>,
}

#[async_trait::async_trait]
impl EventRepository for SharedMockEventRepo {
    async fn create(&self, _tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        self.events.lock().unwrap().push(event.clone());
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

    async fn update(&self, tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut lock = self.events.lock().unwrap();
        if let Some(pos) = lock
            .iter()
            .position(|e| e.tenant_id == tenant_id && e.id == event.id)
        {
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

#[derive(Default)]
struct TestInMemoryApiKeyValidator {
    keys: Mutex<HashMap<String, AuthenticatedKey>>,
}

impl TestInMemoryApiKeyValidator {
    fn register_key(&self, raw_key: &str, tenant_id: Uuid, scopes: Vec<String>) {
        let hash = hash_api_key(raw_key);
        self.keys
            .lock()
            .unwrap()
            .insert(hash, AuthenticatedKey { tenant_id, scopes });
    }
}

#[async_trait::async_trait]
impl ApiKeyValidator for TestInMemoryApiKeyValidator {
    async fn validate_key_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        Ok(self.keys.lock().unwrap().get(key_hash).cloned())
    }
}

fn setup_test_app(
    validator: Arc<dyn ApiKeyValidator>,
) -> (
    axum::Router,
    AppState,
    Arc<SharedMockCalendarRepo>,
    Arc<SharedMockEventRepo>,
) {
    let cal_repo = Arc::new(SharedMockCalendarRepo::default());
    let event_repo = Arc::new(SharedMockEventRepo::default());
    let state = AppState::new(cal_repo.clone(), event_repo.clone(), validator);
    let router = create_router(state.clone());
    (router, state, cal_repo, event_repo)
}

#[tokio::test]
async fn test_tenant_isolation_with_real_sqlx_api_keys_table() {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/events_db?sslmode=disable".to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL not reachable at {db_url}. Skipping real DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let raw_key_a = format!("ikeda_live_{}", Uuid::new_v4().simple());
    let raw_key_b = format!("ikeda_live_{}", Uuid::new_v4().simple());
    let hash_a = hash_api_key(&raw_key_a);
    let hash_b = hash_api_key(&raw_key_b);

    // 1. Insert real API Keys into PostgreSQL api_keys table
    sqlx::query(
        r#"
        INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
        VALUES ($1, 'Tenant A Production Key', 'ikeda_live', $2, '{"*"}', true)
        "#,
    )
    .bind(tenant_a)
    .bind(&hash_a)
    .execute(&pool)
    .await
    .expect("Failed to insert Tenant A API key");

    sqlx::query(
        r#"
        INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
        VALUES ($1, 'Tenant B Production Key', 'ikeda_live', $2, '{"*"}', true)
        "#,
    )
    .bind(tenant_b)
    .bind(&hash_b)
    .execute(&pool)
    .await
    .expect("Failed to insert Tenant B API key");

    // 2. Initialize application with real SqlxApiKeyValidator
    let sqlx_validator = Arc::new(SqlxApiKeyValidator::new(pool.clone()));
    let (app, _state, cal_repo, event_repo) = setup_test_app(sqlx_validator);

    // 3. Tenant A creates Calendar A via HTTP with X-Api-Key
    let cal_cmd = CreateCalendarCommand {
        name: "Tenant A Sensitive Calendar".to_string(),
        timezone: Some("UTC".to_string()),
        allow_overlaps: Some(false),
        working_hours: None,
    };

    let req_cal_a = Request::builder()
        .method("POST")
        .uri("/v1/calendars")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, &raw_key_a)
        .body(Body::from(serde_json::to_string(&cal_cmd).unwrap()))
        .unwrap();

    let res_cal_a = app.clone().oneshot(req_cal_a).await.unwrap();
    assert_eq!(res_cal_a.status(), StatusCode::CREATED);
    let cal_bytes = axum::body::to_bytes(res_cal_a.into_body(), usize::MAX)
        .await
        .unwrap();
    let cal_json: serde_json::Value = serde_json::from_slice(&cal_bytes).unwrap();
    let cal_id = Uuid::parse_str(cal_json["data"]["id"].as_str().unwrap()).unwrap();

    // 4. Tenant A creates Event A via HTTP with X-Api-Key
    let now = Utc::now();
    let event_cmd = CreateEventCommand {
        calendar_id: cal_id,
        title: "Top Secret Strategy Review".to_string(),
        description: Some("Confidential".to_string()),
        starts_at: now + Duration::hours(1),
        ends_at: now + Duration::hours(2),
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: Some(serde_json::json!({ "budget": 500000 })),
    };

    let req_event_a = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, &raw_key_a)
        .body(Body::from(serde_json::to_string(&event_cmd).unwrap()))
        .unwrap();

    let res_event_a = app.clone().oneshot(req_event_a).await.unwrap();
    assert_eq!(res_event_a.status(), StatusCode::CREATED);
    let event_bytes = axum::body::to_bytes(res_event_a.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_json: serde_json::Value = serde_json::from_slice(&event_bytes).unwrap();
    let event_id = Uuid::parse_str(event_json["data"]["id"].as_str().unwrap()).unwrap();

    // 5. Tenant B attempts to list calendars with raw_key_b -> returns 0 items
    let req_b_list = Request::builder()
        .method("GET")
        .uri("/v1/calendars")
        .header(HEADER_API_KEY, &raw_key_b)
        .body(Body::empty())
        .unwrap();

    let res_b_list = app.clone().oneshot(req_b_list).await.unwrap();
    assert_eq!(res_b_list.status(), StatusCode::OK);
    let list_bytes = axum::body::to_bytes(res_b_list.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    assert_eq!(list_json["data"].as_array().unwrap().len(), 0);

    // 6. Tenant B attempts to GET Calendar A by ID with raw_key_b -> 404 NOT_FOUND
    let req_b_get_cal = Request::builder()
        .method("GET")
        .uri(format!("/v1/calendars/{cal_id}"))
        .header(HEADER_API_KEY, &raw_key_b)
        .body(Body::empty())
        .unwrap();

    let res_b_get_cal = app.clone().oneshot(req_b_get_cal).await.unwrap();
    assert_eq!(res_b_get_cal.status(), StatusCode::NOT_FOUND);

    // 7. Tenant B attempts to GET Event A by ID with raw_key_b -> 404 NOT_FOUND
    let req_b_get_event = Request::builder()
        .method("GET")
        .uri(format!("/v1/events/{event_id}"))
        .header(HEADER_API_KEY, &raw_key_b)
        .body(Body::empty())
        .unwrap();

    let res_b_get_event = app.clone().oneshot(req_b_get_event).await.unwrap();
    assert_eq!(res_b_get_event.status(), StatusCode::NOT_FOUND);

    // 8. Tenant B attempts to PATCH Event A with raw_key_b -> 404 NOT_FOUND
    let patch_cmd = UpdateEventCommand {
        title: Some("Hacked Event Title".to_string()),
        description: None,
        starts_at: None,
        ends_at: None,
        status: None,
        metadata: None,
    };
    let req_b_patch = Request::builder()
        .method("PATCH")
        .uri(format!("/v1/events/{event_id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, &raw_key_b)
        .body(Body::from(serde_json::to_string(&patch_cmd).unwrap()))
        .unwrap();

    let res_b_patch = app.clone().oneshot(req_b_patch).await.unwrap();
    assert_eq!(res_b_patch.status(), StatusCode::NOT_FOUND);

    // 9. Tenant B attempts to DELETE Event A with raw_key_b -> 404 NOT_FOUND
    let req_b_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/events/{event_id}"))
        .header(HEADER_API_KEY, &raw_key_b)
        .body(Body::empty())
        .unwrap();

    let res_b_delete = app.clone().oneshot(req_b_delete).await.unwrap();
    assert_eq!(res_b_delete.status(), StatusCode::NOT_FOUND);

    // 10. Attempt request with invalid API Key -> 401 UNAUTHORIZED
    let req_invalid = Request::builder()
        .method("GET")
        .uri("/v1/calendars")
        .header(HEADER_API_KEY, "invalid_bogus_key_xyz")
        .body(Body::empty())
        .unwrap();

    let res_invalid = app.clone().oneshot(req_invalid).await.unwrap();
    assert_eq!(res_invalid.status(), StatusCode::UNAUTHORIZED);

    // Verify Tenant A's event was never touched or deleted
    let event_check = event_repo.find_by_id(tenant_a, event_id).await.unwrap();
    assert!(event_check.is_some());
    assert_eq!(event_check.unwrap().title, "Top Secret Strategy Review");

    let cal_check = cal_repo.find_by_id(tenant_a, cal_id).await.unwrap();
    assert!(cal_check.is_some());
}

#[tokio::test]
async fn test_overlapping_events_on_strict_calendar_returns_409_conflict() {
    let mock_validator = Arc::new(TestInMemoryApiKeyValidator::default());
    let tenant_id = Uuid::new_v4();
    let api_key = "ikeda_test_key_strict_cal";
    mock_validator.register_key(api_key, tenant_id, vec!["*".into()]);

    let (app, _, _, _) = setup_test_app(mock_validator);

    // 1. Create strict calendar (allow_overlaps = false)
    let cal_cmd = CreateCalendarCommand {
        name: "Doctor Examination Room".to_string(),
        timezone: Some("UTC".to_string()),
        allow_overlaps: Some(false),
        working_hours: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/v1/calendars")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, api_key)
        .body(Body::from(serde_json::to_string(&cal_cmd).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let cal_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let cal_id = cal_json["data"]["id"].as_str().unwrap();

    let now = Utc::now();
    let start_1 = now + Duration::hours(2);
    let end_1 = now + Duration::hours(3);

    // 2. Schedule First Event (14:00 - 15:00) -> 201 Created
    let event1_cmd = CreateEventCommand {
        calendar_id: Uuid::parse_str(cal_id).unwrap(),
        title: "Surgery Appointment 1".to_string(),
        description: None,
        starts_at: start_1,
        ends_at: end_1,
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, api_key)
        .body(Body::from(serde_json::to_string(&event1_cmd).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 3. Schedule Colliding Event (14:30 - 15:30) -> 409 Conflict
    let event2_cmd = CreateEventCommand {
        calendar_id: Uuid::parse_str(cal_id).unwrap(),
        title: "Conflicting Surgery".to_string(),
        description: None,
        starts_at: start_1 + Duration::minutes(30),
        ends_at: end_1 + Duration::minutes(30),
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, api_key)
        .body(Body::from(serde_json::to_string(&event2_cmd).unwrap()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let err_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(err_json["success"], false);
    assert!(err_json["message"].as_str().unwrap().contains("conflicts"));
}

#[tokio::test]
async fn test_recurring_event_with_cancellation_exception_returns_exact_occurrences() {
    let mock_validator = Arc::new(TestInMemoryApiKeyValidator::default());
    let tenant_id = Uuid::new_v4();
    let api_key = "ikeda_test_key_recurring";
    mock_validator.register_key(api_key, tenant_id, vec!["*".into()]);

    let (app, state, _, _) = setup_test_app(mock_validator);

    let cal = Calendar::new(tenant_id, "Gym Class", "UTC", true, None);
    let created_cal = state.calendar_repo.create(tenant_id, &cal).await.unwrap();

    // 1. Create a daily recurring event for 5 days
    let start_date = Utc.with_ymd_and_hms(2026, 12, 1, 9, 0, 0).unwrap();
    let end_date = Utc.with_ymd_and_hms(2026, 12, 1, 10, 0, 0).unwrap();

    let event = Event::new(
        tenant_id,
        created_cal.id,
        "Daily Yoga",
        None,
        TimeRange::new(start_date, end_date).unwrap(),
        "UTC",
        Some("FREQ=DAILY;COUNT=5".to_string()),
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    let created_event = state.event_repo.create(tenant_id, &event).await.unwrap();

    // 2. Add an exception cancelling day 3 (2026-12-03)
    let day3_dt = Utc.with_ymd_and_hms(2026, 12, 3, 9, 0, 0).unwrap();
    let exception = EventException::cancel(tenant_id, created_event.id, day3_dt);
    state
        .event_repo
        .add_exception(tenant_id, &exception)
        .await
        .unwrap();

    // 3. Query occurrences via GET /v1/events across the 5 days
    let query_start = Utc
        .with_ymd_and_hms(2026, 12, 1, 0, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let query_end = Utc
        .with_ymd_and_hms(2026, 12, 6, 0, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let uri = format!(
        "/v1/events?calendar_id={}&starts_at={}&ends_at={}",
        created_cal.id, query_start, query_end
    );

    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(HEADER_API_KEY, api_key)
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let instances = body_json["data"].as_array().unwrap();

    // Exactly 4 occurrences returned (Day 1, Day 2, Day 4, Day 5) - Day 3 was cancelled!
    assert_eq!(instances.len(), 4);
}

#[tokio::test]
async fn test_calendar_and_event_crud_via_http() {
    let mock_validator = Arc::new(TestInMemoryApiKeyValidator::default());
    let tenant_id = Uuid::new_v4();
    let api_key = "ikeda_crud_key";
    mock_validator.register_key(api_key, tenant_id, vec!["*".into()]);

    let (app, _, _, _) = setup_test_app(mock_validator);

    // 1. Create Calendar
    let create_cal = CreateCalendarCommand {
        name: "Conference Hall".to_string(),
        timezone: Some("UTC".to_string()),
        allow_overlaps: Some(true),
        working_hours: None,
    };
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/calendars")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, api_key)
                .body(Body::from(serde_json::to_string(&create_cal).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cal_id = Uuid::parse_str(json["data"]["id"].as_str().unwrap()).unwrap();

    // 2. Patch Calendar
    let patch_cal = UpdateCalendarCommand {
        name: Some("Grand Hall".to_string()),
        timezone: None,
        allow_overlaps: None,
        working_hours: None,
    };
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/calendars/{cal_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, api_key)
                .body(Body::from(serde_json::to_string(&patch_cal).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Delete Calendar
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/calendars/{cal_id}"))
                .header(HEADER_API_KEY, api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_event_exception_endpoint() {
    let mock_validator = Arc::new(TestInMemoryApiKeyValidator::default());
    let tenant_id = Uuid::new_v4();
    let api_key = "ikeda_exception_key";
    mock_validator.register_key(api_key, tenant_id, vec!["*".into()]);

    let (app, state, _, _) = setup_test_app(mock_validator);

    let cal = Calendar::new(tenant_id, "Yoga Studio", "UTC", true, None);
    let created_cal = state.calendar_repo.create(tenant_id, &cal).await.unwrap();

    let now = Utc::now();
    let recurring = Event::new(
        tenant_id,
        created_cal.id,
        "Daily Practice",
        None,
        TimeRange::new(now, now + Duration::hours(1)).unwrap(),
        "UTC",
        Some("FREQ=DAILY;COUNT=7".to_string()),
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    let created_event = state
        .event_repo
        .create(tenant_id, &recurring)
        .await
        .unwrap();

    let cmd = CreateEventExceptionCommand {
        original_starts_at: now + Duration::days(2),
        is_cancelled: true,
        rescheduled_starts_at: None,
        rescheduled_ends_at: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/events/{}/exceptions", created_event.id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(HEADER_API_KEY, api_key)
        .body(Body::from(serde_json::to_string(&cmd).unwrap()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_qstash_scheduler_integration() {
    let scheduler = MockNotificationScheduler::default();
    let tenant_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let calendar_id = Uuid::new_v4();
    let now = Utc::now();
    let reminder_time = now + Duration::minutes(15);

    let payload = WebhookEventPayload {
        event_id,
        tenant_id,
        calendar_id,
        event_type: "event.reminder".to_string(),
        starts_at: now + Duration::hours(1),
        ends_at: now + Duration::hours(2),
        title: "Doctor Consultation".to_string(),
        metadata: serde_json::json!({ "customer_email": "patient@example.com" }),
    };

    let msg_id = scheduler
        .schedule_reminder(
            tenant_id,
            "https://api.client.com/webhooks/reminder",
            reminder_time,
            payload.clone(),
        )
        .await
        .unwrap();

    assert!(msg_id.starts_with("mock_msg_"));

    let lock = scheduler.scheduled_reminders.lock().unwrap();
    assert_eq!(lock.len(), 1);
    assert_eq!(lock[0].0, "https://api.client.com/webhooks/reminder");
    assert_eq!(lock[0].1, reminder_time);
    assert_eq!(lock[0].2, payload);
}
