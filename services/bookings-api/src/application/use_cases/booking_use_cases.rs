use crate::domain::models::{Booking, BookingItem, BookingStatus};
use crate::domain::repositories::{BookingRepository, IdempotentCreateResult};
use crate::domain::services::{
    calculate_booking_totals, generate_booking_code, validate_currency, validate_time_range,
    BookingItemInput,
};
use chrono::{DateTime, Duration, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::{AppError, ValidationErrorDetail};
use ikeda_core::response::PaginatedApiResponse;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Payload to create a new multi-service booking with frozen commercial snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateBookingCommand {
    pub client_id: Uuid,
    #[serde(default)]
    pub calendar_id: Option<Uuid>,
    #[serde(default)]
    pub event_id: Option<Uuid>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub status: Option<BookingStatus>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    pub items: Vec<BookingItemInput>,
}

/// Payload for partial updates on a non-terminal booking.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateBookingCommand {
    #[serde(default)]
    pub calendar_id: Option<Uuid>,
    #[serde(default)]
    pub event_id: Option<Uuid>,
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ends_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub items: Option<Vec<BookingItemInput>>,
}

/// Payload to transition a booking to a new lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransitionBookingStatusCommand {
    pub status: BookingStatus,
    #[serde(default)]
    pub cancellation_reason: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Computes a deterministic SHA-256 hex digest of a serializable request payload.
fn hash_request_payload<T: Serialize>(payload: &T) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|e| AppError::InternalServerError(format!("Failed to serialize payload: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn build_booking_items(
    tenant_id: Uuid,
    booking_id: Uuid,
    inputs: &[BookingItemInput],
    calculated: &[crate::domain::services::CalculatedLineItem],
    now: DateTime<Utc>,
) -> Vec<BookingItem> {
    inputs
        .iter()
        .zip(calculated.iter())
        .map(|(input, calc)| BookingItem {
            id: Uuid::now_v7(),
            tenant_id,
            booking_id,
            catalog_item_id: input.catalog_item_id,
            variant_id: input.variant_id,
            name_snapshot: input.name_snapshot.trim().to_string(),
            sku_snapshot: input
                .sku_snapshot
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string),
            quantity: input.quantity,
            unit_price_snapshot: input.unit_price_snapshot,
            tax_rate_bps: input.tax_rate_bps,
            tax_included: input.tax_included,
            line_subtotal: calc.line_subtotal,
            line_tax: calc.line_tax,
            line_total: calc.line_total,
            duration_minutes_snapshot: input.duration_minutes_snapshot,
            buffer_minutes_snapshot: input.buffer_minutes_snapshot,
            metadata: input
                .metadata
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
            created_at: now,
        })
        .collect()
}

/// Use case: creates a booking with frozen snapshots and optional `Idempotency-Key`.
pub struct CreateBookingUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl CreateBookingUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        actor_user_id: Option<Uuid>,
        cmd: CreateBookingCommand,
        idempotency_key: Option<&str>,
    ) -> Result<IdempotentCreateResult, AppError> {
        let initial_status = cmd.status.unwrap_or(BookingStatus::Scheduled);
        if !matches!(
            initial_status,
            BookingStatus::Pending | BookingStatus::Scheduled
        ) {
            return Err(AppError::validation(
                "Invalid initial booking status",
                [ValidationErrorDetail::new(
                    "status",
                    "Initial status on creation must be PENDING or SCHEDULED",
                )],
            ));
        }

        let currency = validate_currency(cmd.currency.as_deref().unwrap_or("CLP"))?;
        let timezone = cmd
            .timezone
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("UTC")
            .to_string();

        let totals = calculate_booking_totals(&cmd.items, cmd.starts_at, cmd.ends_at)?;
        let request_hash = hash_request_payload(&cmd)?;

        let now = Utc::now();
        let booking_id = Uuid::now_v7();
        let booking_code = generate_booking_code(now, booking_id);
        let items = build_booking_items(tenant_id, booking_id, &cmd.items, &totals.line_items, now);

        let booking = Booking {
            id: booking_id,
            tenant_id,
            booking_code,
            client_id: cmd.client_id,
            calendar_id: cmd.calendar_id,
            event_id: cmd.event_id,
            starts_at: cmd.starts_at,
            ends_at: cmd.ends_at,
            timezone,
            status: initial_status,
            subtotal: totals.subtotal,
            tax_total: totals.tax_total,
            total_price: totals.total_price,
            total_duration_minutes: totals.total_duration_minutes,
            currency,
            notes: cmd.notes,
            cancellation_reason: None,
            cancelled_at: None,
            completed_at: None,
            metadata: cmd.metadata.unwrap_or_else(|| serde_json::json!({})),
            created_by: actor_user_id,
            updated_by: actor_user_id,
            items,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        let trimmed_idem_key = idempotency_key.map(str::trim).filter(|k| !k.is_empty());
        if let Some(key) = trimmed_idem_key {
            if key.len() > 255 {
                return Err(AppError::validation(
                    "Invalid Idempotency-Key header",
                    [ValidationErrorDetail::new(
                        "Idempotency-Key",
                        "Idempotency-Key must be at most 255 characters",
                    )
                    .with_code("max_length")],
                ));
            }
        }

        let expires_at = now + Duration::hours(24);
        let idempotency = trimmed_idem_key.map(|k| (k, request_hash.as_str(), expires_at));

        self.repo
            .create_with_idempotency(tenant_id, &booking, idempotency)
            .await
    }
}

