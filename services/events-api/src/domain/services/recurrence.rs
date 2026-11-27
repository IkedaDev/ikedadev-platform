use crate::domain::models::{Event, EventException, EventInstance, EventStatus, TimeRange};
use chrono::{DateTime, Timelike, Utc};
use ikeda_core::error::AppError;
use rrule::{RRuleSet, Tz};
use std::collections::{HashMap, HashSet};

/// Truncates sub-second precision so `DTSTART` and exception `original_starts_at` match reliably.
#[must_use]
pub fn normalize_to_second(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.with_nanosecond(0).unwrap_or(dt)
}

/// Validates and parses an RFC 5545 recurrence rule anchored at `starts_at`.
pub fn parse_rrule_set(starts_at: DateTime<Utc>, rrule_str: &str) -> Result<RRuleSet, AppError> {
    let trimmed = rrule_str.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "Recurrence rule cannot be empty".to_string(),
        ));
    }

    let formatted_dtstart = normalize_to_second(starts_at)
        .format("%Y%m%dT%H%M%SZ")
        .to_string();

    let rrule_input = if trimmed.to_uppercase().contains("DTSTART") {
        trimmed.to_string()
    } else {
        let clean_rule = trimmed
            .strip_prefix("RRULE:")
            .or_else(|| trimmed.strip_prefix("rrule:"))
            .unwrap_or(trimmed);
        format!("DTSTART:{formatted_dtstart}\nRRULE:{clean_rule}")
    };

    rrule_input
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Failed to parse RFC 5545 recurrence rule: {e}")))
}

