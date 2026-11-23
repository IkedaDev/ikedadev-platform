use crate::error::AppError;
use jsonwebtoken::{
    decode, decode_header,
    jwk::{Jwk, JwkSet},
    Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default TTL for the in-memory JWKS cache (5 minutes).
pub const DEFAULT_JWKS_CACHE_TTL_SECS: u64 = 300;

/// Default JWT claim name injected by Keycloak Protocol Mapper for multi-tenant isolation.
pub const DEFAULT_TENANT_CLAIM_NAME: &str = "tenant_id";

/// Resolved identity extracted from a cryptographically verified Bearer JWT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Tenant UUID extracted from the JWT `tenant_id` claim.
    pub tenant_id: Uuid,
    /// Authenticated user UUID extracted from the JWT `sub` claim.
    pub user_id: Uuid,
    /// Unified list of OIDC scopes and Keycloak realm roles.
    pub scopes: Vec<String>,
}

/// Asynchronous port for validating Bearer JWT tokens.
#[async_trait::async_trait]
pub trait JwtValidator: Send + Sync {
    /// Validates a raw Bearer JWT (without the `Bearer ` prefix) and returns the authenticated user.
    async fn validate_token(&self, token: &str) -> Result<AuthenticatedUser, AppError>;
}

/// Configuration for stateless RS256 JWT validation against a remote OIDC JWKS endpoint.
#[derive(Debug, Clone)]
pub struct JwksConfig {
    /// OIDC JWKS endpoint URL (e.g. `https://auth.example.com/realms/ikeda/protocol/openid-connect/certs`).
    pub jwks_uri: String,
    /// Expected token issuer (`iss` claim).
    pub issuer: String,
    /// Optional expected audience (`aud` claim). When `None`, audience validation is skipped.
    pub audience: Option<String>,
    /// How long fetched JWKS keys remain valid in memory before a scheduled refresh.
    pub cache_ttl: Duration,
    /// Name of the JWT claim holding the tenant UUID (defaults to `"tenant_id"`).
    pub tenant_claim_name: String,
}

impl JwksConfig {
    /// Creates a new `JwksConfig` with sensible defaults (5-minute cache TTL, `tenant_id` claim).
    #[must_use]
    pub fn new(jwks_uri: impl Into<String>, issuer: impl Into<String>) -> Self {
        Self {
            jwks_uri: jwks_uri.into(),
            issuer: issuer.into(),
            audience: None,
            cache_ttl: Duration::from_secs(DEFAULT_JWKS_CACHE_TTL_SECS),
            tenant_claim_name: DEFAULT_TENANT_CLAIM_NAME.to_string(),
        }
    }

    /// Sets an expected audience (`aud`) claim to enforce during validation.
    #[must_use]
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// Overrides the JWKS cache TTL.
    #[must_use]
    pub fn with_cache_ttl(mut self, cache_ttl: Duration) -> Self {
        self.cache_ttl = cache_ttl;
        self
    }

    /// Overrides the JWT claim name used to extract `tenant_id`.
    #[must_use]
    pub fn with_tenant_claim_name(mut self, claim_name: impl Into<String>) -> Self {
        self.tenant_claim_name = claim_name.into();
        self
    }
}

#[derive(Debug, Clone)]
struct CachedJwkSet {
    jwks: JwkSet,
    fetched_at: Instant,
}

#[derive(Debug, Deserialize)]
struct RealmAccess {
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct KeycloakClaims {
    sub: Option<String>,
    scope: Option<String>,
    realm_access: Option<RealmAccess>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

/// Stateless RS256 JWT validator backed by an in-memory cached OIDC JWKS endpoint.
///
/// Automatically fetches keys on cold start, refreshes when the cache TTL expires,
/// and triggers an on-demand refresh when a token references an unknown `kid`.
pub struct JwksJwtValidator {
    config: JwksConfig,
    http_client: reqwest::Client,
    cache: Arc<RwLock<Option<CachedJwkSet>>>,
}

impl JwksJwtValidator {
    /// Creates a new `JwksJwtValidator` with an empty cache that will lazily fetch keys from `jwks_uri`.
    pub fn new(config: JwksConfig) -> Result<Self, AppError> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to initialize JWKS HTTP client: {e}"))
            })?;

