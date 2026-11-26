use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::Client;
use crate::domain::repositories::{ClientRepository, ClientSearchPage};
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct ClientRow {
    id: Uuid,
    tenant_id: Uuid,
    full_name: String,
    email: Option<String>,
    phone: Option<String>,
    tax_id: Option<String>,
    address: Option<String>,
    notes: Option<String>,
    keycloak_user_id: Option<Uuid>,
    is_active: bool,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<ClientRow> for Client {
    fn from(r: ClientRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            full_name: r.full_name,
            email: r.email,
            phone: r.phone,
            tax_id: r.tax_id,
            address: r.address,
            notes: r.notes,
            keycloak_user_id: r.keycloak_user_id,
            is_active: r.is_active,
            metadata: r.metadata,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

/// PostgreSQL + RLS implementation of `ClientRepository`.
pub struct SqlxClientRepository {
    pool: PgPool,
}

impl SqlxClientRepository {
    /// Creates a new `SqlxClientRepository`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_write_error(context: &str, err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = err {
        if db_err.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "A client with the same email or tax_id already exists in this tenant".to_string(),
            );
        }
    }
    AppError::DatabaseError(format!("{context}: {err}"))
}

#[async_trait]
impl ClientRepository for SqlxClientRepository {
    async fn create(&self, tenant_id: Uuid, client: &Client) -> Result<Client, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            INSERT INTO clients (
                id, tenant_id, full_name, email, phone, tax_id, address, notes,
                keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING id, tenant_id, full_name, email, phone, tax_id, address, notes,
                      keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            "#,
        )
        .bind(client.id)
        .bind(tenant_id)
        .bind(&client.full_name)
        .bind(&client.email)
        .bind(&client.phone)
        .bind(&client.tax_id)
        .bind(&client.address)
        .bind(&client.notes)
        .bind(client.keycloak_user_id)
        .bind(client.is_active)
        .bind(&client.metadata)
        .bind(client.created_at)
        .bind(client.updated_at)
        .bind(client.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_write_error("Failed to insert client", e))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit client transaction: {e}"))
        })?;

        Ok(row.into())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Client>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, tenant_id, full_name, email, phone, tax_id, address, notes,
                   keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            FROM clients
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find client by ID: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_email(
        &self,
        tenant_id: Uuid,
        email: &str,
    ) -> Result<Option<Client>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, tenant_id, full_name, email, phone, tax_id, address, notes,
                   keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            FROM clients
            WHERE email = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(email)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find client by email: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_tax_id(
        &self,
        tenant_id: Uuid,
        tax_id: &str,
    ) -> Result<Option<Client>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, tenant_id, full_name, email, phone, tax_id, address, notes,
                   keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            FROM clients
            WHERE tax_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(tax_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find client by tax_id: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn update(&self, tenant_id: Uuid, client: &Client) -> Result<Client, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            UPDATE clients
            SET full_name = $1, email = $2, phone = $3, tax_id = $4, address = $5,
                notes = $6, keycloak_user_id = $7, is_active = $8, metadata = $9,
                updated_at = NOW()
            WHERE id = $10 AND tenant_id = $11 AND deleted_at IS NULL
            RETURNING id, tenant_id, full_name, email, phone, tax_id, address, notes,
                      keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at
            "#,
        )
        .bind(&client.full_name)
        .bind(&client.email)
        .bind(&client.phone)
        .bind(&client.tax_id)
        .bind(&client.address)
        .bind(&client.notes)
        .bind(client.keycloak_user_id)
        .bind(client.is_active)
        .bind(&client.metadata)
        .bind(client.id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_write_error("Failed to update client", e))?
        .ok_or_else(|| AppError::NotFound(format!("Client with ID '{}' not found", client.id)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let result = sqlx::query(
            r#"
            UPDATE clients
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete client: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<ClientSearchPage, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        // 1. Total count query (defense-in-depth: explicit tenant_id alongside RLS)
        let mut count_qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM clients WHERE tenant_id = ");
        count_qb.push_bind(tenant_id);
        count_qb.push(" AND deleted_at IS NULL");
        if let Some(ref filter) = criteria.filter {
            count_qb.push(" AND ");
            append_filter_group(&mut count_qb, filter)?;
        }

        let total_items: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to count clients: {e}")))?;

        // 2. Paginated data query (defense-in-depth: explicit tenant_id alongside RLS)
        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, tenant_id, full_name, email, phone, tax_id, address, notes, \
             keycloak_user_id, is_active, metadata, created_at, updated_at, deleted_at \
             FROM clients WHERE tenant_id = ",
        );
        data_qb.push_bind(tenant_id);
        data_qb.push(" AND deleted_at IS NULL");
        if let Some(ref filter) = criteria.filter {
            data_qb.push(" AND ");
            append_filter_group(&mut data_qb, filter)?;
        }

        append_order_by(&mut data_qb, criteria.order_by.as_deref().unwrap_or(&[]))?;

        let pagination = criteria.pagination.unwrap_or_default();
        let limit = i64::try_from(pagination.sql_limit()).unwrap_or(20);
        let offset = i64::try_from(pagination.offset()).unwrap_or(0);
        data_qb.push(" LIMIT ");
        data_qb.push_bind(limit);
        data_qb.push(" OFFSET ");
        data_qb.push_bind(offset);

        let rows: Vec<ClientRow> = data_qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to search clients: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(ClientSearchPage {
            items: rows.into_iter().map(Into::into).collect(),
            total_items: u64::try_from(total_items).unwrap_or(0),
        })
    }
}
