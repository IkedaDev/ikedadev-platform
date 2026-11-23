use ikeda_core::error::AppError;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

/// Configuration for the PostgreSQL connection pool.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URI.
    pub url: String,
    /// Maximum active connections in pool.
    pub max_connections: u32,
    /// Minimum idle connections kept open.
    pub min_connections: u32,
    /// Timeout when acquiring a connection from pool.
    pub acquire_timeout_secs: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_secs: 10,
        }
    }
}

/// Initializes a verified PostgreSQL connection pool.
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool, AppError> {
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .connect(&config.url)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to connect to PostgreSQL: {e}")))
}

/// Begins a PostgreSQL transaction and enforces multi-tenant Row-Level Security (RLS)
/// by binding `app.current_tenant` locally to the transaction.
pub async fn begin_tenant_tx(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Transaction<'_, Postgres>, AppError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to begin transaction: {e}")))?;

    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::DatabaseError(format!("Failed to set tenant RLS context on tx: {e}"))
        })?;

    Ok(tx)
}
