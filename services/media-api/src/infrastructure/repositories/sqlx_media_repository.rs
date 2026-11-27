use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::{MediaAsset, MediaContext, MediaStatus, TenantQuotaLimits};
use crate::domain::ports::{
    ConfirmMediaData, CreatePendingMediaData, MediaRepository, UpdateMediaData,
};
use crate::infrastructure::database::{begin_tenant_tx, map_sqlx_error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct MediaRow {
    id: Uuid,
    tenant_id: Uuid,
    storage_provider: String,
    context: String,
    status: String,
    file_name: String,
    storage_key: String,
    public_url: String,
    mime_type: String,
    size_bytes: i64,
    width: Option<i32>,
    height: Option<i32>,
    alt_text: Option<String>,
    metadata: serde_json::Value,
    uploaded_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl TryFrom<MediaRow> for MediaAsset {
    type Error = AppError;

    fn try_from(r: MediaRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: r.id,
            tenant_id: r.tenant_id,
            storage_provider: r.storage_provider,
            context: MediaContext::parse(&r.context)?,
            status: MediaStatus::parse(&r.status)?,
            file_name: r.file_name,
            storage_key: r.storage_key,
            public_url: r.public_url,
            mime_type: r.mime_type,
            size_bytes: r.size_bytes,
            width: r.width,
            height: r.height,
            alt_text: r.alt_text,
            metadata: r.metadata,
            uploaded_by: r.uploaded_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct QuotaRow {
    max_storage_bytes: i64,
    max_file_size_bytes: i64,
}

#[derive(Debug, FromRow)]
struct UsageAggRow {
    used_bytes: i64,
    asset_count: i64,
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
    async fn create_pending(&self, data: CreatePendingMediaData) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            INSERT INTO media_assets (
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes, uploaded_by
            )
            VALUES ($1, $2, $3, $4, 'PENDING', $5, $6, $7, $8, $9, $10)
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(data.id)
        .bind(data.tenant_id)
        .bind(&data.storage_provider)
        .bind(data.context.as_str())
        .bind(&data.file_name)
        .bind(&data.storage_key)
        .bind(&data.public_url)
        .bind(&data.mime_type)
        .bind(data.size_bytes)
        .bind(data.uploaded_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to create pending media asset"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit create_pending: {e}"))
        })?;
        MediaAsset::try_from(row)
    }

    async fn confirm(
        &self,
        tenant_id: Uuid,
        asset_id: Uuid,
        data: ConfirmMediaData,
    ) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            UPDATE media_assets
            SET
                status = 'READY',
                alt_text = COALESCE($3, alt_text),
                width = COALESCE($4, width),
                height = COALESCE($5, height),
                metadata = COALESCE($6, metadata),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(asset_id)
        .bind(tenant_id)
        .bind(&data.alt_text)
        .bind(data.width)
        .bind(data.height)
        .bind(&data.metadata)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to confirm media asset"))?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{asset_id}' not found")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit confirm: {e}")))?;
        MediaAsset::try_from(row)
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<MediaAsset>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            SELECT
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            FROM media_assets
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find media asset"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit find_by_id: {e}")))?;
        row.map(MediaAsset::try_from).transpose()
    }

    async fn find_by_id_including_deleted(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MediaAsset>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            SELECT
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            FROM media_assets
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find media asset (including deleted)"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!(
                "Failed to commit find_by_id_including_deleted: {e}"
            ))
        })?;
        row.map(MediaAsset::try_from).transpose()
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        context: Option<MediaContext>,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<MediaAsset>, u64), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let context_str = context.map(|c| c.as_str());

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM media_assets
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND status = 'READY'
              AND ($2::TEXT IS NULL OR context = $2)
            "#,
        )
        .bind(tenant_id)
        .bind(context_str)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to count media assets"))?;

        let limit = i64::try_from(page_size).unwrap_or(24);
        let offset = i64::try_from(page.saturating_sub(1).saturating_mul(page_size)).unwrap_or(0);

        let rows = sqlx::query_as::<_, MediaRow>(
            r#"
            SELECT
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            FROM media_assets
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND status = 'READY'
              AND ($2::TEXT IS NULL OR context = $2)
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(tenant_id)
        .bind(context_str)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list media assets"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media list: {e}")))?;

        let mut assets = Vec::with_capacity(rows.len());
        for row in rows {
            assets.push(MediaAsset::try_from(row)?);
        }
        Ok((assets, u64::try_from(total).unwrap_or(0)))
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<(Vec<MediaAsset>, u64), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut count_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT COUNT(*)::BIGINT FROM media_assets WHERE deleted_at IS NULL AND status = 'READY' AND tenant_id = ",
        );
        count_qb.push_bind(tenant_id);
        if let Some(ref filters) = criteria.filter {
            count_qb.push(" AND ");
            append_filter_group(&mut count_qb, filters)?;
        }

        let total: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to count searched media assets"))?;

        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            r#"
            SELECT
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            FROM media_assets
            WHERE deleted_at IS NULL AND status = 'READY' AND tenant_id = "#,
        );
        data_qb.push_bind(tenant_id);
        if let Some(ref filters) = criteria.filter {
            data_qb.push(" AND ");
            append_filter_group(&mut data_qb, filters)?;
        }

        let order_by_slice = criteria.order_by.as_deref().unwrap_or(&[]);
        append_order_by(&mut data_qb, order_by_slice)?;

        let pagination = criteria.pagination.unwrap_or_default();
        let limit = i64::try_from(pagination.limit).unwrap_or(24);
        let offset = i64::try_from(pagination.offset()).unwrap_or(0);
        data_qb.push(" LIMIT ");
        data_qb.push_bind(limit);
        data_qb.push(" OFFSET ");
        data_qb.push_bind(offset);

        let rows = data_qb
            .build_query_as::<MediaRow>()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to search media assets"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media search: {e}")))?;

        let mut assets = Vec::with_capacity(rows.len());
        for row in rows {
            assets.push(MediaAsset::try_from(row)?);
        }
        Ok((assets, u64::try_from(total).unwrap_or(0)))
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateMediaData,
    ) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let existing = sqlx::query_as::<_, MediaRow>(
            r#"
            SELECT
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            FROM media_assets
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to load media asset for update"))?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

        let next_alt = data.alt_text.unwrap_or(existing.alt_text);
        let next_ctx = data
            .context
            .map(|c| c.as_str().to_string())
            .unwrap_or(existing.context);
        let next_meta = data.metadata.unwrap_or(existing.metadata);

        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            UPDATE media_assets
            SET alt_text = $3, context = $4, metadata = $5, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&next_alt)
        .bind(&next_ctx)
        .bind(&next_meta)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update media asset"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media update: {e}")))?;
        MediaAsset::try_from(row)
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            UPDATE media_assets
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete media asset"))?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit media soft_delete: {e}"))
        })?;
        MediaAsset::try_from(row)
    }

    async fn restore(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            UPDATE media_assets
            SET deleted_at = NULL, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to restore media asset"))?
        .ok_or_else(|| AppError::NotFound(format!("Deleted media asset '{id}' not found")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit media restore: {e}")))?;
        MediaAsset::try_from(row)
    }

    async fn hard_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<MediaAsset, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, MediaRow>(
            r#"
            DELETE FROM media_assets
            WHERE id = $1 AND tenant_id = $2
            RETURNING
                id, tenant_id, storage_provider, context, status,
                file_name, storage_key, public_url, mime_type, size_bytes,
                width, height, alt_text, metadata, uploaded_by,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to permanently delete media asset"))?
        .ok_or_else(|| AppError::NotFound(format!("Media asset '{id}' not found")))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit media hard_delete: {e}"))
        })?;
        MediaAsset::try_from(row)
    }

    async fn get_usage(
        &self,
        tenant_id: Uuid,
        defaults: TenantQuotaLimits,
    ) -> Result<(i64, i64, TenantQuotaLimits), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let agg = sqlx::query_as::<_, UsageAggRow>(
            r#"
            SELECT
                COALESCE(SUM(size_bytes), 0)::BIGINT AS used_bytes,
                COUNT(*)::BIGINT AS asset_count
            FROM media_assets
            WHERE tenant_id = $1 AND deleted_at IS NULL AND status = 'READY'
            "#,
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to compute tenant media usage"))?;

        let custom_quota = sqlx::query_as::<_, QuotaRow>(
            r#"
            SELECT max_storage_bytes, max_file_size_bytes
            FROM tenant_quotas
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to read tenant quota"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit get_usage: {e}")))?;

        let limits = match custom_quota {
            Some(q) => TenantQuotaLimits {
                max_storage_bytes: q.max_storage_bytes,
                max_file_size_bytes: q.max_file_size_bytes,
            },
            None => defaults,
        };

        Ok((agg.used_bytes, agg.asset_count, limits))
    }
}
