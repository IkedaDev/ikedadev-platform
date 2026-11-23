use super::context::{AuthSource, TenantContext};
use super::extractor::{HEADER_API_KEY, HEADER_TENANT_ID};
#[cfg(feature = "dev-auth")]
use super::extractor::{HEADER_SCOPES, HEADER_USER_ID};
use super::jwt::{JwksConfig, JwksJwtValidator, JwtValidator};
use super::validator::{hash_api_key, ApiKeyValidator, AuthenticatedKey};
use crate::error::AppError;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{header, Request},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use jsonwebtoken::{
    encode,
    jwk::{AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, RSAKeyParameters},
    Algorithm, EncodingKey, Header,
};
use rand::rngs::OsRng;
use rsa::{
    pkcs1::{EncodeRsaPrivateKey, LineEnding},
    traits::PublicKeyParts,
    RsaPrivateKey, RsaPublicKey,
};
use serde_json::json;
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
    jwt_validator: Option<Arc<dyn JwtValidator>>,
}

impl TestAppState {
    fn new(validator: Arc<dyn ApiKeyValidator>) -> Self {
        Self {
            validator,
            jwt_validator: None,
        }
    }

    fn with_jwt(mut self, jwt_validator: Arc<dyn JwtValidator>) -> Self {
        self.jwt_validator = Some(jwt_validator);
        self
    }
}

impl axum::extract::FromRef<TestAppState> for Arc<dyn ApiKeyValidator> {
    fn from_ref(state: &TestAppState) -> Self {
        state.validator.clone()
    }
}

impl axum::extract::FromRef<TestAppState> for Option<Arc<dyn JwtValidator>> {
    fn from_ref(state: &TestAppState) -> Self {
        state.jwt_validator.clone()
    }
}

struct TestRsaKeyPair {
    kid: String,
    encoding_key: EncodingKey,
    jwk_set: JwkSet,
}

impl TestRsaKeyPair {
    fn generate(kid: &str) -> Self {
        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate RSA key");
        let pub_key = RsaPublicKey::from(&priv_key);

        let pem = priv_key
            .to_pkcs1_pem(LineEnding::LF)
            .expect("failed to encode private key to PEM");
        let encoding_key =
            EncodingKey::from_rsa_pem(pem.as_bytes()).expect("failed to create EncodingKey");

        let n = URL_SAFE_NO_PAD.encode(pub_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(pub_key.e().to_bytes_be());

        let jwk = Jwk {
            common: CommonParameters {
                key_id: Some(kid.to_string()),
                key_algorithm: Some(KeyAlgorithm::RS256),
                ..Default::default()
            },
            algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                key_type: Default::default(),
                n,
                e,
            }),
        };

        Self {
            kid: kid.to_string(),
            encoding_key,
            jwk_set: JwkSet { keys: vec![jwk] },
        }
    }

    fn sign(&self, claims: &serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, claims, &self.encoding_key).expect("failed to sign JWT")
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
async fn test_jwks_valid_token_extracts_user_tenant_and_scopes() {
    let kp = TestRsaKeyPair::generate("test-kid-1");
    let issuer = "https://auth.ikedadev.com/realms/ikeda";
    let config = JwksConfig::new("", issuer);
    let validator =
        JwksJwtValidator::with_static_jwks(config, kp.jwk_set.clone()).expect("validator init");

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let exp = (Utc::now().timestamp() + 600) as usize;

    let claims = json!({
        "iss": issuer,
        "sub": user_id.to_string(),
        "tenant_id": tenant_id.to_string(),
        "exp": exp,
        "scope": "openid profile events:read",
        "realm_access": {
            "roles": ["tenant:admin", "events:read", "crm:write"]
        }
    });

    let token = kp.sign(&claims);
    let user = validator
        .validate_token(&token)
        .await
        .expect("valid token should succeed");

    assert_eq!(user.tenant_id, tenant_id);
    assert_eq!(user.user_id, user_id);
    assert!(user.scopes.contains(&"openid".to_string()));
    assert!(user.scopes.contains(&"events:read".to_string()));
    assert!(user.scopes.contains(&"tenant:admin".to_string()));
    assert!(user.scopes.contains(&"crm:write".to_string()));
    // Deduplicated: `events:read` appears only once
    assert_eq!(
        user.scopes
            .iter()
            .filter(|s| s.as_str() == "events:read")
            .count(),
        1
    );
}

