use super::use_cases::*;
use crate::domain::models::*;
use crate::domain::repositories::{CalendarRepository, EventRepository};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
pub struct MockCalendarRepository {
    pub calendars: Mutex<Vec<Calendar>>,
}

#[async_trait]
impl CalendarRepository for MockCalendarRepository {
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

    async fn update(&self, _tenant_id: Uuid, calendar: &Calendar) -> Result<Calendar, AppError> {
        let mut lock = self.calendars.lock().unwrap();
        if let Some(pos) = lock.iter().position(|c| c.id == calendar.id) {
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
            .find(|c| c.tenant_id == tenant_id && c.id == id && !c.is_deleted())
        {
            c.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Default)]
struct MockEventRepository {
    events: Mutex<Vec<Event>>,
    exceptions: Mutex<Vec<EventException>>,
}

#[async_trait]
impl EventRepository for MockEventRepository {
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

    async fn update(&self, _tenant_id: Uuid, event: &Event) -> Result<Event, AppError> {
        let mut lock = self.events.lock().unwrap();
        if let Some(pos) = lock.iter().position(|e| e.id == event.id) {
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
            .find(|e| e.tenant_id == tenant_id && e.id == id && !e.is_deleted())
        {
            e.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[tokio::test]
async fn test_create_calendar_success_and_validation() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let use_case = CreateCalendarUseCase::new(cal_repo);
    let tenant_id = Uuid::new_v4();

    // Valid creation
    let cmd = CreateCalendarCommand {
        name: "Doctor Consultation Room".to_string(),
        timezone: Some("America/Santiago".to_string()),
        allow_overlaps: Some(false),
        working_hours: None,
    };
    let created = use_case.execute(tenant_id, cmd).await.unwrap();
    assert_eq!(created.name, "Doctor Consultation Room");
    assert_eq!(created.timezone, "America/Santiago");
    assert!(!created.allow_overlaps);

    // Empty name error
    let bad_cmd = CreateCalendarCommand {
        name: "   ".to_string(),
        timezone: None,
        allow_overlaps: None,
        working_hours: None,
    };
    let err = use_case.execute(tenant_id, bad_cmd).await;
    assert!(err.is_err());

    // Invalid timezone error
    let bad_tz_cmd = CreateCalendarCommand {
        name: "Test Room".to_string(),
        timezone: Some("Invalid/Timezone_XYZ".to_string()),
        allow_overlaps: None,
        working_hours: None,
    };
    let tz_err = use_case.execute(tenant_id, bad_tz_cmd).await;
    assert!(tz_err.is_err());
}

#[tokio::test]
async fn test_create_event_overlap_detection_on_strict_calendar() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let event_repo = Arc::new(MockEventRepository::default());
    let tenant_id = Uuid::new_v4();

    // 1. Create a strict calendar (allow_overlaps = false)
    let strict_cal = Calendar::new(tenant_id, "Dentist Chair", "UTC", false, None);
    cal_repo.create(tenant_id, &strict_cal).await.unwrap();

    let use_case = CreateEventUseCase::new(cal_repo.clone(), event_repo.clone());

    let now = Utc::now();
    let slot1_start = now + Duration::hours(1);
    let slot1_end = now + Duration::hours(2);

    // 2. Schedule Event 1 (10:00 - 11:00)
    let cmd1 = CreateEventCommand {
        calendar_id: strict_cal.id,
        title: "Patient A Cleaning".to_string(),
        description: None,
        starts_at: slot1_start,
        ends_at: slot1_end,
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };
    let event1 = use_case.execute(tenant_id, cmd1).await.unwrap();
    assert_eq!(event1.title, "Patient A Cleaning");

    // 3. Try to schedule overlapping Event 2 (10:30 - 11:30)
    let cmd2 = CreateEventCommand {
        calendar_id: strict_cal.id,
        title: "Patient B Root Canal".to_string(),
        description: None,
        starts_at: slot1_start + Duration::minutes(30),
        ends_at: slot1_end + Duration::minutes(30),
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };
    let conflict_err = use_case.execute(tenant_id, cmd2).await;
    assert!(conflict_err.is_err());
    match conflict_err.unwrap_err() {
        AppError::Conflict(msg) => assert!(msg.contains("conflicts")),
        other => panic!("Expected Conflict, got {:?}", other),
    }

    // 4. Scheduling non-overlapping Event 3 (11:00 - 12:00) succeeds
    let cmd3 = CreateEventCommand {
        calendar_id: strict_cal.id,
        title: "Patient C Checkup".to_string(),
        description: None,
        starts_at: slot1_end,
        ends_at: slot1_end + Duration::hours(1),
        timezone: None,
        rrule: None,
        status: None,
        external_resource_id: None,
        metadata: None,
    };
    let event3 = use_case.execute(tenant_id, cmd3).await;
    assert!(event3.is_ok());
}

#[tokio::test]
async fn test_check_availability_use_case() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let event_repo = Arc::new(MockEventRepository::default());
    let tenant_id = Uuid::new_v4();

    let cal = Calendar::new(tenant_id, "Meeting Room 1", "UTC", false, None);
    cal_repo.create(tenant_id, &cal).await.unwrap();

    let now = Utc::now();
    let query_range = TimeRange::new(now, now + Duration::hours(3)).unwrap();

    // Schedule 1 hour busy from now+1h to now+2h
    let busy_event = Event::new(
        tenant_id,
        cal.id,
        "Reserved",
        None,
        TimeRange::new(now + Duration::hours(1), now + Duration::hours(2)).unwrap(),
        "UTC",
        None,
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo.create(tenant_id, &busy_event).await.unwrap();

    let availability_use_case = CheckAvailabilityUseCase::new(cal_repo.clone(), event_repo.clone());
    let result = availability_use_case
        .execute(
            tenant_id,
            CheckAvailabilityQuery {
                calendar_id: cal.id,
                range: query_range,
                slot_duration_minutes: Some(60),
            },
        )
        .await
        .unwrap();

    assert_eq!(result.busy_slots.len(), 1);
    assert_eq!(result.available_slots.len(), 2); // 0..1 and 2..3
}

#[tokio::test]
async fn test_update_calendar_use_case() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let tenant_id = Uuid::new_v4();

    let cal = Calendar::new(tenant_id, "Old Name", "UTC", true, None);
    cal_repo.create(tenant_id, &cal).await.unwrap();

    let use_case = UpdateCalendarUseCase::new(cal_repo.clone());
    let cmd = UpdateCalendarCommand {
        name: Some("New Name".to_string()),
        timezone: Some("America/Santiago".to_string()),
        allow_overlaps: Some(false),
        working_hours: Some(serde_json::json!({"monday": {"start": "09:00", "end": "17:00"}})),
    };

    let updated = use_case.execute(tenant_id, cal.id, cmd).await.unwrap();
    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.timezone, "America/Santiago");
    assert!(!updated.allow_overlaps);

    // Invalid timezone returns BadRequest
    let bad_cmd = UpdateCalendarCommand {
        name: None,
        timezone: Some("Invalid/Timezone".to_string()),
        allow_overlaps: None,
        working_hours: None,
    };
    let bad_res = use_case.execute(tenant_id, cal.id, bad_cmd).await;
    assert!(bad_res.is_err());
}

#[tokio::test]
async fn test_delete_calendar_use_case() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let tenant_id = Uuid::new_v4();

    let cal = Calendar::new(tenant_id, "To Delete", "UTC", true, None);
    cal_repo.create(tenant_id, &cal).await.unwrap();

    let use_case = DeleteCalendarUseCase::new(cal_repo.clone());
    let res = use_case.execute(tenant_id, cal.id).await;
    assert!(res.is_ok());

    let found = cal_repo.find_by_id(tenant_id, cal.id).await.unwrap();
    assert!(found.is_none());

    // Deleting again returns NotFound
    let res2 = use_case.execute(tenant_id, cal.id).await;
    assert!(res2.is_err());
}

#[tokio::test]
async fn test_update_event_use_case_and_self_exclusion() {
    let cal_repo = Arc::new(MockCalendarRepository::default());
    let event_repo = Arc::new(MockEventRepository::default());
    let tenant_id = Uuid::new_v4();

    let strict_cal = Calendar::new(tenant_id, "Strict Room", "UTC", false, None);
    cal_repo.create(tenant_id, &strict_cal).await.unwrap();

    let now = Utc::now();
    let event1 = Event::new(
        tenant_id,
        strict_cal.id,
        "Event 1",
        None,
        TimeRange::new(now + Duration::hours(1), now + Duration::hours(2)).unwrap(),
        "UTC",
        None,
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo.create(tenant_id, &event1).await.unwrap();

    let event2 = Event::new(
        tenant_id,
        strict_cal.id,
        "Event 2",
        None,
        TimeRange::new(now + Duration::hours(2), now + Duration::hours(3)).unwrap(),
        "UTC",
        None,
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo.create(tenant_id, &event2).await.unwrap();

    let use_case = UpdateEventUseCase::new(cal_repo.clone(), event_repo.clone());

    // Updating event 1 title & staying in same time range MUST succeed (not conflict with itself)
    let cmd_ok = UpdateEventCommand {
        title: Some("Event 1 Renamed".to_string()),
        description: Some("New description".to_string()),
        starts_at: Some(now + Duration::hours(1)),
        ends_at: Some(now + Duration::hours(2)),
        status: None,
        metadata: None,
    };
    let updated = use_case
        .execute(tenant_id, event1.id, cmd_ok)
        .await
        .unwrap();
    assert_eq!(updated.title, "Event 1 Renamed");

    // Updating event 1 to collide with event 2 (1:30 - 2:30) MUST fail with 409 Conflict
    let cmd_conflict = UpdateEventCommand {
        title: None,
        description: None,
        starts_at: Some(now + Duration::hours(1) + Duration::minutes(30)),
        ends_at: Some(now + Duration::hours(2) + Duration::minutes(30)),
        status: None,
        metadata: None,
    };
    let conflict_res = use_case.execute(tenant_id, event1.id, cmd_conflict).await;
    match conflict_res.unwrap_err() {
        AppError::Conflict(msg) => assert!(msg.contains("overlaps")),
        other => panic!("Expected Conflict, got {:?}", other),
    }
}

#[tokio::test]
async fn test_delete_event_use_case() {
    let event_repo = Arc::new(MockEventRepository::default());
    let tenant_id = Uuid::new_v4();

    let now = Utc::now();
    let event = Event::new(
        tenant_id,
        Uuid::new_v4(),
        "To Delete",
        None,
        TimeRange::new(now, now + Duration::hours(1)).unwrap(),
        "UTC",
        None,
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo.create(tenant_id, &event).await.unwrap();

    let use_case = DeleteEventUseCase::new(event_repo.clone());
    let res = use_case.execute(tenant_id, event.id).await;
    assert!(res.is_ok());

    let found = event_repo.find_by_id(tenant_id, event.id).await.unwrap();
    assert!(found.is_none());

    let res2 = use_case.execute(tenant_id, event.id).await;
    assert!(res2.is_err());
}

#[tokio::test]
async fn test_create_event_exception_use_case() {
    let event_repo = Arc::new(MockEventRepository::default());
    let tenant_id = Uuid::new_v4();

    let now = Utc::now();
    // Recurring event with RRULE
    let recurring_event = Event::new(
        tenant_id,
        Uuid::new_v4(),
        "Daily Standup",
        None,
        TimeRange::new(now, now + Duration::minutes(30)).unwrap(),
        "UTC",
        Some("FREQ=DAILY;COUNT=5".to_string()),
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo
        .create(tenant_id, &recurring_event)
        .await
        .unwrap();

    let use_case = CreateEventExceptionUseCase::new(event_repo.clone());

    // 1. Cancel an occurrence
    let cancel_cmd = CreateEventExceptionCommand {
        original_starts_at: now + Duration::days(1),
        is_cancelled: true,
        rescheduled_starts_at: None,
        rescheduled_ends_at: None,
    };
    let cancel_ex = use_case
        .execute(tenant_id, recurring_event.id, cancel_cmd)
        .await
        .unwrap();
    assert!(cancel_ex.is_cancelled);
    assert_eq!(cancel_ex.event_id, recurring_event.id);

    // 2. Reschedule an occurrence
    let reschedule_cmd = CreateEventExceptionCommand {
        original_starts_at: now + Duration::days(2),
        is_cancelled: false,
        rescheduled_starts_at: Some(now + Duration::days(2) + Duration::hours(1)),
        rescheduled_ends_at: Some(now + Duration::days(2) + Duration::hours(2)),
    };
    let reschedule_ex = use_case
        .execute(tenant_id, recurring_event.id, reschedule_cmd)
        .await
        .unwrap();
    assert!(!reschedule_ex.is_cancelled);
    assert!(reschedule_ex.rescheduled_time_range.is_some());

    // 3. Exception on non-recurring event fails with BadRequest
    let non_recurring = Event::new(
        tenant_id,
        Uuid::new_v4(),
        "One-off Event",
        None,
        TimeRange::new(now, now + Duration::hours(1)).unwrap(),
        "UTC",
        None,
        EventStatus::Confirmed,
        None,
        serde_json::json!({}),
    );
    event_repo.create(tenant_id, &non_recurring).await.unwrap();

    let fail_cmd = CreateEventExceptionCommand {
        original_starts_at: now,
        is_cancelled: true,
        rescheduled_starts_at: None,
        rescheduled_ends_at: None,
    };
    let fail_res = use_case
        .execute(tenant_id, non_recurring.id, fail_cmd)
        .await;
    assert!(fail_res.is_err());
}
