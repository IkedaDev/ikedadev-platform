pub mod availability;
pub mod overlap;
pub mod recurrence;
pub mod scheduler;

pub use availability::{calculate_free_slots, merge_busy_intervals, MAX_AVAILABILITY_SLOTS};
pub use overlap::{detect_overlaps, find_overlapping};
pub use recurrence::{expand_recurrence, normalize_to_second, parse_rrule_set};
pub use scheduler::{NotificationScheduler, WebhookEventPayload};
