use chrono::{Duration, Utc};
use ikeda_core::error::AppError;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Default expiration for draft preview tokens (30 minutes).
pub const PREVIEW_TOKEN_TTL_MINUTES: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviewClaims {
    sub: String,       // article_id
    tenant_id: String, // tenant_id
    purpose: String,   // "article_preview"
    iat: i64,
    exp: i64,
}

/// Verified payload extracted from a draft preview token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPreviewToken {
    pub tenant_id: Uuid,
    pub article_id: Uuid,
}

/// Response returned when generating a temporary draft preview token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreviewTokenResponse {
    pub token: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub preview_path: String,
}

/// Generates and verifies HMAC-SHA256 signed temporary preview tokens for draft articles.
#[derive(Clone)]
pub struct PreviewTokenService {
    secret: String,
}

impl PreviewTokenService {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    pub fn generate(
        &self,
        tenant_id: Uuid,
        article_id: Uuid,
    ) -> Result<PreviewTokenResponse, AppError> {
        let now = Utc::now();
        let expires_at = now + Duration::minutes(PREVIEW_TOKEN_TTL_MINUTES);
        let claims = PreviewClaims {
            sub: article_id.to_string(),
            tenant_id: tenant_id.to_string(),
            purpose: "article_preview".to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
        };

        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| AppError::InternalServerError(format!("Failed to sign preview token: {e}")))?;

        Ok(PreviewTokenResponse {
            preview_path: format!("/v1/articles/preview?token={token}"),
            token,
            expires_at,
        })
    }

    pub fn verify(&self, token: &str) -> Result<VerifiedPreviewToken, AppError> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Err(AppError::Unauthorized("Missing preview token".to_string()));
        }

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.leeway = 5;

        let data = decode::<PreviewClaims>(
            trimmed,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::Unauthorized("Invalid or expired preview token".to_string()))?;

        if data.claims.purpose != "article_preview" {
            return Err(AppError::Unauthorized(
                "Invalid preview token purpose".to_string(),
            ));
        }

        let tenant_id = Uuid::parse_str(&data.claims.tenant_id).map_err(|_| {
            AppError::Unauthorized("Malformed tenant_id in preview token".to_string())
        })?;
        let article_id = Uuid::parse_str(&data.claims.sub).map_err(|_| {
            AppError::Unauthorized("Malformed article_id in preview token".to_string())
        })?;

        Ok(VerifiedPreviewToken {
            tenant_id,
            article_id,
        })
    }
}