#[tokio::test]
async fn test_jwks_expired_token_returns_unauthorized() {
    let kp = TestRsaKeyPair::generate("test-kid-exp");
    let issuer = "https://auth.ikedadev.com/realms/ikeda";
    let config = JwksConfig::new("", issuer);
    let validator =
        JwksJwtValidator::with_static_jwks(config, kp.jwk_set.clone()).expect("validator init");

    let claims = json!({
        "iss": issuer,
        "sub": Uuid::new_v4().to_string(),
        "tenant_id": Uuid::new_v4().to_string(),
        "exp": (Utc::now().timestamp() - 300) as usize,
    });

    let token = kp.sign(&claims);
    let err = validator
        .validate_token(&token)
        .await
        .expect_err("expired token must fail");

    assert!(matches!(err, AppError::Unauthorized(_)));
}

#[tokio::test]
async fn test_jwks_invalid_signature_returns_unauthorized() {
    let trusted_kp = TestRsaKeyPair::generate("shared-kid");
    let attacker_kp = TestRsaKeyPair::generate("shared-kid");
    let issuer = "https://auth.ikedadev.com/realms/ikeda";
    let config = JwksConfig::new("", issuer);
    let validator = JwksJwtValidator::with_static_jwks(config, trusted_kp.jwk_set.clone())
        .expect("validator init");

    let claims = json!({
        "iss": issuer,
        "sub": Uuid::new_v4().to_string(),
        "tenant_id": Uuid::new_v4().to_string(),
        "exp": (Utc::now().timestamp() + 600) as usize,
    });

    let forged_token = attacker_kp.sign(&claims);
    let err = validator
        .validate_token(&forged_token)
        .await
        .expect_err("forged signature must fail");

    assert!(matches!(err, AppError::Unauthorized(_)));
}

