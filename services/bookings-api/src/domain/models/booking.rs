use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Lifecycle state of a booking with strict state-machine transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BookingStatus {
    Pending,
    Scheduled,
    InProgress,
    Completed,
    Cancelled,
    NoShow,
}

impl BookingStatus {
    /// Returns the canonical database string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Scheduled => "SCHEDULED",
            Self::InProgress => "IN_PROGRESS",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::NoShow => "NO_SHOW",
        }
    }

    /// Parses a database status string into `BookingStatus`.
    pub fn parse_str(value: &str) -> Result<Self, AppError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "SCHEDULED" => Ok(Self::Scheduled),
            "IN_PROGRESS" => Ok(Self::InProgress),
            "COMPLETED" => Ok(Self::Completed),
            "CANCELLED" => Ok(Self::Cancelled),
            "NO_SHOW" => Ok(Self::NoShow),
            other => Err(AppError::DatabaseError(format!(
                "Unknown booking status in storage: '{other}'"
            ))),
        }
    }

    /// Returns `true` if this status is terminal (`COMPLETED`, `CANCELLED`, or `NO_SHOW`).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::NoShow)
    }

    /// Enforces the strict booking state machine:
    /// - `PENDING` -> `SCHEDULED`, `CANCELLED`
    /// - `SCHEDULED` -> `IN_PROGRESS`, `COMPLETED`, `CANCELLED`, `NO_SHOW`
    /// - `IN_PROGRESS` -> `COMPLETED`, `CANCELLED`, `NO_SHOW`
    /// - `COMPLETED`, `CANCELLED`, `NO_SHOW` -> terminal (no transitions allowed)
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Scheduled | Self::Cancelled)
                | (
                    Self::Scheduled,
                    Self::InProgress | Self::Completed | Self::Cancelled | Self::NoShow
                )
                | (
                    Self::InProgress,
                    Self::Completed | Self::Cancelled | Self::NoShow
                )
        )
    }
}

/// Frozen commercial line item associated with a `Booking`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BookingItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub booking_id: Uuid,
    pub catalog_item_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<Uuid>,
    pub name_snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_snapshot: Option<String>,
    pub quantity: i32,
    pub unit_price_snapshot: i64,
    pub tax_rate_bps: i32,
    pub tax_included: bool,
    pub line_subtotal: i64,
    pub line_tax: i64,
    pub line_total: i64,
    pub duration_minutes_snapshot: i32,
    pub buffer_minutes_snapshot: i32,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Multi-tenant, domain-agnostic booking aggregate root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Booking {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub booking_code: String,
    pub client_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub timezone: String,
    pub status: BookingStatus,
    pub subtotal: i64,
    pub tax_total: i64,
    pub total_price: i64,
    pub total_duration_minutes: i32,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<Uuid>,
    pub items: Vec<BookingItem>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::BookingStatus;

    #[test]
    fn test_booking_status_state_machine_transitions() {
        // PENDING
        assert!(BookingStatus::Pending.can_transition_to(BookingStatus::Scheduled));
        assert!(BookingStatus::Pending.can_transition_to(BookingStatus::Cancelled));
        assert!(!BookingStatus::Pending.can_transition_to(BookingStatus::InProgress));
        assert!(!BookingStatus::Pending.can_transition_to(BookingStatus::Completed));
        assert!(!BookingStatus::Pending.can_transition_to(BookingStatus::NoShow));
        assert!(!BookingStatus::Pending.can_transition_to(BookingStatus::Pending));

        // SCHEDULED
        assert!(BookingStatus::Scheduled.can_transition_to(BookingStatus::InProgress));
        assert!(BookingStatus::Scheduled.can_transition_to(BookingStatus::Completed));
        assert!(BookingStatus::Scheduled.can_transition_to(BookingStatus::Cancelled));
        assert!(BookingStatus::Scheduled.can_transition_to(BookingStatus::NoShow));
        assert!(!BookingStatus::Scheduled.can_transition_to(BookingStatus::Pending));
        assert!(!BookingStatus::Scheduled.can_transition_to(BookingStatus::Scheduled));

        // IN_PROGRESS
        assert!(BookingStatus::InProgress.can_transition_to(BookingStatus::Completed));
        assert!(BookingStatus::InProgress.can_transition_to(BookingStatus::Cancelled));
        assert!(BookingStatus::InProgress.can_transition_to(BookingStatus::NoShow));
        assert!(!BookingStatus::InProgress.can_transition_to(BookingStatus::Scheduled));
        assert!(!BookingStatus::InProgress.can_transition_to(BookingStatus::Pending));

        // TERMINAL STATES
        let all = [
            BookingStatus::Pending,
            BookingStatus::Scheduled,
            BookingStatus::InProgress,
            BookingStatus::Completed,
            BookingStatus::Cancelled,
            BookingStatus::NoShow,
        ];
        for terminal in [
            BookingStatus::Completed,
            BookingStatus::Cancelled,
            BookingStatus::NoShow,
        ] {
            assert!(terminal.is_terminal());
            for target in all {
                assert!(!terminal.can_transition_to(target));
            }
        }
    }
}
