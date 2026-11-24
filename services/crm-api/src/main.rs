use crm_api::{infrastructure, presentation};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,crm_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "Initializing IkedaDev CRM API...");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres_password@localhost:5432/crm_db?sslmode=disable".to_string()
    });

    let db_config = infrastructure::DatabaseConfig {
        url: database_url,
        ..Default::default()
    };

    let pool = infrastructure::create_pool(&db_config).await?;
    tracing::info!("PostgreSQL connection pool established for crm_db with RLS support.");

    let client_repo = Arc::new(infrastructure::SqlxClientRepository::new(pool.clone()));

    // Dedicated lightweight pool for centralized API Key validation against identity_db
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
    let mut state = presentation::AppState::new(client_repo, api_key_validator);

    let jwks_uri_env = std::env::var("JWKS_URI").or_else(|_| std::env::var("JWKS_URL"));
    if let (Ok(jwks_uri), Ok(jwt_issuer)) = (jwks_uri_env, std::env::var("JWT_ISSUER")) {
        if !jwks_uri.trim().is_empty() && !jwt_issuer.trim().is_empty() {
            let mut jwks_config = ikeda_core::tenant::JwksConfig::new(jwks_uri, jwt_issuer);
            if let Ok(aud) = std::env::var("JWT_AUDIENCE") {
                if !aud.trim().is_empty() {
                    jwks_config = jwks_config.with_audience(aud);
                }
            }
            let jwt_validator = ikeda_core::tenant::JwksJwtValidator::new(jwks_config)?;
            state = state.with_jwt_validator(Arc::new(jwt_validator));
            tracing::info!("Keycloak/OIDC JWKS JWT validator enabled for crm-api.");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = presentation::create_router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "CRM API listening. Swagger UI at http://{addr}/swagger-ui");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("CRM API shut down gracefully.");
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
