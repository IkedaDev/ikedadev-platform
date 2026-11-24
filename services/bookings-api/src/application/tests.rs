use crate::application::use_cases::{
    CreateBookingCommand, CreateBookingUseCase, DeleteBookingUseCase, GetBookingUseCase,
    TransitionBookingStatusCommand, TransitionBookingStatusUseCase, UpdateBookingCommand,
    UpdateBookingUseCase,
};
use crate::domain::models::{Booking, BookingStatus};
use crate::domain::repositories::{
    BookingRepository, BookingSearchPage, IdempotencyRecord, IdempotentCreateResult,
};
use crate::domain::services::BookingItemInput;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::ApiResponse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct InMemoryBookingRepo {
    bookings: Mutex<HashMap<(Uuid, Uuid), Booking>>,
    idempotency: Mutex<HashMap<(Uuid, String), IdempotencyRecord>>,
}

#[async_trait]
impl BookingRepository for InMemoryBookingRepo {
    async fn create_with_idempotency(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        idempotency: Option<(&str, &str, DateTime<Utc>)>,
    ) -> Result<IdempotentCreateResult, AppError> {
        if let Some((key, req_hash, expires_at)) = idempotency {
            let mut idem_map = self.idempotency.lock().unwrap();
            if let Some(existing) = idem_map.get(&(tenant_id, key.to_string())) {
                if existing.expires_at > Utc::now() {
                    if existing.request_hash == req_hash {
                        return Ok(IdempotentCreateResult::Replayed {
                            status: existing.response_status,
                            body: existing.response_body.clone(),
                        });
                    }
                    return Err(AppError::Conflict(
                        "Idempotency-Key already used with a different request payload".to_string(),
                    ));
                }
            }
            let body = serde_json::to_value(ApiResponse::created(
                booking.clone(),
                "Booking created successfully",
            ))
            .unwrap();
            idem_map.insert(
                (tenant_id, key.to_string()),
                IdempotencyRecord {
                    key: key.to_string(),
                    request_hash: req_hash.to_string(),
                    response_status: 201,
                    response_body: body,
                    expires_at,
                },
            );
        }

        let mut map = self.bookings.lock().unwrap();
        map.insert((tenant_id, booking.id), booking.clone());
        Ok(IdempotentCreateResult::Created(Box::new(booking.clone())))
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Booking>, AppError> {
        let map = self.bookings.lock().unwrap();
        Ok(map
            .get(&(tenant_id, id))
            .filter(|b| b.deleted_at.is_none())
            .cloned())
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        _replace_items: bool,
    ) -> Result<Booking, AppError> {
        let mut map = self.bookings.lock().unwrap();
        if let Some(entry) = map.get_mut(&(tenant_id, booking.id)) {
            if entry.deleted_at.is_none() {
                *entry = booking.clone();
                return Ok(booking.clone());
            }
        }
        Err(AppError::NotFound(format!(
            "Booking '{}' not found",
            booking.id
        )))
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut map = self.bookings.lock().unwrap();
        if let Some(entry) = map.get_mut(&(tenant_id, id)) {
            if entry.deleted_at.is_none() {
                entry.deleted_at = Some(Utc::now());
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        _criteria: &Criteria,
    ) -> Result<BookingSearchPage, AppError> {
        let map = self.bookings.lock().unwrap();
        let items: Vec<Booking> = map
            .iter()
            .filter(|((tid, _), b)| *tid == tenant_id && b.deleted_at.is_none())
            .map(|(_, b)| b.clone())
            .collect();
        let total_items = items.len() as u64;
        Ok(BookingSearchPage { items, total_items })
    }
}

fn sample_create_cmd(start: DateTime<Utc>) -> CreateBookingCommand {
    CreateBookingCommand {
        client_id: Uuid::new_v4(),
        calendar_id: Some(Uuid::new_v4()),
        event_id: Some(Uuid::new_v4()),
        starts_at: start,
        ends_at: start + Duration::minutes(60),
        timezone: Some("America/Santiago".to_string()),
        status: None,
        currency: Some("CLP".to_string()),
        notes: Some("Primera reserva".to_string()),
        metadata: Some(serde_json::json!({ "channel": "web" })),
        items: vec![BookingItemInput {
            catalog_item_id: Uuid::new_v4(),
            variant_id: None,
            name_snapshot: "Servicio Estándar".to_string(),
            sku_snapshot: Some("SVC-STD".to_string()),
            quantity: 1,
            unit_price_snapshot: 23_800,
            tax_rate_bps: 1_900,
            tax_included: true,
            duration_minutes_snapshot: 60,
            buffer_minutes_snapshot: 0,
            metadata: None,
        }],
    }
}

#[tokio::test]
async fn test_idempotent_create_replays_same_payload_and_conflicts_on_different_payload() {
    let repo = Arc::new(InMemoryBookingRepo::default());
    let uc = CreateBookingUseCase::new(repo);
    let tenant_id = Uuid::new_v4();
    let cmd = sample_create_cmd(Utc::now());

    let first = uc
        .execute(tenant_id, None, cmd.clone(), Some("idem-001"))
        .await
        .unwrap();
    let created_id = match first {
        IdempotentCreateResult::Created(b) => b.id,
        IdempotentCreateResult::Replayed { .. } => panic!("Expected Created"),
    };

    // Replay with exact same payload
    let second = uc
        .execute(tenant_id, None, cmd.clone(), Some("idem-001"))
        .await
        .unwrap();
    match second {
        IdempotentCreateResult::Replayed { status, body } => {
            assert_eq!(status, 201);
            assert_eq!(body["data"]["id"].as_str().unwrap(), created_id.to_string());
        }
        IdempotentCreateResult::Created(_) => panic!("Expected Replayed"),
    }

    // Same key with different payload -> 409 Conflict
    let mut modified = cmd;
    modified.notes = Some("Changed payload".to_string());
    let conflict = uc
        .execute(tenant_id, None, modified, Some("idem-001"))
        .await;
    assert!(matches!(conflict, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn test_terminal_booking_blocks_updates_and_further_transitions() {
    let repo = Arc::new(InMemoryBookingRepo::default());
    let create_uc = CreateBookingUseCase::new(repo.clone());
    let transition_uc = TransitionBookingStatusUseCase::new(repo.clone());
    let update_uc = UpdateBookingUseCase::new(repo.clone());
    let get_uc = GetBookingUseCase::new(repo.clone());
    let delete_uc = DeleteBookingUseCase::new(repo);

    let tenant_id = Uuid::new_v4();
    let created = match create_uc
        .execute(tenant_id, None, sample_create_cmd(Utc::now()), None)
        .await
        .unwrap()
    {
        IdempotentCreateResult::Created(b) => b,
        IdempotentCreateResult::Replayed { .. } => panic!("Expected Created"),
    };

    // Transition SCHEDULED -> CANCELLED with metadata merge
    let cancelled = transition_uc
        .execute(
            tenant_id,
            created.id,
            None,
            TransitionBookingStatusCommand {
                status: BookingStatus::Cancelled,
                cancellation_reason: Some("Cliente reprogramó".to_string()),
                metadata: Some(serde_json::json!({ "cancelled_by": "portal" })),
            },
        )
        .await
        .unwrap();

    assert_eq!(cancelled.status, BookingStatus::Cancelled);
    assert!(cancelled.cancelled_at.is_some());
    assert_eq!(
        cancelled.cancellation_reason.as_deref(),
        Some("Cliente reprogramó")
    );
    assert_eq!(cancelled.metadata["channel"], "web");
    assert_eq!(cancelled.metadata["cancelled_by"], "portal");

    // Editing a cancelled booking -> 409 Conflict
    let edit_err = update_uc
        .execute(
            tenant_id,
            created.id,
            None,
            UpdateBookingCommand {
                notes: Some("No debería permitir".to_string()),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(edit_err, Err(AppError::Conflict(_))));

    // Reactivating a cancelled booking -> 409 Conflict
    let reactivate_err = transition_uc
        .execute(
            tenant_id,
            created.id,
            None,
            TransitionBookingStatusCommand {
                status: BookingStatus::Scheduled,
                cancellation_reason: None,
                metadata: None,
            },
        )
        .await;
    assert!(matches!(reactivate_err, Err(AppError::Conflict(_))));

    // Soft delete works and hides from get
    delete_uc.execute(tenant_id, created.id).await.unwrap();
    assert!(matches!(
        get_uc.execute(tenant_id, created.id).await,
        Err(AppError::NotFound(_))
    ));
}
