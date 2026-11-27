use crate::domain::models::MediaAsset;
use crate::domain::ports::{CreateMediaData, MediaRepository};
use crate::infrastructure::database::{begin_tenant_tx, map_sqlx_error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct MediaRow {
    id: Uuid,
    tenant_id: Uuid,
    storage_provider: String,
    file_name: String,
    storage_key: String,
    public_url: String,
    mime_type: String,
    size_bytes: i64,
    alt_text: Option<String>,
    uploaded_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<MediaRow> for MediaAsset {
    fn from(r: MediaRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            storage_provider: r.storage_provider,
            file_name: r.file_name,
            storage_key: r.storage_key,
            public_url: r.public_url,
            mime_type: r.mime_type,
            size_bytes: r.size_bytes,
            alt_text: r.alt_text,
            uploaded_by: r.uploaded_by,
            created_at: r.created_at,
            deleted_at: r.deleted_at,
        }
    }
}

pub struct SqlxMediaRepository {
    pool: PgPool,
}

impl SqlxMediaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MediaRepository for SqlxMediaRepository {
    async fn create(&self, data: CreateMediaData) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            INSERT INTO media_assets (
                id, tenant_id, storage_provider, file_name, storage_key,
                public_url, mime_type, size_bytes, alt_text, uploaded_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                id, tenant_id, storage_provider, file_name, storage_key,
                public_url, mime_type, size_bytes, alt_text, uploaded_by,
                created_at, deleted_at
            "#,
        )
        .bind(data.id)
        .bind(data.tenant_id)
        .bind(&data.storage_provider)
        .bind(&data.file_name)
        .bind(&data.storage_key)
        .bind(&data.public_url)
        .bind(&data.mime_type)
        .bind(data.size_bytes)
        .bind(&data.alt_text)
        .bind(data.uploaded_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to register media asset"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media create: {e}")))?;
        Ok(MediaAsset::from(row))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<MediaAsset>, u64), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM media_assets WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to count media assets"))?;

        let limit = i64::try_from(page_size).unwrap_or(24);
        let offset = i64::try_from(page.saturating_sub(1).saturating_mul(page_size)).unwrap_or(0);

        let rows = sqlx::query_as::<_, MediaRow>(
            r#"
            SELECT
                id, tenant_id, storage_provider, file_name, storage_key,
                public_url, mime_type, size_bytes, alt_text, uploaded_by,
                created_at, deleted_at
            FROM media_assets
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list media assets"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media list: {e}")))?;
        Ok((
            rows.into_iter().map(MediaAsset::from).collect(),
            u64::try_from(total).unwrap_or(0),
        ))
    }

    async fn update_alt_text(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        alt_text: Option<String>,
    ) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            UPDATE media_assets
            SET alt_text = $3
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, storage_provider, file_name, storage_key,
                public_url, mime_type, size_bytes, alt_text, uploaded_by,
                created_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&alt_text)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update media asset"))?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media update: {e}")))?;
        Ok(MediaAsset::from(row))
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let res = sqlx::query(
            r#"
            UPDATE media_assets
            SET deleted_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete media asset"))?;

        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Media asset '{id}' not found")));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media delete: {e}")))?;
        Ok(())
    }
}