        Ok(Self {
            config,
            http_client,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    /// Creates a `JwksJwtValidator` pre-populated with a static `JwkSet` (useful for unit tests
    /// or air-gapped environments).
    pub fn with_static_jwks(config: JwksConfig, jwks: JwkSet) -> Result<Self, AppError> {
        let validator = Self::new(config)?;
        let cached = CachedJwkSet {
            jwks,
            fetched_at: Instant::now(),
        };
        Ok(Self {
            cache: Arc::new(RwLock::new(Some(cached))),
            ..validator
        })
    }

    async fn resolve_jwk(&self, kid: Option<&str>) -> Result<Jwk, AppError> {
        // 1. Try reading from in-memory cache if fresh
        {
            let guard = self.cache.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.fetched_at.elapsed() < self.config.cache_ttl {
                    if let Some(jwk) = find_jwk(&cached.jwks, kid) {
                        return Ok(jwk.clone());
                    }
                }
            }
        }

        // 2. Cache miss, expired TTL, or unknown kid -> refresh from JWKS URI
        let jwks = self.refresh_jwks().await?;

        find_jwk(&jwks, kid).cloned().ok_or_else(|| {
            AppError::Unauthorized("Unable to find matching JWK for token 'kid'".to_string())
        })
    }

    async fn refresh_jwks(&self) -> Result<JwkSet, AppError> {
        if self.config.jwks_uri.trim().is_empty() {
            return Err(AppError::Unauthorized(
                "Unable to find matching JWK and no JWKS URI is configured".to_string(),
            ));
        }

        let response = self
            .http_client
            .get(&self.config.jwks_uri)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, jwks_uri = %self.config.jwks_uri, "Failed to fetch JWKS");
                AppError::Unauthorized("Failed to retrieve identity provider signing keys".to_string())
            })?;

        if !response.status().is_success() {
            tracing::error!(
                status = %response.status(),
                jwks_uri = %self.config.jwks_uri,
                "JWKS endpoint returned non-success status"
            );
            return Err(AppError::Unauthorized(
                "Identity provider signing keys endpoint unavailable".to_string(),
            ));
        }

        let jwks: JwkSet = response.json().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to deserialize JWKS payload");
            AppError::Unauthorized("Invalid JWKS response from identity provider".to_string())
        })?;

        let mut guard = self.cache.write().await;
        *guard = Some(CachedJwkSet {
            jwks: jwks.clone(),
            fetched_at: Instant::now(),
        });

        Ok(jwks)
    }
}

fn find_jwk<'a>(jwks: &'a JwkSet, kid: Option<&str>) -> Option<&'a Jwk> {
    match kid {
        Some(target_kid) => jwks.find(target_kid),
        None => {
            if jwks.keys.len() == 1 {
                jwks.keys.first()
            } else {
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl JwtValidator for JwksJwtValidator {
    async fn validate_token(&self, token: &str) -> Result<AuthenticatedUser, AppError> {
        let header = decode_header(token)
            .map_err(|_| AppError::Unauthorized("Malformed JWT header".to_string()))?;

        if header.alg != Algorithm::RS256 {
            return Err(AppError::Unauthorized(format!(
                "Unsupported JWT signing algorithm: {:?} (expected RS256)",
                header.alg
            )));
        }

        let jwk = self.resolve_jwk(header.kid.as_deref()).await?;

        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|e| {
            tracing::error!(error = %e, "Failed to construct RS256 DecodingKey from JWK");
            AppError::Unauthorized("Invalid signing key material".to_string())
        })?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        validation.set_issuer(&[&self.config.issuer]);

        if let Some(ref aud) = self.config.audience {
            validation.set_audience(&[aud]);
        } else {
            validation.validate_aud = false;
        }

        let token_data = decode::<KeycloakClaims>(token, &decoding_key, &validation)
            .map_err(|e| AppError::Unauthorized(format!("Invalid or expired JWT: {e}")))?;

        let claims = token_data.claims;

        // Extract and validate `sub` -> user_id
        let sub_str = claims.sub.as_deref().ok_or_else(|| {
            AppError::Unauthorized("JWT missing required 'sub' claim".to_string())
        })?;
        let user_id = Uuid::parse_str(sub_str.trim()).map_err(|_| {
            AppError::Unauthorized("JWT 'sub' claim is not a valid UUID".to_string())
        })?;

        // Extract and validate `tenant_id` claim
        let tenant_val = claims
            .extra
            .get(&self.config.tenant_claim_name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AppError::Unauthorized(format!(
                    "JWT missing required '{}' claim",
                    self.config.tenant_claim_name
                ))
            })?;
        let tenant_id = Uuid::parse_str(tenant_val.trim()).map_err(|_| {
            AppError::Unauthorized(format!(
                "JWT '{}' claim is not a valid UUID",
                self.config.tenant_claim_name
            ))
        })?;

        // Unify OIDC `scope` and Keycloak `realm_access.roles` without duplicates
        let mut seen = HashSet::new();
        let mut scopes = Vec::new();

        if let Some(scope_str) = claims.scope {
            for s in scope_str.split_whitespace() {
                if seen.insert(s.to_string()) {
                    scopes.push(s.to_string());
                }
            }
        }

        if let Some(realm_access) = claims.realm_access {
            for role in realm_access.roles {
                let trimmed = role.trim();
                if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
                    scopes.push(trimmed.to_string());
                }
            }
        }

        Ok(AuthenticatedUser {
            tenant_id,
            user_id,
            scopes,
        })
    }
}
