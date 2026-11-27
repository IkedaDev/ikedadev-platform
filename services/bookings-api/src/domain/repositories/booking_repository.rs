use crate::domain::models::{Booking, BookingStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

/// Paginated search result returned by `BookingRepository::search_by_criteria`.
#[derive(Debug, Clone)]
pub struct BookingSearchPage {
    pub items: Vec<Booking>,
    pub total_items: u64,
}

/// Stored idempotency record for `POST /v1/bookings`.
#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub key: String,
    pub request_hash: String,
    pub response_status: i16,
    pub response_body: serde_json::Value,
    pub expires_at: DateTime<Utc>,
}

/// Result of an idempotent booking creation attempt.
#[derive(Debug, Clone)]
pub enum IdempotentCreateResult {
    /// A brand-new booking was created and persisted.
    Created(Box<Booking>),
    /// An existing non-expired idempotency key matched the same `request_hash`.
    Replayed {
        status: i16,
        body: serde_json::Value,
    },
}

/// Repository port for multi-tenant booking persistence with PostgreSQL RLS.
#[async_trait]
pub trait BookingRepository: Send + Sync {
    /// Persists a new booking + its items (and optional idempotency key) atomically
    /// inside a single RLS-bound transaction.
    async fn create_with_idempotency(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        idempotency: Option<(&str, &str, DateTime<Utc>)>,
    ) -> Result<IdempotentCreateResult, AppError>;

    /// Retrieves a single active (non-deleted) booking with its items.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Booking>, AppError>;

    /// Updates a booking (guarded by optimistic `expected_status`) and optionally replaces all its `booking_items` atomically.
    async fn update(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        expected_status: BookingStatus,
        replace_items: bool,
    ) -> Result<Booking, AppError>;

    /// Soft-deletes a booking (`deleted_at = NOW()`). Returns `true` if a row was updated.
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError>;

    /// Dynamic paginated search with `Criteria` and batch-loaded `booking_items` (no N+1).
    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<BookingSearchPage, AppError>;
}
