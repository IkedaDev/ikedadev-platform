use crate::application::use_cases::{
    GetClientTimelineUseCase, GetMetricsCommand, GetMetricsUseCase, IngestBatchCommand,
    IngestBatchEventsUseCase, IngestEventCommand, IngestEventUseCase, SearchEventsUseCase,
};
use crate::domain::models::{
    ClientTimelineQuery, EventKind, EventMetrics, EventNameCount, MetricsQuery, TelemetryEvent,
    MAX_BATCH_SIZE,
};
use crate::domain::repositories::{EventRepository, EventSearchPage};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use ikeda_core::criteria::{Criteria, Pagination};
use ikeda_core::error::AppError;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct InMemoryEventRepo {
    events: Mutex<Vec<TelemetryEvent>>,
}

#[async_trait]
impl EventRepository for InMemoryEventRepo {
    async fn insert(
        &self,
        _tenant_id: Uuid,
        event: &TelemetryEvent,
    ) -> Result<TelemetryEvent, AppError> {
        let mut guard = self.events.lock().unwrap();
        guard.push(event.clone());
        Ok(event.clone())
    }

    async fn insert_batch(
        &self,
        _tenant_id: Uuid,
        events: &[TelemetryEvent],
    ) -> Result<usize, AppError> {
        let mut guard = self.events.lock().unwrap();
        guard.extend(events.iter().cloned());
        Ok(events.len())
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<EventSearchPage, AppError> {
        let guard = self.events.lock().unwrap();
        let mut filtered: Vec<TelemetryEvent> = guard
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .cloned()
            .collect();
        filtered.sort_by_key(|b| std::cmp::Reverse(b.occurred_at));

        let total_items = filtered.len() as u64;
        let pagination = criteria.pagination.unwrap_or_default();
        let page = pagination.page.max(1);
        let limit = pagination.limit.clamp(1, 100);
        let start = ((page - 1) * limit) as usize;
        let items = filtered
            .into_iter()
            .skip(start)
            .take(limit as usize)
            .collect();

        Ok(EventSearchPage { items, total_items })
    }

    async fn get_metrics(
        &self,
        tenant_id: Uuid,
        query: &MetricsQuery,
    ) -> Result<EventMetrics, AppError> {
        let guard = self.events.lock().unwrap();
        let matching: Vec<&TelemetryEvent> = guard
            .iter()
            .filter(|e| {
                e.tenant_id == tenant_id
                    && e.occurred_at >= query.from
                    && e.occurred_at <= query.to
                    && query.kind.is_none_or(|k| e.kind == k)
                    && query.source.as_deref().is_none_or(|s| e.source == s)
            })
            .collect();

        let total_events = matching.len() as u64;
        let unique_clients: HashSet<Uuid> = matching.iter().filter_map(|e| e.client_id).collect();
        let unique_sessions: HashSet<&str> = matching
            .iter()
            .filter_map(|e| e.session_id.as_deref())
            .collect();

        let mut counts: HashMap<String, u64> = HashMap::new();
        for ev in &matching {
            *counts.entry(ev.event_name.clone()).or_insert(0) += 1;
        }
        let mut by_event_name: Vec<EventNameCount> = counts
            .into_iter()
            .map(|(event_name, count)| EventNameCount { event_name, count })
            .collect();
        by_event_name.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.event_name.cmp(&b.event_name))
        });
        by_event_name.truncate(query.top_limit as usize);

        Ok(EventMetrics {
            from: query.from,
            to: query.to,
            total_events,
            unique_clients: unique_clients.len() as u64,
            unique_sessions: unique_sessions.len() as u64,
            by_event_name,
        })
    }

    async fn get_client_timeline(
        &self,
        tenant_id: Uuid,
        query: &ClientTimelineQuery,
    ) -> Result<EventSearchPage, AppError> {
        let guard = self.events.lock().unwrap();
        let mut matching: Vec<TelemetryEvent> = guard
            .iter()
            .filter(|e| {
                e.tenant_id == tenant_id
                    && e.client_id == Some(query.client_id)
                    && query.kind.is_none_or(|k| e.kind == k)
            })
            .cloned()
            .collect();
        matching.sort_by_key(|b| std::cmp::Reverse(b.occurred_at));

        let total_items = matching.len() as u64;
        let start = ((query.page - 1) * query.limit) as usize;
        let items = matching
            .into_iter()
            .skip(start)
            .take(query.limit as usize)
            .collect();

        Ok(EventSearchPage { items, total_items })
    }
}

fn sample_cmd(event_name: &str) -> IngestEventCommand {
    IngestEventCommand {
        kind: EventKind::Tracking,
        event_name: event_name.to_string(),
        source: Some("web".to_string()),
        client_id: Some(Uuid::new_v4()),
        actor_id: None,
        anonymous_id: Some("anon-1".to_string()),
        session_id: Some("sess-1".to_string()),
        resource_type: Some("booking".to_string()),
        resource_id: Some(Uuid::new_v4()),
        request_id: None,
        properties: Some(json!({ "path": "/dashboard" })),
        context: Some(json!({ "locale": "es-CL" })),
        occurred_at: None,
    }
}

#[tokio::test]
async fn test_ingest_single_event_validates_and_applies_fallbacks() {
    let repo = Arc::new(InMemoryEventRepo::default());
    let uc = IngestEventUseCase::new(repo);
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();

    let ev = uc
        .execute(
            tenant_id,
            Some(actor_id),
            Some("req-123".to_string()),
            sample_cmd("page.viewed"),
        )
        .await
        .unwrap();

    assert_eq!(ev.tenant_id, tenant_id);
    assert_eq!(ev.actor_id, Some(actor_id));
    assert_eq!(ev.request_id.as_deref(), Some("req-123"));
    assert_eq!(ev.source, "web");
    assert_eq!(ev.event_name, "page.viewed");
}

