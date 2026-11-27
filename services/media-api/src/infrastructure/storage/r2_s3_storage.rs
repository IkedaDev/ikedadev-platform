use crate::domain::ports::{GeneratedPresignedPut, ObjectStoragePort};
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use ikeda_core::error::AppError;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const PRESIGN_EXPIRES_SECS: u64 = 900; // 15 minutes

/// Cloudflare R2 / S3-compatible configuration.
#[derive(Debug, Clone)]
pub struct R2Config {
    pub account_id: String,
    pub bucket_name: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub public_base_url: String,
    pub endpoint_url: Option<String>,
}

/// Cloudflare R2 / S3-compatible adapter implementing `ObjectStoragePort` (AWS SigV4 PUT & DELETE).
pub struct R2S3ObjectStorage {
    config: R2Config,
    http: reqwest::Client,
}

impl R2S3ObjectStorage {
    pub fn new(config: R2Config) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
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

    fn percent_encode(input: &str) -> String {
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

    fn signing_key(&self, date_stamp: &str, region: &str) -> Result<Vec<u8>, AppError> {
        let k_date = Self::hmac_sha256(
            format!("AWS4{}", self.config.secret_access_key).as_bytes(),
            date_stamp.as_bytes(),
        )?;
        let k_region = Self::hmac_sha256(&k_date, region.as_bytes())?;
        let k_service = Self::hmac_sha256(&k_region, b"s3")?;
        Self::hmac_sha256(&k_service, b"aws4_request")
    }
}

#[async_trait]
impl ObjectStoragePort for R2S3ObjectStorage {
    fn provider_name(&self) -> &'static str {
        "R2_PLATFORM"
    }

    async fn generate_presigned_put(
        &self,
        storage_key: &str,
        _mime_type: &str,
        _size_bytes: i64,
    ) -> Result<GeneratedPresignedPut, AppError> {
        let endpoint = self.endpoint();
        let parsed = Url::parse(&endpoint)
            .map_err(|e| AppError::InternalServerError(format!("Invalid R2 endpoint URL: {e}")))?;
        let host = parsed.host_str().ok_or_else(|| {
            AppError::InternalServerError("R2 endpoint URL missing host".to_string())
        })?;

        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = "auto";
        let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
        let credential = format!("{}/{credential_scope}", self.config.access_key_id);
        let canonical_uri = format!("/{}/{}", self.config.bucket_name.trim(), storage_key);

        let query_params = [
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_string()),
            ("X-Amz-Credential", Self::percent_encode(&credential)),
            ("X-Amz-Date", amz_date.clone()),
            ("X-Amz-Expires", PRESIGN_EXPIRES_SECS.to_string()),
            ("X-Amz-SignedHeaders", "host".to_string()),
        ];

        let canonical_query = query_params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let canonical_request = format!(
            "PUT\n{canonical_uri}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD"
        );
        let hashed_req = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_req}");

        let key = self.signing_key(&date_stamp, region)?;
        let signature = hex::encode(Self::hmac_sha256(&key, string_to_sign.as_bytes())?);

        let upload_url =
            format!("{endpoint}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}");
        let public_url = format!(
            "{}/{}",
            self.config.public_base_url.trim_end_matches('/'),
            storage_key.trim_start_matches('/')
        );

        Ok(GeneratedPresignedPut {
            upload_url,
            public_url,
            expires_in_secs: PRESIGN_EXPIRES_SECS,
        })
    }

    async fn delete_object(&self, storage_key: &str) -> Result<(), AppError> {
        let endpoint = self.endpoint();
        let parsed = Url::parse(&endpoint)
            .map_err(|e| AppError::InternalServerError(format!("Invalid R2 endpoint URL: {e}")))?;
        let host = parsed.host_str().ok_or_else(|| {
            AppError::InternalServerError("R2 endpoint URL missing host".to_string())
        })?;

        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = "auto";
        let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
        let canonical_uri = format!("/{}/{}", self.config.bucket_name.trim(), storage_key);
        let empty_hash = hex::encode(Sha256::digest(b""));

        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{empty_hash}\nx-amz-date:{amz_date}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "DELETE\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{empty_hash}"
        );
        let hashed_req = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_req}");

        let key = self.signing_key(&date_stamp, region)?;
        let signature = hex::encode(Self::hmac_sha256(&key, string_to_sign.as_bytes())?);
        let auth_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.config.access_key_id
        );

        let url = format!("{endpoint}{canonical_uri}");
        let res = self
            .http
            .delete(&url)
            .header("x-amz-date", &amz_date)
            .header("x-amz-content-sha256", &empty_hash)
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| AppError::InternalServerError(format!("R2 delete request failed: {e}")))?;

        if !res.status().is_success() && res.status().as_u16() != 404 {
            return Err(AppError::InternalServerError(format!(
                "R2 delete object failed with status {}",
                res.status()
            )));
        }
        Ok(())
    }
}

