use ikeda_core::observability::{http_trace_layer, init_tracing, request_id_layer};
use media_api::domain::models::TenantQuotaLimits;
use media_api::domain::ports::ObjectStoragePort;
use media_api::domain::services::{DEFAULT_MAX_FILE_SIZE_BYTES, DEFAULT_MAX_STORAGE_BYTES};
use media_api::{infrastructure, presentation};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    init_tracing("media-api");

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8087);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "Initializing IkedaDev Media API...");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/media_db?sslmode=disable"
            .to_string()
    });
    let max_connections: u32 = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let min_connections: u32 = std::env::var("DATABASE_MIN_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    let pool = infrastructure::create_pool(&infrastructure::DatabaseConfig {
        url: database_url,
        max_connections,
        min_connections,
        ..Default::default()
    })
    .await?;
    tracing::info!("PostgreSQL connection pool established for media_db with RLS support.");

    let media_repo = Arc::new(infrastructure::SqlxMediaRepository::new(pool.clone()));

    // Dedicated lightweight pool for centralized API key validation against identity_db
    let identity_pool = match std::env::var("IDENTITY_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let id_pool = infrastructure::create_pool(&infrastructure::DatabaseConfig {
                url,
                max_connections: 3,
                min_connections: 1,
                ..Default::default()
            })
            .await?;
            tracing::info!(
                "Dedicated identity_db connection pool established for API key validation."
            );
            id_pool
        }
        _ => {
            tracing::warn!(
                "IDENTITY_DATABASE_URL not set; falling back to primary DATABASE_URL for API key validation."
            );
            pool
        }
    };

    let api_key_validator = Arc::new(ikeda_core::tenant::CachedApiKeyValidator::new(
        infrastructure::SqlxApiKeyValidator::new(identity_pool),
    ));

    let default_quotas = TenantQuotaLimits {
        max_storage_bytes: std::env::var("DEFAULT_MAX_STORAGE_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_MAX_STORAGE_BYTES),
        max_file_size_bytes: std::env::var("DEFAULT_MAX_FILE_SIZE_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_MAX_FILE_SIZE_BYTES),
    };

    // ObjectStoragePort resolution:
    // 1. If Cloudflare R2 credentials are present -> use R2S3ObjectStorage (production & QA).
    // 2. Otherwise -> require explicit ALLOW_LOCAL_MEDIA_STORAGE=true (fail fast in QA/Prod).
    let allow_local = std::env::var("ALLOW_LOCAL_MEDIA_STORAGE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let (object_storage, local_storage): (
        Arc<dyn ObjectStoragePort>,
        Option<Arc<infrastructure::LocalDevObjectStorage>>,
    ) = match (
        std::env::var("R2_ACCOUNT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("R2_BUCKET_NAME")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("R2_ACCESS_KEY_ID")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("R2_SECRET_ACCESS_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("R2_PUBLIC_BASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    ) {
        (
            Some(account_id),
            Some(bucket_name),
            Some(access_key_id),
            Some(secret_access_key),
            Some(public_base_url),
        ) => {
            tracing::info!(bucket = %bucket_name, "Cloudflare R2 ObjectStoragePort enabled (R2_PLATFORM).");
            (
                Arc::new(infrastructure::R2S3ObjectStorage::new(
                    infrastructure::R2Config {
                        account_id,
                        bucket_name,
                        access_key_id,
                        secret_access_key,
                        public_base_url,
                        endpoint_url: std::env::var("R2_ENDPOINT_URL").ok(),
                    },
                )),
                None,
            )
        }
        _ if allow_local => {
            let base_url = std::env::var("LOCAL_MEDIA_BASE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("http://localhost:{port}/v1/media/local"));
            let storage_dir = PathBuf::from(
                std::env::var("LOCAL_MEDIA_DIR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "/tmp/ikeda-media-local".to_string()),
            );
            let signing_secret = std::env::var("LOCAL_MEDIA_SIGNING_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "dev-local-media-hmac-secret-change-me".to_string());

            tracing::warn!(
                %base_url,
                dir = %storage_dir.display(),
                "ALLOW_LOCAL_MEDIA_STORAGE=true: LocalDevObjectStorage active for development."
            );
            let local = Arc::new(infrastructure::LocalDevObjectStorage::new(
                infrastructure::LocalMediaConfig {
                    base_url,
                    storage_dir,
                    signing_secret,
                },
            ));
            (local.clone(), Some(local))
        }
        _ => {
            return Err(
                "Object storage is not configured: set R2_* credentials or explicitly set ALLOW_LOCAL_MEDIA_STORAGE=true in local dev"
                    .into(),
            );
        }
    };

    let mut state = presentation::AppState::new(media_repo, object_storage, api_key_validator)
        .with_default_quotas(default_quotas);

    if let Some(local) = local_storage {
        let local_cors_origin = std::env::var("LOCAL_CORS_ORIGIN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "http://localhost:5173".to_string());
        state = state.with_local_storage(local, local_cors_origin);
    }

    // Optional Keycloak / OIDC JWKS JWT validation
    let jwks_url = std::env::var("JWKS_URI").or_else(|_| std::env::var("JWKS_URL"));
    if let (Ok(jwks_uri), Ok(jwt_issuer)) = (jwks_url, std::env::var("JWT_ISSUER")) {
        if !jwks_uri.trim().is_empty() && !jwt_issuer.trim().is_empty() {
            let mut jwks_config = ikeda_core::tenant::JwksConfig::new(jwks_uri, jwt_issuer);
            if let Ok(aud) = std::env::var("JWT_AUDIENCE") {
                if !aud.trim().is_empty() {
                    jwks_config = jwks_config.with_audience(aud);
                }
            }
            let jwt_validator = ikeda_core::tenant::JwksJwtValidator::new(jwks_config)?;
            state = state.with_jwt_validator(Arc::new(jwt_validator));
            tracing::info!("Keycloak/OIDC JWKS JWT validator enabled for media-api.");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = presentation::create_router(state)
        .layer(cors)
        .layer(http_trace_layer("media-api"))
        .layer(request_id_layer());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Media API listening. Swagger UI at http://{addr}/swagger-ui");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Media API shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "Failed to install Ctrl+C signal handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to install SIGTERM signal handler");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
