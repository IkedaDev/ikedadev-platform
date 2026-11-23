use super::context::{AuthSource, TenantContext};
use super::extractor::{HEADER_API_KEY, HEADER_TENANT_ID};
#[cfg(feature = "dev-auth")]
use super::extractor::{HEADER_SCOPES, HEADER_USER_ID};
use super::validator::{hash_api_key, ApiKeyValidator, AuthenticatedKey};
use crate::error::AppError;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{header, Request},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

struct MockApiKeyValidator {
    keys: HashMap<String, AuthenticatedKey>,
}

impl MockApiKeyValidator {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    fn add_key(&mut self, raw_key: &str, tenant_id: Uuid, scopes: Vec<String>) {
        let hash = hash_api_key(raw_key);
        self.keys
            .insert(hash, AuthenticatedKey { tenant_id, scopes });
    }
}

#[async_trait]
impl ApiKeyValidator for MockApiKeyValidator {
    async fn validate_key_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        Ok(self.keys.get(key_hash).cloned())
    }
}

#[derive(Clone)]
struct TestAppState {
    validator: Arc<dyn ApiKeyValidator>,
}

impl axum::extract::FromRef<TestAppState> for Arc<dyn ApiKeyValidator> {
    fn from_ref(state: &TestAppState) -> Self {
        state.validator.clone()
    }
}

#[test]
fn test_tenant_context_scopes() {
    let tenant_id = Uuid::new_v4();
    let ctx = TenantContext::new(tenant_id)
        .with_scopes(vec!["events:read".into(), "events:write".into()]);

    assert!(ctx.has_scope("events:read"));
    assert!(ctx.has_scope("events:write"));
    assert!(!ctx.has_scope("events:delete"));

    assert!(ctx.require_scope("events:read").is_ok());
    assert!(ctx.require_scope("events:delete").is_err());
}

#[test]
fn test_tenant_context_wildcard_scope() {
    let tenant_id = Uuid::new_v4();
    let ctx = TenantContext::new(tenant_id).with_scopes(vec!["*".into()]);

    assert!(ctx.has_scope("events:delete"));
    assert!(ctx.has_scope("admin:all"));
    assert!(ctx.require_scope("anything").is_ok());
}

#[test]
fn test_hash_api_key_deterministic() {
    let raw = "ikeda_sec_12345";
    let hash1 = hash_api_key(raw);
    let hash2 = hash_api_key(raw);
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64);
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn test_extractor_with_valid_api_key_ignores_spoofed_tenant_header() {
    let mut mock = MockApiKeyValidator::new();
    let real_tenant_id = Uuid::new_v4();
    let spoofed_tenant_id = Uuid::new_v4();
    let raw_key = "ikeda_live_valid_key_999";

    mock.add_key(raw_key, real_tenant_id, vec!["events:write".into()]);
    let state = TestAppState {
        validator: Arc::new(mock),
    };

    let req = Request::builder()
        .header(HEADER_API_KEY, raw_key)
        .header(HEADER_TENANT_ID, spoofed_tenant_id.to_string())
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let ctx = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect("extractor should succeed with valid api key");

    // Real tenant MUST be resolved, spoofed header MUST be ignored
    assert_eq!(ctx.tenant_id, real_tenant_id);
    assert_ne!(ctx.tenant_id, spoofed_tenant_id);
    assert_eq!(ctx.auth_source, AuthSource::ApiKey);
    assert_eq!(ctx.scopes, vec!["events:write"]);
}

#[tokio::test]
async fn test_extractor_with_invalid_api_key_returns_unauthorized() {
    let mock = MockApiKeyValidator::new();
    let state = TestAppState {
        validator: Arc::new(mock),
    };

    let req = Request::builder()
        .header(HEADER_API_KEY, "invalid_or_revoked_key")
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let err = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect_err("should reject invalid api key");

    match err {
        AppError::Unauthorized(msg) => {
            assert!(msg.contains("Invalid or expired API key"));
        }
        other => panic!("expected Unauthorized error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_extractor_without_credentials_returns_unauthorized() {
    let mock = MockApiKeyValidator::new();
    let state = TestAppState {
        validator: Arc::new(mock),
    };

    let req = Request::builder()
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let err = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect_err("should reject request with no credentials");

    match err {
        AppError::Unauthorized(_) => {}
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn test_extractor_bearer_returns_unauthorized_not_configured() {
    let mock = MockApiKeyValidator::new();
    let state = TestAppState {
        validator: Arc::new(mock),
    };

    let req = Request::builder()
        .header(header::AUTHORIZATION, "Bearer some.jwt.token")
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let err = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect_err("should reject bearer token before jwt configured");

    match err {
        AppError::Unauthorized(msg) => {
            assert!(msg.contains("JWT/JWKS is not yet configured"));
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[cfg(not(feature = "dev-auth"))]
#[tokio::test]
async fn test_extractor_without_dev_auth_ignores_plain_tenant_header() {
    let mock = MockApiKeyValidator::new();
    let state = TestAppState {
        validator: Arc::new(mock),
    };

    let req = Request::builder()
        .header(HEADER_TENANT_ID, Uuid::new_v4().to_string())
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let err = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect_err("without dev-auth, X-Tenant-Id without API key must fail");

    match err {
        AppError::Unauthorized(msg) => {
            assert!(msg.contains("Missing or invalid authentication credentials"));
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[cfg(feature = "dev-auth")]
#[tokio::test]
async fn test_extractor_with_dev_auth_feature_allows_dev_headers() {
    let mock = MockApiKeyValidator::new();
    let state = TestAppState {
        validator: Arc::new(mock),
    };
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let req = Request::builder()
        .header(HEADER_TENANT_ID, tenant_id.to_string())
        .header(HEADER_USER_ID, user_id.to_string())
        .header(HEADER_SCOPES, "events:read, calendars:write")
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let ctx = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect("with dev-auth enabled, dev headers should succeed");

    assert_eq!(ctx.tenant_id, tenant_id);
    assert_eq!(ctx.user_id, Some(user_id));
    assert_eq!(ctx.auth_source, AuthSource::DevHeaders);
    assert_eq!(ctx.scopes, vec!["events:read", "calendars:write"]);
}