/// Expands an event (either single or recurring RFC 5545) into concrete `EventInstance`s
/// within the specified `query_range`, applying any registered exceptions.
pub fn expand_recurrence(
    event: &Event,
    query_range: &TimeRange,
    exceptions: &[EventException],
) -> Result<Vec<EventInstance>, AppError> {
    if event.status == EventStatus::Cancelled {
        return Ok(Vec::new());
    }

    // 1. If not recurring, check if base event overlaps with query range
    if !event.is_recurring() {
        if event.time_range.overlaps(query_range) {
            return Ok(vec![EventInstance::new(
                event.id,
                event.calendar_id,
                &event.title,
                event.time_range,
                event.status,
                false,
                false,
            )]);
        }
        return Ok(Vec::new());
    }

    // 2. Parse RFC 5545 RRULE
    let rrule_str = event
        .rrule
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("Event has no recurrence rule defined".to_string()))?;

    let event_duration = event.time_range.duration();
    let rrule_set = parse_rrule_set(event.time_range.starts_at, rrule_str)?;

    // Build map of exceptions keyed by whole-second normalized original_starts_at
    let mut exception_map: HashMap<DateTime<Utc>, &EventException> = HashMap::new();
    for ex in exceptions {
        exception_map.insert(normalize_to_second(ex.original_starts_at), ex);
    }

    // Window recurrence expansion to [query_range.starts_at - event_duration, query_range.ends_at]
    // so long-running recurring events (e.g. daily > 500 days old) are never silently truncated.
    let window_start = (query_range.starts_at - event_duration).with_timezone(&Tz::UTC);
    let window_end = query_range.ends_at.with_timezone(&Tz::UTC);
    let occurrences = rrule_set.after(window_start).before(window_end).all(500);

    let mut instances = Vec::new();
    let mut visited_originals: HashSet<DateTime<Utc>> = HashSet::new();

    for occ in occurrences.dates {
        let occ_utc: DateTime<Utc> = normalize_to_second(occ.with_timezone(&Utc));
        visited_originals.insert(occ_utc);

        // Check if there is an exception for this occurrence
        if let Some(exception) = exception_map.get(&occ_utc) {
            if exception.is_cancelled {
                continue; // Cancelled instance: omit from results
            }

            if let Some(rescheduled) = exception.rescheduled_time_range {
                if rescheduled.overlaps(query_range) {
                    instances.push(EventInstance::new(
                        event.id,
                        event.calendar_id,
                        &event.title,
                        rescheduled,
                        event.status,
                        true,
                        true,
                    ));
                }
                continue;
            }
        }

        let occ_end = occ_utc + event_duration;
        let instance_range = TimeRange::new(occ_utc, occ_end)?;

        if instance_range.overlaps(query_range) {
            instances.push(EventInstance::new(
                event.id,
                event.calendar_id,
                &event.title,
                instance_range,
                event.status,
                true,
                false,
            ));
        }
    }

    // Include any rescheduled exceptions whose original occurrence fell outside the query window
    // but whose rescheduled time range was moved INTO `query_range`.
    for ex in exceptions {
        if ex.is_cancelled {
            continue;
        }
        let orig_key = normalize_to_second(ex.original_starts_at);
        if visited_originals.contains(&orig_key) {
            continue;
        }
        if let Some(rescheduled) = ex.rescheduled_time_range {
            if rescheduled.overlaps(query_range) {
                instances.push(EventInstance::new(
                    event.id,
                    event.calendar_id,
                    &event.title,
                    rescheduled,
                    event.status,
                    true,
                    true,
                ));
            }
        }
    }

    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::TimeRange;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn test_expand_weekly_recurrence() {
        let tenant_id = Uuid::new_v4();
        let cal_id = Uuid::new_v4();

        let dt_start = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
        let dt_end = Utc.with_ymd_and_hms(2026, 11, 1, 11, 0, 0).unwrap();
        let time_range = TimeRange::new(dt_start, dt_end).unwrap();

        let event = Event::new(
            tenant_id,
            cal_id,
            "Weekly Sync",
            None,
            time_range,
            "UTC",
            Some("FREQ=WEEKLY;INTERVAL=1;COUNT=4".into()),
            EventStatus::Confirmed,
            None,
            serde_json::json!({}),
        );

        let query_range = TimeRange::new(
            Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 30, 23, 59, 59).unwrap(),
        )
        .unwrap();

        let instances = expand_recurrence(&event, &query_range, &[]).expect("expansion failed");
        assert_eq!(instances.len(), 4);
    }

    #[test]
    fn test_expand_with_cancellation_exception() {
        let tenant_id = Uuid::new_v4();
        let cal_id = Uuid::new_v4();

        let dt_start = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
        let dt_end = Utc.with_ymd_and_hms(2026, 11, 1, 11, 0, 0).unwrap();
        let time_range = TimeRange::new(dt_start, dt_end).unwrap();

        let event = Event::new(
            tenant_id,
            cal_id,
            "Weekly Standup",
            None,
            time_range,
            "UTC",
            Some("FREQ=WEEKLY;INTERVAL=1;COUNT=3".into()),
            EventStatus::Confirmed,
            None,
            serde_json::json!({}),
        );

        // Cancel the 2nd instance on 2026-11-08
        let cancelled_dt = Utc.with_ymd_and_hms(2026, 11, 8, 10, 0, 0).unwrap();
        let exception = EventException::cancel(tenant_id, event.id, cancelled_dt);

        let query_range = TimeRange::new(
            Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 30, 0, 0, 0).unwrap(),
        )
        .unwrap();

        let instances =
            expand_recurrence(&event, &query_range, &[exception]).expect("expansion failed");
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn test_daily_recurrence_beyond_500_days_still_expands_in_window() {
        let tenant_id = Uuid::new_v4();
        let cal_id = Uuid::new_v4();

        // Started 600 days ago
        let dt_start = Utc.with_ymd_and_hms(2025, 1, 1, 9, 0, 0).unwrap();
        let dt_end = Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap();
        let event = Event::new(
            tenant_id,
            cal_id,
            "Long Running Daily",
            None,
            TimeRange::new(dt_start, dt_end).unwrap(),
            "UTC",
            Some("FREQ=DAILY".into()),
            EventStatus::Confirmed,
            None,
            serde_json::json!({}),
        );

        // Query 3 days in Oct 2026 (> 600 occurrences after DTSTART)
        let query_range = TimeRange::new(
            Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 4, 0, 0, 0).unwrap(),
        )
        .unwrap();

        let instances = expand_recurrence(&event, &query_range, &[]).expect("expansion failed");
        assert_eq!(instances.len(), 3);
    }

    #[test]
    fn test_exception_matches_when_timestamps_have_subsecond_precision() {
        let tenant_id = Uuid::new_v4();
        let cal_id = Uuid::new_v4();

        let dt_start = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap()
            + chrono::Duration::milliseconds(500);
        let dt_end = dt_start + chrono::Duration::hours(1);
        let event = Event::new(
            tenant_id,
            cal_id,
            "Subsecond Weekly",
            None,
            TimeRange::new(dt_start, dt_end).unwrap(),
            "UTC",
            Some("FREQ=WEEKLY;COUNT=3".into()),
            EventStatus::Confirmed,
            None,
            serde_json::json!({}),
        );

        let cancelled_dt = dt_start + chrono::Duration::days(7);
        let exception = EventException::cancel(tenant_id, event.id, cancelled_dt);

        let query_range = TimeRange::new(
            Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 30, 0, 0, 0).unwrap(),
        )
        .unwrap();

        let instances =
            expand_recurrence(&event, &query_range, &[exception]).expect("expansion failed");
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn test_invalid_rrule_rejected_by_parser() {
        let dt_start = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
        assert!(matches!(
            parse_rrule_set(dt_start, "NOT_A_VALID_RRULE"),
            Err(AppError::BadRequest(_))
        ));
    }
}
