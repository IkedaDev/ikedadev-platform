use crate::domain::models::PublishWebhook;
use crate::domain::ports::{CreateWebhookData, UpdateWebhookData, WebhookRepository};
use crate::infrastructure::database::{begin_tenant_tx, map_sqlx_error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct WebhookRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    target_url: String,
    events: Vec<String>,
    is_active: bool,
    last_triggered_at: Option<DateTime<Utc>>,
    last_status_code: Option<i16>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<WebhookRow> for PublishWebhook {
    fn from(r: WebhookRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            target_url: r.target_url,
            events: r.events,
            is_active: r.is_active,
            last_triggered_at: r.last_triggered_at,
            last_status_code: r.last_status_code,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

pub struct SqlxWebhookRepository {
    pool: PgPool,
}

impl SqlxWebhookRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookRepository for SqlxWebhookRepository {
    async fn create(&self, data: CreateWebhookData) -> Result<PublishWebhook, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let row = sqlx::query_as::<_, WebhookRow>(
            r#"
            INSERT INTO publish_webhooks (tenant_id, name, target_url, events, is_active)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(&data.name)
        .bind(&data.target_url)
        .bind(&data.events)
        .bind(data.is_active)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to create publish webhook"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit create webhook: {e}"))
        })?;
        Ok(PublishWebhook::from(row))
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<PublishWebhook>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let rows = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            FROM publish_webhooks
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list publish webhooks"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit list webhooks: {e}")))?;
        Ok(rows.into_iter().map(PublishWebhook::from).collect())
    }

    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PublishWebhook>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            FROM publish_webhooks
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find publish webhook"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit find webhook: {e}")))?;
        Ok(row.map(PublishWebhook::from))
    }

    async fn find_active_for_event(
        &self,
        tenant_id: Uuid,
        event_name: &str,
    ) -> Result<Vec<PublishWebhook>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let rows = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            FROM publish_webhooks
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND is_active = true
              AND $2 = ANY(events)
            "#,
        )
        .bind(tenant_id)
        .bind(event_name)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to query active publish webhooks"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit active webhooks query: {e}"))
        })?;
        Ok(rows.into_iter().map(PublishWebhook::from).collect())
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateWebhookData,
    ) -> Result<PublishWebhook, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let existing = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            FROM publish_webhooks
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to load publish webhook"))?
        .ok_or_else(|| AppError::NotFound(format!("Webhook '{id}' not found")))?;

        let row = sqlx::query_as::<_, WebhookRow>(
            r#"
            UPDATE publish_webhooks
            SET name = $3, target_url = $4, events = $5, is_active = $6, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, name, target_url, events, is_active,
                last_triggered_at, last_status_code, created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(data.name.unwrap_or(existing.name))
        .bind(data.target_url.unwrap_or(existing.target_url))
        .bind(data.events.unwrap_or(existing.events))
        .bind(data.is_active.unwrap_or(existing.is_active))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update publish webhook"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit update webhook: {e}"))
        })?;
        Ok(PublishWebhook::from(row))
    }

    async fn record_delivery(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status_code: i16,
    ) -> Result<(), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        sqlx::query(
            r#"
            UPDATE publish_webhooks
            SET last_triggered_at = NOW(), last_status_code = $3, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(status_code)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to record webhook delivery"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit record_delivery: {e}"))
        })?;
        Ok(())
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let res = sqlx::query(
            r#"
            UPDATE publish_webhooks
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete publish webhook"))?;

        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Webhook '{id}' not found")));
        }

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit delete webhook: {e}"))
        })?;
        Ok(())
    }
}
