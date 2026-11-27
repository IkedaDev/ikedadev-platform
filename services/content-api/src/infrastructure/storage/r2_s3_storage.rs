use crate::domain::models::PresignedUpload;
use crate::domain::ports::ObjectStoragePort;
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use ikeda_core::error::AppError;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const PRESIGN_EXPIRES_SECONDS: u64 = 900; // 15 minutes

/// Configuration for Cloudflare R2 (or any S3-compatible bucket via AWS SigV4).
#[derive(Debug, Clone)]
pub struct R2Config {
    pub account_id: String,
    pub bucket_name: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub public_base_url: String,
    pub endpoint_url: Option<String>,
}

/// Cloudflare R2 / S3-compatible presigned URL generator implementing `ObjectStoragePort`.
pub struct R2S3ObjectStorage {
    config: R2Config,
}

impl R2S3ObjectStorage {
    pub fn new(config: R2Config) -> Self {
        Self { config }
    }

    fn endpoint(&self) -> String {
        self.config
            .endpoint_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| {
                format!(
                    "https://{}.r2.cloudflarestorage.com",
                    self.config.account_id.trim()
                )
            })
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut mac = HmacSha256::new_from_slice(key)
            .map_err(|e| AppError::InternalServerError(format!("HMAC init failed: {e}")))?;
        mac.update(data);
        Ok(mac.finalize().into_bytes().to_vec())
    }

    fn percent_encode_query_component(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for b in input.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }
}

#[async_trait]
impl ObjectStoragePort for R2S3ObjectStorage {
    fn provider_name(&self) -> &'static str {
        "R2_PLATFORM"
    }

    async fn generate_presigned_upload(
        &self,
        tenant_id: Uuid,
        asset_id: Uuid,
        extension: &str,
        _mime_type: &str,
        _size_bytes: i64,
    ) -> Result<PresignedUpload, AppError> {
        let storage_key = format!("tenants/{tenant_id}/media/{asset_id}.{extension}");
        let endpoint = self.endpoint();
        let parsed_endpoint = Url::parse(&endpoint)
            .map_err(|e| AppError::InternalServerError(format!("Invalid R2 endpoint URL: {e}")))?;
        let host = parsed_endpoint.host_str().ok_or_else(|| {
            AppError::InternalServerError("R2 endpoint URL missing host".to_string())
        })?;

        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = "auto";
        let service = "s3";
        let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
        let credential = format!("{}/{credential_scope}", self.config.access_key_id);

        let canonical_uri = format!("/{}/{}", self.config.bucket_name.trim(), storage_key);

        let query_params = [
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_string()),
            (
                "X-Amz-Credential",
                Self::percent_encode_query_component(&credential),
            ),
            ("X-Amz-Date", amz_date.clone()),
            ("X-Amz-Expires", PRESIGN_EXPIRES_SECONDS.to_string()),
            ("X-Amz-SignedHeaders", "host".to_string()),
        ];

        let canonical_query = query_params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        let canonical_headers = format!("host:{host}\n");
        let canonical_request = format!(
            "PUT\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\nhost\nUNSIGNED-PAYLOAD"
        );

        let hashed_canonical_request = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_canonical_request}");

        let k_date = Self::hmac_sha256(
            format!("AWS4{}", self.config.secret_access_key).as_bytes(),
            date_stamp.as_bytes(),
        )?;
        let k_region = Self::hmac_sha256(&k_date, region.as_bytes())?;
        let k_service = Self::hmac_sha256(&k_region, service.as_bytes())?;
        let k_signing = Self::hmac_sha256(&k_service, b"aws4_request")?;
        let signature = hex::encode(Self::hmac_sha256(&k_signing, string_to_sign.as_bytes())?);

        let upload_url =
            format!("{endpoint}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}");
        let public_url = self.public_url_for_key(&storage_key);

        Ok(PresignedUpload {
            asset_id,
            storage_provider: self.provider_name().to_string(),
            storage_key,
            upload_url,
            public_url,
            expires_in_seconds: PRESIGN_EXPIRES_SECONDS,
        })
    }

    fn public_url_for_key(&self, storage_key: &str) -> String {
        format!(
            "{}/{}",
            self.config.public_base_url.trim_end_matches('/'),
            storage_key.trim_start_matches('/')
        )
    }
}

/// Local development / No-op Object Storage adapter when R2 credentials are not configured.
/// Serves and accepts uploads via `http://localhost:8086/v1/media/local/{tenant_id}/{asset_id}.{ext}`.
#[derive(Debug, Clone)]
pub struct NoopObjectStorage {
    public_base_url: String,
}

impl Default for NoopObjectStorage {
    fn default() -> Self {
        Self {
            public_base_url: "http://localhost:8086/v1/media/local".to_string(),
        }
    }
}

impl NoopObjectStorage {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self {
            public_base_url: public_base_url.into(),
        }
    }
}

#[async_trait]
impl ObjectStoragePort for NoopObjectStorage {
    fn provider_name(&self) -> &'static str {
        "R2_PLATFORM"
    }

    async fn generate_presigned_upload(
        &self,
        tenant_id: Uuid,
        asset_id: Uuid,
        extension: &str,
        _mime_type: &str,
        _size_bytes: i64,
    ) -> Result<PresignedUpload, AppError> {
        let storage_key = format!("tenants/{tenant_id}/media/{asset_id}.{extension}");
        let public_url = self.public_url_for_key(&storage_key);
        let upload_url = public_url.clone();

        Ok(PresignedUpload {
            asset_id,
            storage_provider: self.provider_name().to_string(),
            storage_key,
            upload_url,
            public_url,
            expires_in_seconds: PRESIGN_EXPIRES_SECONDS,
        })
    }

    fn public_url_for_key(&self, storage_key: &str) -> String {
        let base = self.public_base_url.trim_end_matches('/');
        // Convert canonical key `tenants/{tenant_id}/media/{filename}` -> `{base}/{tenant_id}/{filename}`
        let parts: Vec<&str> = storage_key.trim_start_matches('/').split('/').collect();
        if parts.len() == 4 && parts[0] == "tenants" && parts[2] == "media" {
            format!("{base}/{}/{}", parts[1], parts[3])
        } else {
            format!("{base}/{}", storage_key.trim_start_matches('/'))
        }
    }
}