/// Use case: retrieves a single booking by ID including its line items.
pub struct GetBookingUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl GetBookingUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, id: Uuid) -> Result<Booking, AppError> {
        self.repo
            .find_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Booking with ID '{id}' not found")))
    }
}

/// Use case: updates a non-terminal booking (optionally replacing items and recalculating totals).
pub struct UpdateBookingUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl UpdateBookingUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor_user_id: Option<Uuid>,
        cmd: UpdateBookingCommand,
    ) -> Result<Booking, AppError> {
        let mut booking = self
            .repo
            .find_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Booking with ID '{id}' not found")))?;

        let expected_status = booking.status;
        if booking.status.is_terminal() {
            return Err(AppError::Conflict(format!(
                "Cannot edit booking '{}' because its status is terminal ({})",
                booking.id,
                booking.status.as_str()
            )));
        }

        if let Some(cal_id) = cmd.calendar_id {
            booking.calendar_id = Some(cal_id);
        }
        if let Some(evt_id) = cmd.event_id {
            booking.event_id = Some(evt_id);
        }
        if let Some(starts_at) = cmd.starts_at {
            booking.starts_at = starts_at;
        }
        if let Some(ends_at) = cmd.ends_at {
            booking.ends_at = ends_at;
        }
        validate_time_range(booking.starts_at, booking.ends_at)?;

        if let Some(tz) = cmd.timezone {
            let trimmed = tz.trim();
            if !trimmed.is_empty() {
                booking.timezone = trimmed.to_string();
            }
        }
        if let Some(notes) = cmd.notes {
            booking.notes = Some(notes);
        }
        if let Some(metadata) = cmd.metadata {
            booking.metadata = metadata;
        }

        let now = Utc::now();
        let replace_items = if let Some(ref new_items) = cmd.items {
            let totals = calculate_booking_totals(new_items, booking.starts_at, booking.ends_at)?;
            booking.subtotal = totals.subtotal;
            booking.tax_total = totals.tax_total;
            booking.total_price = totals.total_price;
            booking.total_duration_minutes = totals.total_duration_minutes;
            booking.items =
                build_booking_items(tenant_id, booking.id, new_items, &totals.line_items, now);
            true
        } else {
            false
        };

        booking.updated_by = actor_user_id;
        booking.updated_at = now;

        self.repo
            .update(tenant_id, &booking, expected_status, replace_items)
            .await
    }
}

/// Use case: transitions booking status following the strict state machine.
pub struct TransitionBookingStatusUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl TransitionBookingStatusUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor_user_id: Option<Uuid>,
        cmd: TransitionBookingStatusCommand,
    ) -> Result<Booking, AppError> {
        let mut booking = self
            .repo
            .find_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Booking with ID '{id}' not found")))?;

        let expected_status = booking.status;
        if !booking.status.can_transition_to(cmd.status) {
            return Err(AppError::Conflict(format!(
                "Invalid booking status transition from {} to {}",
                booking.status.as_str(),
                cmd.status.as_str()
            )));
        }

        let now = Utc::now();
        booking.status = cmd.status;
        if cmd.status == BookingStatus::Cancelled {
            booking.cancelled_at = Some(now);
            if let Some(reason) = cmd.cancellation_reason {
                let trimmed = reason.trim();
                if !trimmed.is_empty() {
                    booking.cancellation_reason = Some(trimmed.to_string());
                }
            }
        } else if cmd.status == BookingStatus::Completed {
            booking.completed_at = Some(now);
        }

        if let Some(patch_meta) = cmd.metadata {
            match (&mut booking.metadata, patch_meta) {
                (serde_json::Value::Object(existing), serde_json::Value::Object(incoming)) => {
                    for (k, v) in incoming {
                        existing.insert(k, v);
                    }
                }
                (_, replacement) => {
                    booking.metadata = replacement;
                }
            }
        }

        booking.updated_by = actor_user_id;
        booking.updated_at = now;

        self.repo
            .update(tenant_id, &booking, expected_status, false)
            .await
    }
}

/// Use case: soft-deletes a booking.
pub struct DeleteBookingUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl DeleteBookingUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let deleted = self.repo.soft_delete(tenant_id, id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Booking with ID '{id}' not found"
            )));
        }
        Ok(())
    }
}

/// Use case: dynamic paginated search using `ikeda_core::Criteria`.
pub struct SearchBookingsUseCase {
    repo: Arc<dyn BookingRepository>,
}

impl SearchBookingsUseCase {
    pub fn new(repo: Arc<dyn BookingRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        criteria: Criteria,
    ) -> Result<PaginatedApiResponse<Booking>, AppError> {
        let pagination = criteria.pagination.unwrap_or_default();
        let page = self.repo.search_by_criteria(tenant_id, &criteria).await?;
        let current_page = pagination.page.max(1);
        Ok(PaginatedApiResponse::new(
            page.items,
            current_page,
            pagination.sql_limit(),
            page.total_items,
        ))
    }
}
