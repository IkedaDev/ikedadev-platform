use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use bookings_api::application::use_cases::{
    CreateBookingCommand, TransitionBookingStatusCommand, UpdateBookingCommand,
};
use bookings_api::domain::models::BookingStatus;
use bookings_api::domain::services::BookingItemInput;
use bookings_api::infrastructure::{SqlxApiKeyValidator, SqlxBookingRepository};
use bookings_api::presentation::{create_router, ApiDoc, AppState};
use chrono::{Duration, Utc};
use ikeda_core::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, Pagination,
};
use ikeda_core::tenant::{hash_api_key, CachedApiKeyValidator, HEADER_API_KEY};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

#[test]
fn test_bookings_openapi_spec_generation() {
    let openapi = ApiDoc::openapi();
    assert_eq!(openapi.info.title, "IkedaDev Bookings API");
    assert!(openapi.paths.paths.contains_key("/health"));
    assert!(openapi.paths.paths.contains_key("/v1/bookings"));
    assert!(openapi.paths.paths.contains_key("/v1/bookings/{id}"));
    assert!(openapi.paths.paths.contains_key("/v1/bookings/{id}/status"));
    assert!(openapi.paths.paths.contains_key("/v1/bookings/search"));
}

#[tokio::test]
async fn test_bookings_rls_idempotency_transitions_criteria_and_soft_delete_on_postgres() {
    let db_url = std::env::var("BOOKINGS_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/bookings_db?sslmode=disable"
            .to_string()
    });

    let pool = match sqlx::PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("PostgreSQL bookings_db not reachable at {db_url}. Skipping live DB test.");
            return;
        }
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let key_a = format!("ik_bk_a_{}", Uuid::new_v4().simple());
    let key_b = format!("ik_bk_b_{}", Uuid::new_v4().simple());

    for (tid, name, raw_key) in [
        (tenant_a, "Tenant A Bookings Key", &key_a),
        (tenant_b, "Tenant B Bookings Key", &key_b),
    ] {
        let mut tx = pool.begin().await.expect("Failed to begin bookings key tx");
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tid.to_string())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");
        sqlx::query(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
            VALUES ($1, $2, 'ik_bk_key', $3, '{"*"}', true)
            "#,
        )
        .bind(tid)
        .bind(name)
        .bind(hash_api_key(raw_key))
        .execute(&mut *tx)
        .await
        .expect("Failed to insert bookings API key");
        tx.commit().await.expect("Failed to commit bookings key tx");
    }

    let state = AppState::new(
        Arc::new(SqlxBookingRepository::new(pool.clone())),
        Arc::new(CachedApiKeyValidator::new(SqlxApiKeyValidator::new(pool))),
    );
    let app = create_router(state);

    let start = Utc::now() + Duration::hours(2);
    let end = start + Duration::minutes(90);
    let client_id = Uuid::new_v4();
    let calendar_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let idem_key = format!("idem-{}", Uuid::new_v4().simple());

    let create_payload = CreateBookingCommand {
        client_id,
        calendar_id: Some(calendar_id),
        event_id: Some(event_id),
        starts_at: start,
        ends_at: end,
        timezone: Some("America/Santiago".to_string()),
        status: Some(BookingStatus::Scheduled),
        currency: Some("CLP".to_string()),
        notes: Some("Reserva multi-servicio".to_string()),
        metadata: Some(serde_json::json!({ "channel": "portal", "tier": "vip" })),
        items: vec![
            BookingItemInput {
                catalog_item_id: Uuid::new_v4(),
                variant_id: None,
                name_snapshot: "Servicio A".to_string(),
                sku_snapshot: Some("SVC-A".to_string()),
                quantity: 1,
                unit_price_snapshot: 23_800,
                tax_rate_bps: 1_900,
                tax_included: true,
                duration_minutes_snapshot: 60,
                buffer_minutes_snapshot: 0,
                metadata: None,
            },
            BookingItemInput {
                catalog_item_id: Uuid::new_v4(),
                variant_id: Some(Uuid::new_v4()),
                name_snapshot: "Servicio B".to_string(),
                sku_snapshot: Some("SVC-B".to_string()),
                quantity: 2,
                unit_price_snapshot: 10_000,
                tax_rate_bps: 1_900,
                tax_included: false,
                duration_minutes_snapshot: 15,
                buffer_minutes_snapshot: 0,
                metadata: None,
            },
        ],
    };

    // 1. Tenant A creates multi-service booking with Idempotency-Key
    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bookings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .header("Idempotency-Key", &idem_key)
                .body(Body::from(serde_json::to_string(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create.status(), StatusCode::CREATED);
    let created_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let booking_id = Uuid::parse_str(created_json["data"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(created_json["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(created_json["data"]["total_price"], 47_600);
    assert_eq!(created_json["data"]["total_duration_minutes"], 90);

    // 2. Idempotency replay with same key + same payload returns identical booking
    let res_replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bookings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .header("Idempotency-Key", &idem_key)
                .body(Body::from(serde_json::to_string(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_replay.status(), StatusCode::CREATED);
    let replay_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_replay.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay_json["data"]["id"], created_json["data"]["id"]);

    // 3. Idempotency conflict with same key + different payload -> 409 Conflict
    let mut altered_payload = create_payload.clone();
    altered_payload.notes = Some("Diferente".to_string());
    let res_conflict = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bookings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .header("Idempotency-Key", &idem_key)
                .body(Body::from(serde_json::to_string(&altered_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_conflict.status(), StatusCode::CONFLICT);

    // 4. Strict RLS: Tenant B cannot GET, PATCH, or transition Tenant A's booking
    let res_b_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/bookings/{booking_id}"))
                .header(HEADER_API_KEY, &key_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_get.status(), StatusCode::NOT_FOUND);

    let res_b_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/bookings/{booking_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_b)
                .body(Body::from(
                    serde_json::to_string(&UpdateBookingCommand {
                        notes: Some("Cross-tenant hack".to_string()),
                        ..Default::default()
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_patch.status(), StatusCode::NOT_FOUND);

    let res_b_status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/bookings/{booking_id}/status"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_b)
                .body(Body::from(
                    serde_json::to_string(&TransitionBookingStatusCommand {
                        status: BookingStatus::Cancelled,
                        cancellation_reason: Some("Hack".to_string()),
                        metadata: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_status.status(), StatusCode::NOT_FOUND);

    // 5. Criteria search filtering by status, starts_at range, and metadata.tier
    let search_criteria = Criteria {
        filter: Some(FilterGroup::And(vec![
            FilterRule::Condition(FilterCondition {
                field: "status".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("SCHEDULED"),
            }),
            FilterRule::Condition(FilterCondition {
                field: "starts_at".to_string(),
                operator: FilterOperator::Gte,
                value: serde_json::json!((start - Duration::minutes(10)).to_rfc3339()),
            }),
            FilterRule::Condition(FilterCondition {
                field: "metadata.tier".to_string(),
                operator: FilterOperator::Eq,
                value: serde_json::json!("vip"),
            }),
        ])),
        order_by: Some(vec![OrderBy::desc("starts_at")]),
        pagination: Some(Pagination { page: 1, limit: 10 }),
    };

    let res_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bookings/search")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&search_criteria).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_search.status(), StatusCode::OK);
    let search_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_search.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(search_json["data"][0]["items"].as_array().unwrap().len(), 2);

    // 6. Status transitions: SCHEDULED -> CANCELLED with reason, then reactivating -> 409
    let res_cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/bookings/{booking_id}/status"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&TransitionBookingStatusCommand {
                        status: BookingStatus::Cancelled,
                        cancellation_reason: Some("Cliente canceló a tiempo".to_string()),
                        metadata: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_cancel.status(), StatusCode::OK);

    let res_reactivate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/bookings/{booking_id}/status"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&TransitionBookingStatusCommand {
                        status: BookingStatus::Scheduled,
                        cancellation_reason: None,
                        metadata: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_reactivate.status(), StatusCode::CONFLICT);

    // 7. Create a second booking and transition SCHEDULED -> COMPLETED
    let res_create2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bookings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(serde_json::to_string(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create2.status(), StatusCode::CREATED);
    let created2_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res_create2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let booking2_id = Uuid::parse_str(created2_json["data"]["id"].as_str().unwrap()).unwrap();

    let res_complete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/bookings/{booking2_id}/status"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(HEADER_API_KEY, &key_a)
                .body(Body::from(
                    serde_json::to_string(&TransitionBookingStatusCommand {
                        status: BookingStatus::Completed,
                        cancellation_reason: None,
                        metadata: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_complete.status(), StatusCode::OK);

    // 8. Soft Delete hides booking from GET and search
    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/bookings/{booking_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::NO_CONTENT);

    let res_after_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/bookings/{booking_id}"))
                .header(HEADER_API_KEY, &key_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_after_del.status(), StatusCode::NOT_FOUND);
}