/// Configuration for the local development Object Storage adapter (explicitly gated by `ALLOW_LOCAL_MEDIA_STORAGE=true`).
#[derive(Debug, Clone)]
pub struct LocalMediaConfig {
    pub base_url: String,
    pub storage_dir: PathBuf,
    pub signing_secret: String,
}

/// Local development adapter storing files on disk with HMAC-signed presigned PUT URLs.
#[derive(Debug, Clone)]
pub struct LocalDevObjectStorage {
    config: LocalMediaConfig,
}

impl LocalDevObjectStorage {
    pub fn new(config: LocalMediaConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn storage_dir(&self) -> &PathBuf {
        &self.config.storage_dir
    }

    /// Computes a deterministic HMAC-SHA256 hex signature over `{storage_key}:{exp}`.
    pub fn sign_upload(&self, storage_key: &str, exp: i64) -> Result<String, AppError> {
        let mut mac = HmacSha256::new_from_slice(self.config.signing_secret.as_bytes())
            .map_err(|e| AppError::InternalServerError(format!("HMAC init failed: {e}")))?;
        mac.update(format!("{storage_key}:{exp}").as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    /// Verifies a temporary HMAC-signed local upload URL query (`exp` + `sig`).
    pub fn verify_upload_signature(
        &self,
        storage_key: &str,
        exp: i64,
        sig_hex: &str,
    ) -> Result<(), AppError> {
        if Utc::now().timestamp() > exp {
            return Err(AppError::Unauthorized(
                "Local presigned upload URL has expired".to_string(),
            ));
        }
        let provided = hex::decode(sig_hex.trim())
            .map_err(|_| AppError::Unauthorized("Malformed local upload signature".to_string()))?;
        let mut mac = HmacSha256::new_from_slice(self.config.signing_secret.as_bytes())
            .map_err(|e| AppError::InternalServerError(format!("HMAC init failed: {e}")))?;
        mac.update(format!("{storage_key}:{exp}").as_bytes());
        mac.verify_slice(&provided)
            .map_err(|_| AppError::Unauthorized("Invalid local upload signature".to_string()))
    }

    /// Maps `tenants/{tenant_id}/{context}/{filename}` -> `{base_url}/{tenant_id}/{context}/{filename}`.
    fn public_url_for_key(&self, storage_key: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let rel = storage_key
            .trim_start_matches('/')
            .strip_prefix("tenants/")
            .unwrap_or(storage_key.trim_start_matches('/'));
        format!("{base}/{rel}")
    }
}

#[async_trait]
impl ObjectStoragePort for LocalDevObjectStorage {
    fn provider_name(&self) -> &'static str {
        "LOCAL_DEV"
    }

    async fn generate_presigned_put(
        &self,
        storage_key: &str,
        _mime_type: &str,
        _size_bytes: i64,
    ) -> Result<GeneratedPresignedPut, AppError> {
        let exp = Utc::now().timestamp() + i64::try_from(PRESIGN_EXPIRES_SECS).unwrap_or(900);
        let sig = self.sign_upload(storage_key, exp)?;
        let public_url = self.public_url_for_key(storage_key);
        let upload_url = format!("{public_url}?exp={exp}&sig={sig}");

        Ok(GeneratedPresignedPut {
            upload_url,
            public_url,
            expires_in_secs: PRESIGN_EXPIRES_SECS,
        })
    }

    async fn delete_object(&self, storage_key: &str) -> Result<(), AppError> {
        let rel = storage_key
            .trim_start_matches('/')
            .strip_prefix("tenants/")
            .unwrap_or(storage_key.trim_start_matches('/'));
        let target = self.config.storage_dir.join(rel);
        match tokio::fs::remove_file(&target).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::InternalServerError(format!(
                "Failed to delete local media file: {err}"
            ))),
        }
    }
}
