use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::{Booking, BookingItem, BookingStatus};
use crate::domain::repositories::{BookingRepository, BookingSearchPage, IdempotentCreateResult};
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::ApiResponse;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct BookingRow {
    id: Uuid,
    tenant_id: Uuid,
    booking_code: String,
    client_id: Uuid,
    calendar_id: Option<Uuid>,
    event_id: Option<Uuid>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    timezone: String,
    status: String,
    subtotal: i64,
    tax_total: i64,
    total_price: i64,
    total_duration_minutes: i32,
    currency: String,
    notes: Option<String>,
    cancellation_reason: Option<String>,
    cancelled_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    metadata: serde_json::Value,
    created_by: Option<Uuid>,
    updated_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct BookingItemRow {
    id: Uuid,
    tenant_id: Uuid,
    booking_id: Uuid,
    catalog_item_id: Uuid,
    variant_id: Option<Uuid>,
    name_snapshot: String,
    sku_snapshot: Option<String>,
    quantity: i32,
    unit_price_snapshot: i64,
    tax_rate_bps: i32,
    tax_included: bool,
    line_subtotal: i64,
    line_tax: i64,
    line_total: i64,
    duration_minutes_snapshot: i32,
    buffer_minutes_snapshot: i32,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct IdempotencyRow {
    request_hash: String,
    response_status: i16,
    response_body: serde_json::Value,
    expires_at: DateTime<Utc>,
}

impl From<BookingItemRow> for BookingItem {
    fn from(r: BookingItemRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            booking_id: r.booking_id,
            catalog_item_id: r.catalog_item_id,
            variant_id: r.variant_id,
            name_snapshot: r.name_snapshot,
            sku_snapshot: r.sku_snapshot,
            quantity: r.quantity,
            unit_price_snapshot: r.unit_price_snapshot,
            tax_rate_bps: r.tax_rate_bps,
            tax_included: r.tax_included,
            line_subtotal: r.line_subtotal,
            line_tax: r.line_tax,
            line_total: r.line_total,
            duration_minutes_snapshot: r.duration_minutes_snapshot,
            buffer_minutes_snapshot: r.buffer_minutes_snapshot,
            metadata: r.metadata,
            created_at: r.created_at,
        }
    }
}

fn row_to_booking(row: BookingRow, items: Vec<BookingItem>) -> Result<Booking, AppError> {
    let status = BookingStatus::parse_str(&row.status)?;
    Ok(Booking {
        id: row.id,
        tenant_id: row.tenant_id,
        booking_code: row.booking_code,
        client_id: row.client_id,
        calendar_id: row.calendar_id,
        event_id: row.event_id,
        starts_at: row.starts_at,
        ends_at: row.ends_at,
        timezone: row.timezone,
        status,
        subtotal: row.subtotal,
        tax_total: row.tax_total,
        total_price: row.total_price,
        total_duration_minutes: row.total_duration_minutes,
        currency: row.currency,
        notes: row.notes,
        cancellation_reason: row.cancellation_reason,
        cancelled_at: row.cancelled_at,
        completed_at: row.completed_at,
        metadata: row.metadata,
        created_by: row.created_by,
        updated_by: row.updated_by,
        items,
        created_at: row.created_at,
        updated_at: row.updated_at,
        deleted_at: row.deleted_at,
    })
}

async fn hydrate_bookings(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    rows: Vec<BookingRow>,
) -> Result<Vec<Booking>, AppError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let booking_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let item_rows = sqlx::query_as::<_, BookingItemRow>(
        r#"
        SELECT id, tenant_id, booking_id, catalog_item_id, variant_id,
               name_snapshot, sku_snapshot, quantity, unit_price_snapshot,
               tax_rate_bps, tax_included, line_subtotal, line_tax, line_total,
               duration_minutes_snapshot, buffer_minutes_snapshot, metadata, created_at
        FROM booking_items
        WHERE tenant_id = $1 AND booking_id = ANY($2)
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(&booking_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::DatabaseError(format!("Failed to batch-load booking items: {e}")))?;

    let mut items_map: HashMap<Uuid, Vec<BookingItem>> = HashMap::new();
    for item in item_rows {
        items_map
            .entry(item.booking_id)
            .or_default()
            .push(item.into());
    }

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let items = items_map.remove(&id).unwrap_or_default();
        result.push(row_to_booking(row, items)?);
    }

    Ok(result)
}

