//! Shared observability primitives for `ikedadev-platform` microservices.
//!
//! Provides structured JSON/pretty logging compatible with Grafana Loki
//! (LogQL `| json`) and Google Cloud Logging, `X-Request-Id` generation and
//! propagation, and safe HTTP request/response tracing layers.

pub mod http;
pub mod tracing;

pub use http::{
    http_trace_layer, request_id_layer, HttpMakeSpan, HttpOnResponse, HttpTraceLayer,
    RequestIdLayer, REQUEST_ID_HEADER,
};
pub use tracing::{
    init_tracing, LogFormat, ObservabilityConfig, StructuredJsonFormat, DEFAULT_RUST_LOG,
};
