use chrono::{DateTime, Utc};
use ikeda_core::error::{AppError, ValidationErrorDetail};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Maximum number of events accepted in a single batch ingestion request.
pub const MAX_BATCH_SIZE: usize = 100;
/// Maximum length for `event_name`.
pub const MAX_EVENT_NAME_LEN: usize = 120;
/// Maximum length for `source`.
pub const MAX_SOURCE_LEN: usize = 50;
/// Default `source` when not specified by the caller.
pub const DEFAULT_SOURCE: &str = "web";
/// Maximum length for `anonymous_id`, `session_id`, and `request_id`.
pub const MAX_IDENTIFIER_LEN: usize = 128;
/// Maximum length for `resource_type`.
pub const MAX_RESOURCE_TYPE_LEN: usize = 64;
/// Default number of top event names returned by metrics summary.
pub const DEFAULT_METRICS_TOP_LIMIT: i64 = 20;
/// Maximum number of top event names returned by metrics summary.
pub const MAX_METRICS_TOP_LIMIT: i64 = 100;

/// Classification of a telemetry event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventKind {
    Tracking,
    Audit,
}

impl EventKind {
    /// Returns the uppercase database/wire representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tracking => "TRACKING",
            Self::Audit => "AUDIT",
        }
    }

    /// Parses a string representation into an `EventKind`.
    pub fn parse_str(raw: &str) -> Result<Self, AppError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "TRACKING" => Ok(Self::Tracking),
            "AUDIT" => Ok(Self::Audit),
            other => Err(AppError::InternalServerError(format!(
                "Invalid telemetry event kind stored in database: '{other}'"
            ))),
        }
    }
}

/// Immutable telemetry event entity stored in the append-only `telemetry_events` log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TelemetryEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: EventKind,
    pub event_name: String,
    pub source: String,
    pub client_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub anonymous_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub properties: serde_json::Value,
    pub context: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

/// Unvalidated input parameters used to construct and validate a [`TelemetryEvent`].
#[derive(Debug, Clone)]
pub struct NewTelemetryEvent {
    pub tenant_id: Uuid,
    pub kind: EventKind,
    pub event_name: String,
    pub source: Option<String>,
    pub client_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub anonymous_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub occurred_at: Option<DateTime<Utc>>,
}

impl TelemetryEvent {
    /// Validates domain invariants and builds a new immutable [`TelemetryEvent`].
    pub fn try_new(input: NewTelemetryEvent) -> Result<Self, AppError> {
        let mut errors = Vec::new();
        let event = Self::build_with_prefix(input, None, &mut errors);
        if !errors.is_empty() {
            return Err(AppError::validation(
                "Telemetry event validation failed",
                errors,
            ));
        }
        event.ok_or_else(|| {
            AppError::InternalServerError(
                "Failed to construct validated telemetry event".to_string(),
            )
        })
    }

    /// Validates a batch of events (1..=100) and builds immutable [`TelemetryEvent`] instances.
    pub fn try_new_batch(inputs: Vec<NewTelemetryEvent>) -> Result<Vec<Self>, AppError> {
        if inputs.is_empty() {
            return Err(AppError::validation(
                "Batch must contain at least 1 event",
                vec![
                    ValidationErrorDetail::new("events", "At least one event is required")
                        .with_code("batch_empty"),
                ],
            ));
        }
        if inputs.len() > MAX_BATCH_SIZE {
            return Err(AppError::validation(
                format!("Batch exceeds maximum size of {MAX_BATCH_SIZE} events"),
                vec![ValidationErrorDetail::new(
                    "events",
                    format!(
                        "Received {} events; maximum allowed per batch is {MAX_BATCH_SIZE}",
                        inputs.len()
                    ),
                )
                .with_code("batch_too_large")],
            ));
        }

        let mut errors = Vec::new();
        let mut built = Vec::with_capacity(inputs.len());
        for (idx, input) in inputs.into_iter().enumerate() {
            let prefix = format!("events[{idx}]");
            if let Some(ev) = Self::build_with_prefix(input, Some(&prefix), &mut errors) {
                built.push(ev);
            }
        }

        if !errors.is_empty() {
            return Err(AppError::validation(
                "One or more events in batch failed validation",
                errors,
            ));
        }

        Ok(built)
    }

    fn build_with_prefix(
        input: NewTelemetryEvent,
        prefix: Option<&str>,
        errors: &mut Vec<ValidationErrorDetail>,
    ) -> Option<Self> {
        let initial_err_count = errors.len();
        let field_name = |name: &str| match prefix {
            Some(p) => format!("{p}.{name}"),
            None => name.to_string(),
        };

        let event_name = input.event_name.trim().to_string();
        if !is_valid_event_name(&event_name) {
            errors.push(
                ValidationErrorDetail::new(
                    field_name("event_name"),
                    format!(
                        "Must be non-empty, <= {MAX_EVENT_NAME_LEN} chars, and use lowercase 'namespace.action' format (e.g. 'page.viewed', 'booking.created')"
                    ),
                )
                .with_code("invalid_event_name"),
            );
        }

        let source = input
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_SOURCE)
            .to_string();
        if source.len() > MAX_SOURCE_LEN {
            errors.push(
                ValidationErrorDetail::new(
                    field_name("source"),
                    format!("Must be at most {MAX_SOURCE_LEN} characters"),
                )
                .with_code("max_length"),
            );
        }