async fn insert_booking_items(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    booking_id: Uuid,
    items: &[BookingItem],
) -> Result<Vec<BookingItem>, AppError> {
    let mut inserted = Vec::with_capacity(items.len());
    for item in items {
        let row = sqlx::query_as::<_, BookingItemRow>(
            r#"
            INSERT INTO booking_items (
                id, tenant_id, booking_id, catalog_item_id, variant_id,
                name_snapshot, sku_snapshot, quantity, unit_price_snapshot,
                tax_rate_bps, tax_included, line_subtotal, line_tax, line_total,
                duration_minutes_snapshot, buffer_minutes_snapshot, metadata, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13, $14,
                $15, $16, $17, $18
            )
            RETURNING id, tenant_id, booking_id, catalog_item_id, variant_id,
                      name_snapshot, sku_snapshot, quantity, unit_price_snapshot,
                      tax_rate_bps, tax_included, line_subtotal, line_tax, line_total,
                      duration_minutes_snapshot, buffer_minutes_snapshot, metadata, created_at
            "#,
        )
        .bind(item.id)
        .bind(tenant_id)
        .bind(booking_id)
        .bind(item.catalog_item_id)
        .bind(item.variant_id)
        .bind(&item.name_snapshot)
        .bind(&item.sku_snapshot)
        .bind(item.quantity)
        .bind(item.unit_price_snapshot)
        .bind(item.tax_rate_bps)
        .bind(item.tax_included)
        .bind(item.line_subtotal)
        .bind(item.line_tax)
        .bind(item.line_total)
        .bind(item.duration_minutes_snapshot)
        .bind(item.buffer_minutes_snapshot)
        .bind(&item.metadata)
        .bind(item.created_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to insert booking item: {e}")))?;

        inserted.push(row.into());
    }
    Ok(inserted)
}

fn map_booking_write_err(ctx: &str, err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = err {
        if db_err.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "A booking with the same booking_code or idempotency key already exists"
                    .to_string(),
            );
        }
    }
    AppError::DatabaseError(format!("{ctx}: {err}"))
}

/// PostgreSQL + RLS implementation of `BookingRepository`.
pub struct SqlxBookingRepository {
    pool: PgPool,
}

