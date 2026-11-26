//! HTTP observability middleware: `X-Request-Id` propagation and safe HTTP tracing.
//!
//! When wiring into an Axum router, apply `request_id_layer()` outermost so
//! `http_trace_layer()` sees the generated or propagated `X-Request-Id`:
//!
//! ```ignore
//! let app = router
//!     .layer(http_trace_layer("bookings-api"))
//!     .layer(request_id_layer());
//! ```

use crate::tenant::{TenantContext, HEADER_TENANT_ID};
use http::{HeaderMap, HeaderName, Request, Response};
use std::time::Duration;
use tower::layer::util::Stack;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnBodyChunk, DefaultOnEos, MakeSpan, OnResponse, TraceLayer};
use tracing::Span;

/// Canonical HTTP header used to correlate requests across services.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Concrete Tower layer type combining `SetRequestIdLayer` and `PropagateRequestIdLayer`.
pub type RequestIdLayer = Stack<PropagateRequestIdLayer, SetRequestIdLayer<MakeRequestUuid>>;

/// Concrete Tower layer type for structured HTTP request/response tracing.
pub type HttpTraceLayer = TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    HttpMakeSpan,
    (),
    HttpOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    (),
>;

/// Builds a Tower layer that generates a UUID v4 `X-Request-Id` when missing,
/// preserves any incoming `X-Request-Id`, and echoes it back on the response.
///
/// Apply this layer **outside** [`http_trace_layer`] in Axum (i.e. call
/// `.layer(http_trace_layer(...)).layer(request_id_layer())`).
pub fn request_id_layer() -> RequestIdLayer {
    let header = HeaderName::from_static(REQUEST_ID_HEADER);
    Stack::new(
        PropagateRequestIdLayer::new(header.clone()),
        SetRequestIdLayer::new(header, MakeRequestUuid),
    )
}

/// Span builder for [`http_trace_layer`].
///
/// Captures `service`, `request_id`, `method`, `path`, and `tenant_id` without
/// ever reading or logging sensitive headers (`Authorization`, `X-Api-Key`, `Cookie`).
#[derive(Debug, Clone, Copy)]
pub struct HttpMakeSpan {
    service_name: &'static str,
}

impl<B> MakeSpan<B> for HttpMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let request_id = extract_header(request.headers(), REQUEST_ID_HEADER).unwrap_or("-");
        let tenant_id = extract_tenant_id(request);
        let method = request.method().as_str();
        let path = request.uri().path();

        tracing::info_span!(
            "http_request",
            service = self.service_name,
            request_id = %request_id,
            method = %method,
            path = %path,
            tenant_id = %tenant_id,
        )
    }
}

/// Response logger for [`http_trace_layer`].
///
/// Emits `service`, `status`, and `latency_ms` inside the active `http_request`
/// span (which carries `request_id`, `method`, `path`, and `tenant_id`).
#[derive(Debug, Clone, Copy)]
pub struct HttpOnResponse {
    service_name: &'static str,
}

impl<B> OnResponse<B> for HttpOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        let status = response.status().as_u16();
        let latency_ms = latency.as_millis() as u64;

        let _enter = span.enter();
        if status >= 500 {
            tracing::error!(
                service = self.service_name,
                status = status,
                latency_ms = latency_ms,
                "HTTP request failed"
            );
        } else if status >= 400 {
            tracing::warn!(
                service = self.service_name,
                status = status,
                latency_ms = latency_ms,
                "HTTP request completed with client error"
            );
        } else {
            tracing::info!(
                service = self.service_name,
                status = status,
                latency_ms = latency_ms,
                "HTTP request completed"
            );
        }
    }
}

/// Builds a reusable Tower/Axum HTTP tracing layer for a microservice.
///
/// Logs `service`, `request_id`, `method`, `path`, `status`, `latency_ms`, and
/// `tenant_id` (from request extensions [`TenantContext`] or `X-Tenant-Id`
/// header). Sensitive headers (`Authorization`, `X-Api-Key`, `Cookie`) are
/// never logged.
pub fn http_trace_layer(service_name: &'static str) -> HttpTraceLayer {
    TraceLayer::new_for_http()
        .make_span_with(HttpMakeSpan { service_name })
        .on_request(())
        .on_response(HttpOnResponse { service_name })
        .on_failure(())
}

fn extract_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn extract_tenant_id<B>(request: &Request<B>) -> String {
    if let Some(ctx) = request.extensions().get::<TenantContext>() {
        return ctx.tenant_id.to_string();
    }
    if let Some(header_value) = extract_header(request.headers(), HEADER_TENANT_ID) {
        return header_value.to_string();
    }
    "-".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::tracing::StructuredJsonFormat;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use serde_json::Value;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    use tracing_subscriber::fmt::format::JsonFields;
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt;
    use uuid::Uuid;

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

    fn build_test_app() -> Router {
        Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .layer(http_trace_layer("test-api"))
            .layer(request_id_layer())
    }

    #[tokio::test]
    async fn generates_uuid_request_id_when_missing_and_returns_on_response() {
        let app = build_test_app();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let header = resp
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("x-request-id header present on response")
            .to_str()
            .expect("valid utf8");

        assert!(
            Uuid::parse_str(header).is_ok(),
            "generated x-request-id should be a valid UUID, got: {header}"
        );
    }

    #[tokio::test]
    async fn propagates_existing_request_id_to_response() {
        let app = build_test_app();
        let incoming_id = "req-custom-trace-42";
        let req = Request::builder()
            .uri("/health")
            .header(REQUEST_ID_HEADER, incoming_id)
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let header = resp
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("x-request-id header present on response")
            .to_str()
            .expect("valid utf8");

        assert_eq!(header, incoming_id);
    }

    #[tokio::test]
    async fn logs_request_fields_and_never_logs_sensitive_headers() {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(buffer.clone())
                .fmt_fields(JsonFields::new())
                .event_format(StructuredJsonFormat::new("test-api")),
        );

        let tenant_id = Uuid::new_v4().to_string();
        let secret_bearer = "Bearer super-secret-jwt-token-999";
        let secret_api_key = "ik_live_super_secret_api_key_888";
        let secret_cookie = "session_id=super_secret_cookie_777";

        let app = build_test_app();
        let req = Request::builder()
            .uri("/health")
            .header(REQUEST_ID_HEADER, "req-safe-check")
            .header(HEADER_TENANT_ID, &tenant_id)
            .header("authorization", secret_bearer)
            .header("x-api-key", secret_api_key)
            .header("cookie", secret_cookie)
            .body(Body::empty())
            .expect("request");

        let _guard = tracing::subscriber::set_default(subscriber);
        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        drop(_guard);

        let output = String::from_utf8(buffer.0.lock().expect("lock").clone()).expect("utf8");
        assert!(!output.is_empty(), "expected HTTP trace log output");
        assert!(
            !output.contains(secret_bearer),
            "Authorization header value must never be logged"
        );
        assert!(
            !output.contains(secret_api_key),
            "X-Api-Key header value must never be logged"
        );
        assert!(
            !output.contains(secret_cookie),
            "Cookie header value must never be logged"
        );

        let line = output.lines().last().expect("log line");
        let parsed: Value = serde_json::from_str(line).expect("valid json");
        assert_eq!(parsed["service"], "test-api");
        assert_eq!(parsed["request_id"], "req-safe-check");
        assert_eq!(parsed["method"], "GET");
        assert_eq!(parsed["path"], "/health");
        assert_eq!(parsed["status"], 200);
        assert_eq!(parsed["tenant_id"], tenant_id);
        assert!(parsed["latency_ms"].as_u64().is_some());
    }
}