        let anonymous_id = normalize_optional_str(
            input.anonymous_id,
            MAX_IDENTIFIER_LEN,
            &field_name("anonymous_id"),
            errors,
        );
        let session_id = normalize_optional_str(
            input.session_id,
            MAX_IDENTIFIER_LEN,
            &field_name("session_id"),
            errors,
        );
        let resource_type = normalize_optional_str(
            input.resource_type,
            MAX_RESOURCE_TYPE_LEN,
            &field_name("resource_type"),
            errors,
        );
        let request_id = normalize_optional_str(
            input.request_id,
            MAX_IDENTIFIER_LEN,
            &field_name("request_id"),
            errors,
        );

        let properties = input
            .properties
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        if !properties.is_object() {
            errors.push(
                ValidationErrorDetail::new(field_name("properties"), "Must be a JSON object")
                    .with_code("invalid_json_object"),
            );
        }

        let context = input
            .context
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        if !context.is_object() {
            errors.push(
                ValidationErrorDetail::new(field_name("context"), "Must be a JSON object")
                    .with_code("invalid_json_object"),
            );
        }

        if errors.len() > initial_err_count {
            return None;
        }

        let received_at = Utc::now();
        let occurred_at = input.occurred_at.unwrap_or(received_at);

        Some(Self {
            id: Uuid::now_v7(),
            tenant_id: input.tenant_id,
            kind: input.kind,
            event_name,
            source,
            client_id: input.client_id,
            actor_id: input.actor_id,
            anonymous_id,
            session_id,
            resource_type,
            resource_id: input.resource_id,
            request_id,
            properties,
            context,
            occurred_at,
            received_at,
        })
    }
}

/// Validates `namespace.action` format:
/// - non-empty, length `<= 120`
/// - at least 2 dot-separated segments
/// - each segment is non-empty, starts with `[a-z0-9]`, and contains only `[a-z0-9_-]`
#[must_use]
pub fn is_valid_event_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_EVENT_NAME_LEN {
        return false;
    }
    let mut segment_count = 0usize;
    for segment in name.split('.') {
        if segment.is_empty() {
            return false;
        }
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
            return false;
        }
        if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return false;
        }
        segment_count += 1;
    }
    segment_count >= 2
}

fn normalize_optional_str(
    raw: Option<String>,
    max_len: usize,
    field: &str,
    errors: &mut Vec<ValidationErrorDetail>,
) -> Option<String> {
    let trimmed = raw
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    if trimmed.len() > max_len {
        errors.push(
            ValidationErrorDetail::new(field, format!("Must be at most {max_len} characters"))
                .with_code("max_length"),
        );
        return None;
    }
    Some(trimmed)
}

/// Parameters for aggregating telemetry metrics over a time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub kind: Option<EventKind>,
    pub source: Option<String>,
    pub top_limit: i64,
}

impl MetricsQuery {
    /// Validates the date range and optional filters.
    pub fn try_new(
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        kind: Option<EventKind>,
        source: Option<String>,
        top_limit: Option<i64>,
    ) -> Result<Self, AppError> {
        let mut errors = Vec::new();
        if to < from {
            errors.push(
                ValidationErrorDetail::new(
                    "to",
                    "'to' timestamp must be greater than or equal to 'from'",
                )
                .with_code("invalid_date_range"),
            );
        }

        let normalized_source =
            normalize_optional_str(source, MAX_SOURCE_LEN, "source", &mut errors);

        let limit = top_limit
            .unwrap_or(DEFAULT_METRICS_TOP_LIMIT)
            .clamp(1, MAX_METRICS_TOP_LIMIT);

        if !errors.is_empty() {
            return Err(AppError::validation("Invalid metrics query", errors));
        }

        Ok(Self {
            from,
            to,
            kind,
            source: normalized_source,
            top_limit: limit,
        })
    }
}

/// Count of occurrences for a single `event_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventNameCount {
    pub event_name: String,
    pub count: u64,
}

/// Aggregated telemetry metrics over a requested `[from, to]` window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventMetrics {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub total_events: u64,
    pub unique_clients: u64,
    pub unique_sessions: u64,
    pub by_event_name: Vec<EventNameCount>,
}

/// Parameters for fetching a client's paginated telemetry timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientTimelineQuery {
    pub client_id: Uuid,
    pub kind: Option<EventKind>,
    pub page: u64,
    pub limit: u64,
}

impl ClientTimelineQuery {
    #[must_use]
    pub fn new(
        client_id: Uuid,
        kind: Option<EventKind>,
        page: Option<u64>,
        limit: Option<u64>,
    ) -> Self {
        Self {
            client_id,
            kind,
            page: page.unwrap_or(1).max(1),
            limit: limit.unwrap_or(20).clamp(1, 100),
        }
    }
}
