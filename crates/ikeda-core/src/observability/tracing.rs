//! Structured tracing initialization for IkedaDev microservices.
//!
//! Emits flat structured JSON to stdout by default (`LOG_FORMAT=json`) with
//! fields compatible with both Grafana Loki (`| json`) and Google Cloud Logging
//! (`level`, `severity`, `timestamp`, `service`, `target`, `message`, and
//! flattened span/event fields). Supports `LOG_FORMAT=pretty` for local
//! development.

use chrono::{SecondsFormat, Utc};
use serde_json::{Map, Value};
use std::fmt;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::{JsonFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Default `RUST_LOG` filter applied when the environment variable is unset or empty.
pub const DEFAULT_RUST_LOG: &str = "info,tower_http=info,sqlx=warn";

/// Supported output formats controlled by the `LOG_FORMAT` environment variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Flat single-line JSON compatible with Grafana Loki and Google Cloud Logging.
    Json,
    /// Human-readable multi-line output for local development.
    Pretty,
}

impl LogFormat {
    /// Parses `LOG_FORMAT` value (case-insensitive). Defaults to [`LogFormat::Json`].
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("pretty") => Self::Pretty,
            _ => Self::Json,
        }
    }
}

/// Resolved tracing configuration for a microservice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfig {
    pub service_name: String,
    pub rust_log: String,
    pub log_format: LogFormat,
}

impl ObservabilityConfig {
    /// Constructs configuration from explicit optional values (useful for deterministic tests).
    pub fn from_values(
        service_name: impl Into<String>,
        rust_log: Option<&str>,
        log_format: Option<&str>,
    ) -> Self {
        let resolved_rust_log = rust_log
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(DEFAULT_RUST_LOG)
            .to_string();

        Self {
            service_name: service_name.into(),
            rust_log: resolved_rust_log,
            log_format: LogFormat::parse(log_format),
        }
    }

    /// Reads `RUST_LOG` and `LOG_FORMAT` from process environment variables.
    pub fn from_env(service_name: impl Into<String>) -> Self {
        let rust_log = std::env::var("RUST_LOG").ok();
        let log_format = std::env::var("LOG_FORMAT").ok();
        Self::from_values(service_name, rust_log.as_deref(), log_format.as_deref())
    }
}

/// Custom flat JSON event formatter compatible with Grafana Loki and Google Cloud Logging.
#[derive(Debug, Clone)]
pub struct StructuredJsonFormat {
    service_name: String,
}

impl StructuredJsonFormat {
    /// Creates a new JSON formatter tagged with the given `service_name`.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}

/// Maps `tracing::Level` to Google Cloud Logging `severity` strings.
fn gcp_severity(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        Level::DEBUG | Level::TRACE => "DEBUG",
    }
}

struct JsonFieldVisitor {
    fields: Map<String, Value>,
    message: Option<String>,
}

impl JsonFieldVisitor {
    fn new() -> Self {
        Self {
            fields: Map::new(),
            message: None,
        }
    }

    fn insert_value(&mut self, name: &str, value: Value) {
        if name == "message" {
            match value {
                Value::String(s) => self.message = Some(s),
                other => self.message = Some(other.to_string()),
            }
        } else {
            self.fields.insert(name.to_string(), value);
        }
    }
}