#[tokio::test]
async fn test_jwks_missing_tenant_id_claim_returns_unauthorized() {
    let kp = TestRsaKeyPair::generate("test-kid-no-tenant");
    let issuer = "https://auth.ikedadev.com/realms/ikeda";
    let config = JwksConfig::new("", issuer);
    let validator =
        JwksJwtValidator::with_static_jwks(config, kp.jwk_set.clone()).expect("validator init");

    let claims = json!({
        "iss": issuer,
        "sub": Uuid::new_v4().to_string(),
        "exp": (Utc::now().timestamp() + 600) as usize,
    });

    let token = kp.sign(&claims);
    let err = validator
        .validate_token(&token)
        .await
        .expect_err("token without tenant_id claim must fail");

    match err {
        AppError::Unauthorized(msg) => assert!(msg.contains("tenant_id")),
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn test_jwks_wrong_issuer_returns_unauthorized() {
    let kp = TestRsaKeyPair::generate("test-kid-iss");
    let config = JwksConfig::new("", "https://auth.ikedadev.com/realms/ikeda");
    let validator =
        JwksJwtValidator::with_static_jwks(config, kp.jwk_set.clone()).expect("validator init");

    let claims = json!({
        "iss": "https://evil.example.com/realms/other",
        "sub": Uuid::new_v4().to_string(),
        "tenant_id": Uuid::new_v4().to_string(),
        "exp": (Utc::now().timestamp() + 600) as usize,
    });

    let token = kp.sign(&claims);
    let err = validator
        .validate_token(&token)
        .await
        .expect_err("wrong issuer must fail");

    assert!(matches!(err, AppError::Unauthorized(_)));
}

#[tokio::test]
async fn test_extractor_with_valid_bearer_jwt_ignores_spoofed_tenant_header() {
    let kp = TestRsaKeyPair::generate("extractor-kid");
    let issuer = "https://auth.ikedadev.com/realms/ikeda";
    let config = JwksConfig::new("", issuer);
    let jwt_validator =
        JwksJwtValidator::with_static_jwks(config, kp.jwk_set.clone()).expect("validator init");

    let state =
        TestAppState::new(Arc::new(MockApiKeyValidator::new())).with_jwt(Arc::new(jwt_validator));

    let real_tenant_id = Uuid::new_v4();
    let spoofed_tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let claims = json!({
        "iss": issuer,
        "sub": user_id.to_string(),
        "tenant_id": real_tenant_id.to_string(),
        "exp": (Utc::now().timestamp() + 600) as usize,
        "realm_access": { "roles": ["tenant:admin"] }
    });
    let token = kp.sign(&claims);

    let req = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(HEADER_TENANT_ID, spoofed_tenant_id.to_string())
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let ctx = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect("extractor should succeed with valid JWT");

    assert_eq!(ctx.tenant_id, real_tenant_id);
    assert_ne!(ctx.tenant_id, spoofed_tenant_id);
    assert_eq!(ctx.user_id, Some(user_id));
    assert_eq!(ctx.auth_source, AuthSource::Jwt);
    assert!(ctx.has_scope("tenant:admin"));
}

#[tokio::test]
async fn test_extractor_with_valid_api_key_ignores_spoofed_tenant_header() {
    let mut mock = MockApiKeyValidator::new();
    let real_tenant_id = Uuid::new_v4();
    let spoofed_tenant_id = Uuid::new_v4();
    let raw_key = "ikeda_live_valid_key_999";

    mock.add_key(raw_key, real_tenant_id, vec!["events:write".into()]);
    let state = TestAppState::new(Arc::new(mock));

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
    let state = TestAppState::new(Arc::new(MockApiKeyValidator::new()));

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
    let state = TestAppState::new(Arc::new(MockApiKeyValidator::new()));

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
async fn test_extractor_bearer_returns_unauthorized_when_jwt_validator_not_configured() {
    let state = TestAppState::new(Arc::new(MockApiKeyValidator::new()));

    let req = Request::builder()
        .header(header::AUTHORIZATION, "Bearer some.jwt.token")
        .body(Body::empty())
        .expect("request build failed");

    let (mut parts, _) = req.into_parts();
    let err = TenantContext::from_request_parts(&mut parts, &state)
        .await
        .expect_err("should reject bearer token when jwt validator is None");

    match err {
        AppError::Unauthorized(msg) => {
            assert!(msg.contains("JWT/JWKS is not configured"));
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[cfg(not(feature = "dev-auth"))]
#[tokio::test]
async fn test_extractor_without_dev_auth_ignores_plain_tenant_header() {
    let state = TestAppState::new(Arc::new(MockApiKeyValidator::new()));

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
    let state = TestAppState::new(Arc::new(MockApiKeyValidator::new()));
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

struct CountingApiKeyValidator {
    inner: MockApiKeyValidator,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait]
impl ApiKeyValidator for CountingApiKeyValidator {
    async fn validate_key_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<AuthenticatedKey>, AppError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.validate_key_hash(key_hash).await
    }
}

#[tokio::test]
async fn test_cached_api_key_validator_hits_cache_and_expires_after_ttl() {
    use super::validator::CachedApiKeyValidator;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let mut mock = MockApiKeyValidator::new();
    let tenant_id = Uuid::new_v4();
    let raw_key = "ik_live_cached_key_001";
    let key_hash = hash_api_key(raw_key);
    mock.add_key(raw_key, tenant_id, vec!["crm:read".into()]);

    let calls = Arc::new(AtomicUsize::new(0));
    let counting = CountingApiKeyValidator {
        inner: mock,
        calls: calls.clone(),
    };
    let cached = CachedApiKeyValidator::with_ttl(counting, Duration::from_millis(50));

    // 1. First call -> cache miss, delegates to inner validator
    let first = cached
        .validate_key_hash(&key_hash)
        .await
        .expect("first lookup failed")
        .expect("expected valid key");
    assert_eq!(first.tenant_id, tenant_id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // 2. Second call within TTL -> cache hit, inner NOT called again
    let second = cached
        .validate_key_hash(&key_hash)
        .await
        .expect("second lookup failed")
        .expect("expected cached key");
    assert_eq!(second.tenant_id, tenant_id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // 3. Sleep past TTL -> cache expired, delegates to inner validator again
    tokio::time::sleep(Duration::from_millis(70)).await;
    let third = cached
        .validate_key_hash(&key_hash)
        .await
        .expect("third lookup failed")
        .expect("expected refreshed key");
    assert_eq!(third.tenant_id, tenant_id);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_cached_api_key_validator_rejects_invalid_keys_without_caching() {
    use super::validator::CachedApiKeyValidator;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let calls = Arc::new(AtomicUsize::new(0));
    let counting = CountingApiKeyValidator {
        inner: MockApiKeyValidator::new(),
        calls: calls.clone(),
    };
    let cached = CachedApiKeyValidator::with_ttl(counting, Duration::from_secs(60));

    let bogus_hash = hash_api_key("ik_live_invalid_key");

    let res1 = cached
        .validate_key_hash(&bogus_hash)
        .await
        .expect("validation error");
    assert!(res1.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Invalid keys are not cached as valid; second lookup delegates again and returns None
    let res2 = cached
        .validate_key_hash(&bogus_hash)
        .await
        .expect("validation error");
    assert!(res2.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
