use content_api::application::preview_token::PreviewTokenService;
use content_api::domain::ports::{ObjectStoragePort, SchedulerPort};
use content_api::{infrastructure, presentation};
use ikeda_core::observability::{http_trace_layer, init_tracing, request_id_layer};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    init_tracing("content-api");

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8086);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "Initializing IkedaDev Content API (Headless CMS)...");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://ikeda_app:ikeda_app_password@localhost:5432/content_db?sslmode=disable"
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

    let db_config = infrastructure::DatabaseConfig {
        url: database_url,
        max_connections,
        min_connections,
        ..Default::default()
    };

    let pool = infrastructure::create_pool(&db_config).await?;
    tracing::info!("PostgreSQL connection pool established for content_db with RLS support.");

    let article_repo = Arc::new(infrastructure::SqlxArticleRepository::new(pool.clone()));
    let taxonomy_repo = Arc::new(infrastructure::SqlxTaxonomyRepository::new(pool.clone()));
    let media_repo = Arc::new(infrastructure::SqlxMediaRepository::new(pool.clone()));
    let webhook_repo = Arc::new(infrastructure::SqlxWebhookRepository::new(pool.clone()));

    // Dedicated lightweight pool for centralized API key validation against identity_db
    let identity_pool = match std::env::var("IDENTITY_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let identity_db_config = infrastructure::DatabaseConfig {
                url,
                max_connections: 3,
                min_connections: 1,
                ..Default::default()
            };
            let id_pool = infrastructure::create_pool(&identity_db_config).await?;
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

    // Object Storage adapter (Cloudflare R2 S3-compatible or local localhost dev fallback)
    let (object_storage, local_media_dir): (
        Arc<dyn ObjectStoragePort>,
        Option<std::path::PathBuf>,
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
        _ => {
            let local_base = std::env::var("LOCAL_MEDIA_BASE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("http://localhost:{port}/v1/media/local"));
            let dir = std::path::PathBuf::from(
                std::env::var("LOCAL_MEDIA_DIR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "/tmp/ikeda-content-media".to_string()),
            );
            tracing::warn!(
                %local_base,
                local_dir = %dir.display(),
                "R2 credentials not configured; using local dev media storage on localhost."
            );
            (
                Arc::new(infrastructure::NoopObjectStorage::new(local_base)),
                Some(dir),
            )
        }
    };

    // Scheduler adapter (Upstash QStash or No-op local dev fallback)
    let scheduler: Arc<dyn SchedulerPort> = match (
        std::env::var("QSTASH_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("QSTASH_CURRENT_SIGNING_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        std::env::var("PUBLIC_BASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    ) {
        (Some(token), Some(current_signing_key), Some(public_base_url)) => {
            let callback_url = format!(
                "{}/v1/internal/qstash/publish",
                public_base_url.trim_end_matches('/')
            );
            tracing::info!(%callback_url, "Upstash QStash SchedulerPort enabled.");
            Arc::new(infrastructure::QStashScheduler::new(
                reqwest::Client::new(),
                infrastructure::QStashConfig {
                    base_url: std::env::var("QSTASH_BASE_URL")
                        .unwrap_or_else(|_| "https://qstash.upstash.io/v2/publish".to_string()),
                    token,
                    current_signing_key,
                    next_signing_key: std::env::var("QSTASH_NEXT_SIGNING_KEY").ok(),
                    callback_url,
                },
            ))
        }
        _ => {
            tracing::warn!("QSTASH_TOKEN not configured; using NoopScheduler adapter.");
            let dev_key = std::env::var("QSTASH_CURRENT_SIGNING_KEY")
                .unwrap_or_else(|_| "dev-qstash-signing-key".to_string());
            Arc::new(infrastructure::NoopScheduler::new(dev_key))
        }
    };

    let preview_secret = std::env::var("PREVIEW_TOKEN_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "dev-preview-token-secret-change-in-production".to_string());
    let allow_http_urls = std::env::var("ALLOW_INSECURE_HTTP_URLS")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let mut state = presentation::AppState::new(
        article_repo,
        taxonomy_repo,
        media_repo,
        webhook_repo,
        object_storage,
        scheduler,
        PreviewTokenService::new(preview_secret),
        api_key_validator,
    )
    .with_allow_http_urls(allow_http_urls);

    if let Some(dir) = local_media_dir {
        state = state.with_local_media_dir(dir);
    }

    // Optional Keycloak / OIDC JWKS JWT validation (accepts JWKS_URI or JWKS_URL)
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
            tracing::info!("Keycloak/OIDC JWKS JWT validator enabled for content-api.");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = presentation::create_router(state)
        .layer(cors)
        .layer(http_trace_layer("content-api"))
        .layer(request_id_layer());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Content API listening. Swagger UI at http://{addr}/swagger-ui");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Content API shut down gracefully.");
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