impl Visit for JsonFieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if let Some(num) = serde_json::Number::from_f64(value) {
            self.insert_value(field.name(), Value::Number(num));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert_value(field.name(), Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert_value(field.name(), Value::Number(value.into()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert_value(field.name(), Value::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert_value(field.name(), Value::String(value.to_string()));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.insert_value(field.name(), Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_value(field.name(), Value::String(format!("{value:?}")));
    }
}

impl<S, N> FormatEvent<S, N> for StructuredJsonFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let level = meta.level();

        let mut payload = Map::new();
        payload.insert(
            "timestamp".to_string(),
            Value::String(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
        );
        payload.insert(
            "level".to_string(),
            Value::String(level.as_str().to_string()),
        );
        payload.insert(
            "severity".to_string(),
            Value::String(gcp_severity(level).to_string()),
        );
        payload.insert(
            "service".to_string(),
            Value::String(self.service_name.clone()),
        );
        payload.insert(
            "target".to_string(),
            Value::String(meta.target().to_string()),
        );

        // Flatten enclosing span fields (root -> leaf so inner spans override outer ones)
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                let extensions = span.extensions();
                if let Some(formatted) = extensions.get::<FormattedFields<N>>() {
                    if let Ok(Value::Object(span_map)) =
                        serde_json::from_str::<Value>(formatted.as_str())
                    {
                        for (k, v) in span_map {
                            payload.insert(k, v);
                        }
                    }
                }
            }
        }

        // Flatten event fields and extract message
        let mut visitor = JsonFieldVisitor::new();
        event.record(&mut visitor);

        for (k, v) in visitor.fields {
            payload.insert(k, v);
        }
        payload.insert(
            "message".to_string(),
            Value::String(visitor.message.unwrap_or_default()),
        );

        match serde_json::to_string(&Value::Object(payload)) {
            Ok(serialized) => writeln!(writer, "{serialized}"),
            Err(_) => Err(fmt::Error),
        }
    }
}

/// Initializes the global `tracing` subscriber for a microservice.
///
/// Reads `RUST_LOG` (defaulting to `"info,tower_http=info,sqlx=warn"`) and
/// `LOG_FORMAT` (`"json"` by default, `"pretty"` for local development).
///
/// Uses `.try_init()` so calling this multiple times (e.g. in tests) never panics.
pub fn init_tracing(service_name: &str) {
    let config = ObservabilityConfig::from_env(service_name);
    let env_filter = EnvFilter::try_new(&config.rust_log)
        .or_else(|_| EnvFilter::try_new(DEFAULT_RUST_LOG))
        .unwrap_or_default();

    match config.log_format {
        LogFormat::Json => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .fmt_fields(JsonFields::new())
                .event_format(StructuredJsonFormat::new(config.service_name));

            let _ = tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .try_init();
        }
        LogFormat::Pretty => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .pretty()
                .with_target(true);

            let _ = tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .try_init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    struct BufferGuard(Arc<Mutex<Vec<u8>>>);

    impl io::Write for BufferGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut guard = self.0.lock().expect("buffer lock");
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for SharedBuffer {
        type Writer = BufferGuard;

        fn make_writer(&'a self) -> Self::Writer {
            BufferGuard(Arc::clone(&self.0))
        }
    }

    #[test]
    fn parses_log_format_defaults_and_variants() {
        assert_eq!(LogFormat::parse(None), LogFormat::Json);
        assert_eq!(LogFormat::parse(Some("")), LogFormat::Json);
        assert_eq!(LogFormat::parse(Some("json")), LogFormat::Json);
        assert_eq!(LogFormat::parse(Some("JSON")), LogFormat::Json);
        assert_eq!(LogFormat::parse(Some("unknown")), LogFormat::Json);
        assert_eq!(LogFormat::parse(Some("pretty")), LogFormat::Pretty);
        assert_eq!(LogFormat::parse(Some("  PRETTY  ")), LogFormat::Pretty);
    }

    #[test]
    fn resolves_observability_config_with_defaults_and_overrides() {
        let default_cfg = ObservabilityConfig::from_values("bookings-api", None, None);
        assert_eq!(default_cfg.service_name, "bookings-api");
        assert_eq!(default_cfg.rust_log, DEFAULT_RUST_LOG);
        assert_eq!(default_cfg.log_format, LogFormat::Json);

        let blank_cfg = ObservabilityConfig::from_values("crm-api", Some("   "), Some("pretty"));
        assert_eq!(blank_cfg.rust_log, DEFAULT_RUST_LOG);
        assert_eq!(blank_cfg.log_format, LogFormat::Pretty);

        let custom_cfg = ObservabilityConfig::from_values(
            "telemetry-api",
            Some("debug,sqlx=error"),
            Some("json"),
        );
        assert_eq!(custom_cfg.rust_log, "debug,sqlx=error");
        assert_eq!(custom_cfg.log_format, LogFormat::Json);
    }

    #[test]
    fn structured_json_format_emits_flat_loki_and_gcp_fields() {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(buffer.clone())
                .fmt_fields(JsonFields::new())
                .event_format(StructuredJsonFormat::new("telemetry-api")),
        );

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "http_request",
                request_id = "req-123",
                tenant_id = "tenant-abc"
            );
            let _enter = span.enter();
            tracing::warn!(status = 404u16, latency_ms = 12u64, "Resource not found");
        });

        let output = String::from_utf8(buffer.0.lock().expect("lock").clone()).expect("utf8");
        let line = output.lines().next().expect("expected one json log line");
        let parsed: Value = serde_json::from_str(line).expect("valid json log line");

        assert_eq!(parsed["service"], "telemetry-api");
        assert_eq!(parsed["level"], "WARN");
        assert_eq!(parsed["severity"], "WARNING");
        assert_eq!(parsed["message"], "Resource not found");
        assert_eq!(parsed["request_id"], "req-123");
        assert_eq!(parsed["tenant_id"], "tenant-abc");
        assert_eq!(parsed["status"], 404);
        assert_eq!(parsed["latency_ms"], 12);
        assert!(parsed["timestamp"].as_str().is_some());
        assert!(parsed["target"].as_str().is_some());
    }

    #[test]
    fn init_tracing_is_idempotent_and_never_panics() {
        init_tracing("test-service");
        init_tracing("test-service");
    }
}
