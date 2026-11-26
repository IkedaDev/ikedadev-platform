use identity_api::domain::services::IdentityProviderPort;
use identity_api::{infrastructure, presentation};
use ikeda_core::observability::{http_trace_layer, init_tracing, request_id_layer};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    init_tracing("identity-api");

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8082);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "Initializing IkedaDev Identity API...");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres_password@localhost:5432/identity_db?sslmode=disable"
            .to_string()
    });

    let db_config = infrastructure::DatabaseConfig {
        url: database_url,
        ..Default::default()
    };

    let pool = infrastructure::create_pool(&db_config).await?;
    tracing::info!("PostgreSQL connection pool established for identity_db with RLS support.");

    let tenant_repo = Arc::new(infrastructure::SqlxTenantRepository::new(pool.clone()));
    let user_repo = Arc::new(infrastructure::SqlxTenantUserRepository::new(pool.clone()));
    let api_key_repo = Arc::new(infrastructure::SqlxApiKeyRepository::new(pool.clone()));
    let api_key_validator = Arc::new(ikeda_core::tenant::CachedApiKeyValidator::new(
        infrastructure::SqlxApiKeyValidator::new(pool),
    ));

    let idp: Arc<dyn IdentityProviderPort> = match (
        std::env::var("KEYCLOAK_BASE_URL"),
        std::env::var("KEYCLOAK_REALM"),
        std::env::var("KEYCLOAK_CLIENT_ID"),
        std::env::var("KEYCLOAK_CLIENT_SECRET"),
    ) {
        (Ok(base_url), Ok(realm), Ok(client_id), Ok(client_secret))
            if !base_url.trim().is_empty() && !client_secret.trim().is_empty() =>
        {
            tracing::info!(%base_url, %realm, "Configuring Keycloak Admin REST API client.");
            let kc =
                infrastructure::KeycloakAdminClient::new(infrastructure::KeycloakAdminConfig {
                    base_url,
                    realm,
                    client_id,
                    client_secret,
                })?;
            Arc::new(kc)
        }
        _ => {
            tracing::warn!(
                "KEYCLOAK_* env vars not fully configured; using in-memory MockIdentityProvider."
            );
            Arc::new(infrastructure::MockIdentityProvider::default())
        }
    };

    let mut state =
        presentation::AppState::new(tenant_repo, user_repo, api_key_repo, idp, api_key_validator);

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
            tracing::info!("Keycloak/OIDC JWKS JWT validator enabled for identity-api.");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = presentation::create_router(state)
        .layer(cors)
        .layer(http_trace_layer("identity-api"))
        .layer(request_id_layer());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Identity API listening. Swagger UI at http://{addr}/swagger-ui");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Identity API shut down gracefully.");
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