#[tokio::test]
async fn test_ingest_rejects_invalid_event_name_and_non_object_json() {
    let repo = Arc::new(InMemoryEventRepo::default());
    let uc = IngestEventUseCase::new(repo);
    let tenant_id = Uuid::new_v4();

    for bad_name in [
        "",
        "invalid",
        "Page.Viewed",
        "page..viewed",
        ".page.viewed",
        "page.viewed.",
        "page.view ed",
    ] {
        let cmd = sample_cmd(bad_name);
        let err = uc.execute(tenant_id, None, None, cmd).await.unwrap_err();
        assert!(
            matches!(err, AppError::UnprocessableEntity { .. }),
            "expected 422 for '{bad_name}'"
        );
    }

    let mut bad_props = sample_cmd("booking.created");
    bad_props.properties = Some(json!(["not", "an", "object"]));
    let err = uc
        .execute(tenant_id, None, None, bad_props)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::UnprocessableEntity { .. }));

    let mut bad_ctx = sample_cmd("booking.created");
    bad_ctx.context = Some(json!("string-not-object"));
    let err = uc
        .execute(tenant_id, None, None, bad_ctx)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::UnprocessableEntity { .. }));
}

#[tokio::test]
async fn test_ingest_batch_enforces_bounds_1_to_100() {
    let repo = Arc::new(InMemoryEventRepo::default());
    let uc = IngestBatchEventsUseCase::new(repo);
    let tenant_id = Uuid::new_v4();

    // Empty batch -> 422
    let err_empty = uc
        .execute(tenant_id, None, None, IngestBatchCommand { events: vec![] })
        .await
        .unwrap_err();
    assert!(matches!(err_empty, AppError::UnprocessableEntity { .. }));

    // 101 events -> 422
    let too_many: Vec<IngestEventCommand> = (0..=MAX_BATCH_SIZE)
        .map(|i| sample_cmd(&format!("batch.event_{i}")))
        .collect();
    let err_large = uc
        .execute(
            tenant_id,
            None,
            None,
            IngestBatchCommand { events: too_many },
        )
        .await
        .unwrap_err();
    assert!(matches!(err_large, AppError::UnprocessableEntity { .. }));

    // Valid batch of 3 -> accepted: 3
    let valid = vec![
        sample_cmd("page.viewed"),
        sample_cmd("booking.created"),
        sample_cmd("client.updated"),
    ];
    let res = uc
        .execute(
            tenant_id,
            None,
            Some("req-batch".to_string()),
            IngestBatchCommand { events: valid },
        )
        .await
        .unwrap();
    assert_eq!(res.accepted, 3);
}

#[tokio::test]
async fn test_metrics_timeline_and_search_use_cases() {
    let repo = Arc::new(InMemoryEventRepo::default());
    let ingest = IngestBatchEventsUseCase::new(repo.clone());
    let search = SearchEventsUseCase::new(repo.clone());
    let timeline = GetClientTimelineUseCase::new(repo.clone());
    let metrics = GetMetricsUseCase::new(repo);

    let tenant_id = Uuid::new_v4();
    let client_a = Uuid::new_v4();
    let client_b = Uuid::new_v4();
    let now = Utc::now();

    let mut ev1 = sample_cmd("page.viewed");
    ev1.client_id = Some(client_a);
    ev1.session_id = Some("s1".to_string());
    ev1.occurred_at = Some(now - Duration::minutes(10));

    let mut ev2 = sample_cmd("page.viewed");
    ev2.client_id = Some(client_b);
    ev2.session_id = Some("s2".to_string());
    ev2.occurred_at = Some(now - Duration::minutes(5));

    let mut ev3 = sample_cmd("booking.created");
    ev3.kind = EventKind::Audit;
    ev3.client_id = Some(client_a);
    ev3.session_id = Some("s1".to_string());
    ev3.occurred_at = Some(now - Duration::minutes(1));

    ingest
        .execute(
            tenant_id,
            None,
            None,
            IngestBatchCommand {
                events: vec![ev1, ev2, ev3],
            },
        )
        .await
        .unwrap();

    // Search returns all 3
    let page = search
        .execute(
            tenant_id,
            Criteria {
                filter: None,
                order_by: None,
                pagination: Some(Pagination { page: 1, limit: 10 }),
            },
        )
        .await
        .unwrap();
    assert_eq!(page.pagination.total_items, 3);

    // Timeline for client_a filtered to AUDIT returns 1
    let tl = timeline
        .execute(
            tenant_id,
            client_a,
            Some(EventKind::Audit),
            Some(1),
            Some(10),
        )
        .await
        .unwrap();
    assert_eq!(tl.pagination.total_items, 1);
    assert_eq!(tl.data[0].event_name, "booking.created");

    // Metrics summary across window
    let summary = metrics
        .execute(
            tenant_id,
            GetMetricsCommand {
                from: now - Duration::hours(1),
                to: now + Duration::minutes(1),
                kind: None,
                source: None,
                top_limit: Some(10),
            },
        )
        .await
        .unwrap();
    assert_eq!(summary.total_events, 3);
    assert_eq!(summary.unique_clients, 2);
    assert_eq!(summary.unique_sessions, 2);
    assert_eq!(summary.by_event_name[0].event_name, "page.viewed");
    assert_eq!(summary.by_event_name[0].count, 2);

    // Invalid date range (to < from) -> 422
    let err_range = metrics
        .execute(
            tenant_id,
            GetMetricsCommand {
                from: now,
                to: now - Duration::hours(1),
                kind: None,
                source: None,
                top_limit: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err_range, AppError::UnprocessableEntity { .. }));
}