impl SqlxBookingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BookingRepository for SqlxBookingRepository {
    async fn create_with_idempotency(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        idempotency: Option<(&str, &str, DateTime<Utc>)>,
    ) -> Result<IdempotentCreateResult, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        if let Some((idem_key, req_hash, _)) = idempotency {
            let existing = sqlx::query_as::<_, IdempotencyRow>(
                r#"
                SELECT request_hash, response_status, response_body, expires_at
                FROM idempotency_keys
                WHERE tenant_id = $1 AND idempotency_key = $2
                FOR UPDATE
                "#,
            )
            .bind(tenant_id)
            .bind(idem_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Idempotency lookup failed: {e}")))?;

            if let Some(record) = existing {
                if record.expires_at > Utc::now() {
                    if record.request_hash == req_hash {
                        tx.commit().await.map_err(|e| {
                            AppError::DatabaseError(format!("Transaction commit failed: {e}"))
                        })?;
                        return Ok(IdempotentCreateResult::Replayed {
                            status: record.response_status,
                            body: record.response_body,
                        });
                    }
                    return Err(AppError::Conflict(
                        "Idempotency-Key was already used with a different request payload"
                            .to_string(),
                    ));
                }

                // Purge expired idempotency entry before re-using key
                sqlx::query(
                    "DELETE FROM idempotency_keys WHERE tenant_id = $1 AND idempotency_key = $2",
                )
                .bind(tenant_id)
                .bind(idem_key)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AppError::DatabaseError(format!("Failed to purge expired idempotency: {e}"))
                })?;
            }
        }

        let row = sqlx::query_as::<_, BookingRow>(
            r#"
            INSERT INTO bookings (
                id, tenant_id, booking_code, client_id, calendar_id, event_id,
                starts_at, ends_at, timezone, status,
                subtotal, tax_total, total_price, total_duration_minutes, currency,
                notes, cancellation_reason, cancelled_at, completed_at,
                metadata, created_by, updated_by, created_at, updated_at, deleted_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13, $14, $15,
                $16, $17, $18, $19,
                $20, $21, $22, $23, $24, $25
            )
            RETURNING id, tenant_id, booking_code, client_id, calendar_id, event_id,
                      starts_at, ends_at, timezone, status,
                      subtotal, tax_total, total_price, total_duration_minutes, currency,
                      notes, cancellation_reason, cancelled_at, completed_at,
                      metadata, created_by, updated_by, created_at, updated_at, deleted_at
            "#,
        )
        .bind(booking.id)
        .bind(tenant_id)
        .bind(&booking.booking_code)
        .bind(booking.client_id)
        .bind(booking.calendar_id)
        .bind(booking.event_id)
        .bind(booking.starts_at)
        .bind(booking.ends_at)
        .bind(&booking.timezone)
        .bind(booking.status.as_str())
        .bind(booking.subtotal)
        .bind(booking.tax_total)
        .bind(booking.total_price)
        .bind(booking.total_duration_minutes)
        .bind(&booking.currency)
        .bind(&booking.notes)
        .bind(&booking.cancellation_reason)
        .bind(booking.cancelled_at)
        .bind(booking.completed_at)
        .bind(&booking.metadata)
        .bind(booking.created_by)
        .bind(booking.updated_by)
        .bind(booking.created_at)
        .bind(booking.updated_at)
        .bind(booking.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_booking_write_err("Failed to insert booking", e))?;

        let items = insert_booking_items(&mut tx, tenant_id, row.id, &booking.items).await?;
        let created = row_to_booking(row, items)?;

        if let Some((idem_key, req_hash, expires_at)) = idempotency {
            let cached_body = serde_json::to_value(ApiResponse::created(
                created.clone(),
                "Booking created successfully",
            ))
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to serialize idempotency response: {e}"
                ))
            })?;

            sqlx::query(
                r#"
                INSERT INTO idempotency_keys (
                    tenant_id, idempotency_key, request_hash,
                    response_status, response_body, expires_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(tenant_id)
            .bind(idem_key)
            .bind(req_hash)
            .bind(201_i16)
            .bind(cached_body)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| map_booking_write_err("Failed to store idempotency key", e))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(IdempotentCreateResult::Created(Box::new(created)))
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Booking>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, BookingRow>(
            r#"
            SELECT id, tenant_id, booking_code, client_id, calendar_id, event_id,
                   starts_at, ends_at, timezone, status,
                   subtotal, tax_total, total_price, total_duration_minutes, currency,
                   notes, cancellation_reason, cancelled_at, completed_at,
                   metadata, created_by, updated_by, created_at, updated_at, deleted_at
            FROM bookings
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find booking: {e}")))?;

        let hydrated = match row {
            Some(r) => hydrate_bookings(&mut tx, tenant_id, vec![r])
                .await?
                .into_iter()
                .next(),
            None => None,
        };

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(hydrated)
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        booking: &Booking,
        replace_items: bool,
    ) -> Result<Booking, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, BookingRow>(
            r#"
            UPDATE bookings
            SET calendar_id = $1, event_id = $2, starts_at = $3, ends_at = $4,
                timezone = $5, status = $6, subtotal = $7, tax_total = $8,
                total_price = $9, total_duration_minutes = $10, notes = $11,
                cancellation_reason = $12, cancelled_at = $13, completed_at = $14,
                metadata = $15, updated_by = $16, updated_at = NOW()
            WHERE id = $17 AND tenant_id = $18 AND deleted_at IS NULL
            RETURNING id, tenant_id, booking_code, client_id, calendar_id, event_id,
                      starts_at, ends_at, timezone, status,
                      subtotal, tax_total, total_price, total_duration_minutes, currency,
                      notes, cancellation_reason, cancelled_at, completed_at,
                      metadata, created_by, updated_by, created_at, updated_at, deleted_at
            "#,
        )
        .bind(booking.calendar_id)
        .bind(booking.event_id)
        .bind(booking.starts_at)
        .bind(booking.ends_at)
        .bind(&booking.timezone)
        .bind(booking.status.as_str())
        .bind(booking.subtotal)
        .bind(booking.tax_total)
        .bind(booking.total_price)
        .bind(booking.total_duration_minutes)
        .bind(&booking.notes)
        .bind(&booking.cancellation_reason)
        .bind(booking.cancelled_at)
        .bind(booking.completed_at)
        .bind(&booking.metadata)
        .bind(booking.updated_by)
        .bind(booking.id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_booking_write_err("Failed to update booking", e))?
        .ok_or_else(|| AppError::NotFound(format!("Booking with ID '{}' not found", booking.id)))?;

        let updated = if replace_items {
            sqlx::query("DELETE FROM booking_items WHERE tenant_id = $1 AND booking_id = $2")
                .bind(tenant_id)
                .bind(booking.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AppError::DatabaseError(format!("Failed to replace booking items: {e}"))
                })?;

            let items = insert_booking_items(&mut tx, tenant_id, row.id, &booking.items).await?;
            row_to_booking(row, items)?
        } else {
            hydrate_bookings(&mut tx, tenant_id, vec![row])
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    AppError::NotFound(format!("Booking with ID '{}' not found", booking.id))
                })?
        };

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(updated)
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let res = sqlx::query(
            r#"
            UPDATE bookings
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete booking: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(res.rows_affected() > 0)
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<BookingSearchPage, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut count_qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM bookings WHERE tenant_id = ");
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
            .map_err(|e| AppError::DatabaseError(format!("Failed to count bookings: {e}")))?;

        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, tenant_id, booking_code, client_id, calendar_id, event_id, \
             starts_at, ends_at, timezone, status, \
             subtotal, tax_total, total_price, total_duration_minutes, currency, \
             notes, cancellation_reason, cancelled_at, completed_at, \
             metadata, created_by, updated_by, created_at, updated_at, deleted_at \
             FROM bookings WHERE tenant_id = ",
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

        let rows: Vec<BookingRow> = data_qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to search bookings: {e}")))?;

        let items = hydrate_bookings(&mut tx, tenant_id, rows).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(BookingSearchPage {
            items,
            total_items: u64::try_from(total_items).unwrap_or(0),
        })
    }
}
