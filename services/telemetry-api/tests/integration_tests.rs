use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, Pagination,
};
use ikeda_core::observability::REQUEST_ID_HEADER;
use ikeda_core::tenant::{hash_api_key, CachedApiKeyValidator, HEADER_API_KEY};
use serde_json::json;
use std::sync::Arc;
use telemetry_api::application::use_cases::{
    GetMetricsCommand, IngestBatchCommand, IngestEventCommand,
};
use telemetry_api::domain::models::{EventKind, MAX_BATCH_SIZE};
use telemetry_api::infrastructure::{SqlxApiKeyValidator, SqlxEventRepository};
use telemetry_api::presentation::{create_router, ApiDoc, AppState};
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_telemetry_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Telemetry API");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/events"));
    assert!(openapi.paths.paths.contains_key("/v1/events/batch"));
    assert!(openapi.paths.paths.contains_key("/v1/events/search"));
    assert!(openapi
        .paths
        .paths
        .contains_key("/v1/clients/{client_id}/timeline"));
    assert!(openapi.paths.paths.contains_key("/v1/metrics/summary"));
}

#[tokio::test]
async fn test_telemetry_rls_ingest_batch_search_timeline_and_metrics_on_postgres() {
    let db_url = std::env::var("TELEMETRY_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres_password@localhost:5432/telemetry_db?sslmode=disable"
            .to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL telemetry_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let key_a = format!("ik_tel_a_{}", Uuid::new_v4().simple());
    let key_b = format!("ik_tel_b_{}", Uuid::new_v4().simple());

    for (tid, name, raw_key) in [
        (tenant_a, "Tenant A Telemetry Key", &key_a),
        (tenant_b, "Tenant B Telemetry Key", &key_b),
    ] {
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ik_tel_k', $3, '{"telemetry:read","telemetry:write"}', true)
            "#,
        )
        .bind(tid)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .execute(&pool)
        .await
        .expect("Failed to insert telemetry API key");
    }

    let state = AppState::new(
        Arc::new(SqlxEventRepository::new(pool.clone())),
        Arc::new(CachedApiKeyValidator::new(SqlxApiKeyValidator::new(pool))),
    );
    let app = create_router(state);

    let client_a1 = Uuid::new_v4();
    let client_a2 = Uuid::new_v4();
    let booking_resource = Uuid::new_v4();
    let now = Utc::now();

    // 1. Tenant A ingests a single AUDIT event
    let single_cmd = IngestEventCommand {
        kind: EventKind::Audit,
        event_name: "booking.created".to_string(),
        source: Some("bff".to_string()),
        client_id: Some(client_a1),
        actor_id: Some(Uuid::new_v4()),
        anonymous_id: None,
        session_id: Some("sess-a-1".to_string()),
        resource_type: Some("booking".to_string()),
        resource_id: Some(booking_resource),
        request_id: None,
        properties: Some(json!({ "channel": "portal", "amount": "45000" })),
        context: Some(json!({ "env": "test" })),
        occurred_at: Some(now - Duration::minutes(5)),
    };

    let res_single = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .header(REQUEST_ID_HEADER, "req-trace-001")
                .body(Body::from(serde_json::to_string(&single_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_single.status(), StatusCode::CREATED);
    let single_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_single.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(single_json["data"]["event_name"], "booking.created");
    assert_eq!(single_json["data"]["request_id"], "req-trace-001");

    // 2. Batch validation: empty batch -> 422, >100 events -> 422
    let res_empty = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/batch")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&IngestBatchCommand { events: vec![] }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_empty.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let too_many: Vec<IngestEventCommand> = (0..=MAX_BATCH_SIZE)
        .map(|i| IngestEventCommand {
            kind: EventKind::Tracking,
            event_name: format!("page.view_{i}"),
            source: Some("web".to_string()),
            client_id: None,
            actor_id: None,
            anonymous_id: None,
            session_id: None,
            resource_type: None,
            resource_id: None,
            request_id: None,
            properties: Some(json!({})),
            context: Some(json!({})),
            occurred_at: None,
        })
        .collect();
    let res_too_many = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/batch")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&IngestBatchCommand { events: too_many }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_too_many.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 3. Tenant A ingests a valid batch of 3 TRACKING events
    let batch_cmd = IngestBatchCommand {
        events: vec![
            IngestEventCommand {
                kind: EventKind::Tracking,
                event_name: "page.viewed".to_string(),
                source: Some("web".to_string()),
                client_id: Some(client_a1),
                actor_id: None,
                anonymous_id: Some("anon-1".to_string()),
                session_id: Some("sess-a-1".to_string()),
                resource_type: None,
                resource_id: None,
                request_id: Some("req-batch-1".to_string()),
                properties: Some(json!({ "path": "/bookings", "utm": { "source": "google" } })),
                context: Some(json!({})),
                occurred_at: Some(now - Duration::minutes(4)),
            },
            IngestEventCommand {
                kind: EventKind::Tracking,
                event_name: "page.viewed".to_string(),
                source: Some("web".to_string()),
                client_id: Some(client_a2),
                actor_id: None,
                anonymous_id: Some("anon-2".to_string()),
                session_id: Some("sess-a-2".to_string()),
                resource_type: None,
                resource_id: None,
                request_id: Some("req-batch-1".to_string()),
                properties: Some(json!({ "path": "/catalog", "utm": { "source": "direct" } })),
                context: Some(json!({})),
                occurred_at: Some(now - Duration::minutes(3)),
            },
            IngestEventCommand {
                kind: EventKind::Tracking,
                event_name: "checkout.started".to_string(),
                source: Some("web".to_string()),
                client_id: Some(client_a2),
                actor_id: None,
                anonymous_id: Some("anon-2".to_string()),
                session_id: Some("sess-a-2".to_string()),
                resource_type: None,
                resource_id: None,
                request_id: Some("req-batch-1".to_string()),
                properties: Some(json!({ "path": "/checkout" })),
                context: Some(json!({})),
                occurred_at: Some(now - Duration::minutes(2)),
            },
        ],
    };

    let res_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/batch")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&batch_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_batch.status(), StatusCode::CREATED);
    let batch_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_batch.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(batch_json["data"]["accepted"], 3);

    // 4. Criteria + JSONB search (properties.utm.source = "google") for Tenant A -> 1 result
    let criteria = Criteria {
        filter: Some(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition {
                field: "kind".to_string(),
                operator: FilterOperator::Eq,
                value: json!("TRACKING"),
            }),
            FilterRule::Condition(FilterCondition {
                field: "properties.utm.source".to_string(),
                operator: FilterOperator::Eq,
                value: json!("google"),
            }),
        ])),
        order_by: Some(vec![OrderBy::desc("occurred_at")]),
        pagination: Some(Pagination { page: 1, limit: 10 }),
    };

    let res_search_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search_a.status(), StatusCode::OK);
    let search_a_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_a_json["pagination"]["total_items"], 1);
    assert_eq!(search_a_json["data"][0]["properties"]["path"], "/bookings");

    // 5. Strict RLS: Tenant B searches with empty criteria and sees ZERO of Tenant A's events
    let empty_criteria = Criteria {
        filter: None,
        order_by: None,
        pagination: Some(Pagination { page: 1, limit: 20 }),
    };
    let res_search_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_b)
                .body(Body::from(serde_json::to_string(&empty_criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search_b.status(), StatusCode::OK);
    let search_b_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_b_json["pagination"]["total_items"], 0);

    // 6. Client Timeline for client_a1 (Tenant A sees 2 events; Tenant B sees 0)
    let res_tl_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/clients/{client_a1}/timeline?page=1&limit=10"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_tl_a.status(), StatusCode::OK);
    let tl_a_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_tl_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tl_a_json["pagination"]["total_items"], 2);

    let res_tl_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/clients/{client_a1}/timeline?page=1&limit=10"))
                .header(HEADER_API_KEY, &key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_tl_b.status(), StatusCode::OK);
    let tl_b_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_tl_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tl_b_json["pagination"]["total_items"], 0);

    // 7. Metrics summary for Tenant A
    let metrics_cmd = GetMetricsCommand {
        from: now - Duration::hours(1),
        to: now + Duration::minutes(1),
        kind: None,
        source: None,
        top_limit: Some(10),
    };
    let res_metrics_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/metrics/summary")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&metrics_cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_metrics_a.status(), StatusCode::OK);
    let metrics_a_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_metrics_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(metrics_a_json["data"]["total_events"], 4);
    assert_eq!(metrics_a_json["data"]["unique_clients"], 2);
    assert_eq!(metrics_a_json["data"]["unique_sessions"], 2);
    assert_eq!(
        metrics_a_json["data"]["by_event_name"][0]["event_name"],
        "page.viewed"
    );
    assert_eq!(metrics_a_json["data"]["by_event_name"][0]["count"], 2);
}
