use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::{BundleComponent, CatalogItem, CatalogItemVariant, ItemDetails};
use crate::domain::repositories::{CatalogItemRepository, CatalogItemSearchPage};
use crate::infrastructure::database::begin_tenant_tx;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct CatalogItemRow {
    id: Uuid,
    tenant_id: Uuid,
    category_id: Option<Uuid>,
    item_type: String,
    sku: Option<String>,
    name: String,
    description: Option<String>,
    base_price: i64,
    currency: String,
    tax_rate_bps: i32,
    tax_included: bool,
    duration_minutes: Option<i32>,
    buffer_minutes: i32,
    unit_of_measure: Option<String>,
    track_inventory: bool,
    is_active: bool,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct VariantRow {
    id: Uuid,
    tenant_id: Uuid,
    item_id: Uuid,
    name: String,
    sku: Option<String>,
    price_override: Option<i64>,
    duration_override: Option<i32>,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<VariantRow> for CatalogItemVariant {
    fn from(r: VariantRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            item_id: r.item_id,
            name: r.name,
            sku: r.sku,
            price_override: r.price_override,
            duration_override: r.duration_override,
            is_active: r.is_active,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct BundleItemRow {
    bundle_id: Uuid,
    item_id: Uuid,
    quantity: i32,
}

fn row_to_item(
    row: CatalogItemRow,
    variants: Vec<CatalogItemVariant>,
    bundle_components: Vec<BundleComponent>,
) -> Result<CatalogItem, AppError> {
    let details = match row.item_type.as_str() {
        "SERVICE" => ItemDetails::Service {
            duration_minutes: row.duration_minutes.unwrap_or(0),
            buffer_minutes: row.buffer_minutes,
        },
        "PRODUCT" => ItemDetails::Product {
            unit_of_measure: row.unit_of_measure,
            track_inventory: row.track_inventory,
        },
        "BUNDLE" => ItemDetails::Bundle {
            items: bundle_components,
        },
        other => {
            return Err(AppError::DatabaseError(format!(
                "Unknown item_type in database: '{other}'"
            )));
        }
    };

    Ok(CatalogItem {
        id: row.id,
        tenant_id: row.tenant_id,
        category_id: row.category_id,
        sku: row.sku,
        name: row.name,
        description: row.description,
        base_price: row.base_price,
        currency: row.currency,
        tax_rate_bps: row.tax_rate_bps,
        tax_included: row.tax_included,
        details,
        is_active: row.is_active,
        metadata: row.metadata,
        variants,
        created_at: row.created_at,
        updated_at: row.updated_at,
        deleted_at: row.deleted_at,
    })
}

fn extract_type_columns(
    details: &ItemDetails,
) -> (&'static str, Option<i32>, i32, Option<&str>, bool) {
    match details {
        ItemDetails::Service {
            duration_minutes,
            buffer_minutes,
        } => (
            "SERVICE",
            Some(*duration_minutes),
            *buffer_minutes,
            None,
            false,
        ),
        ItemDetails::Product {
            unit_of_measure,
            track_inventory,
        } => (
            "PRODUCT",
            None,
            0,
            unit_of_measure.as_deref(),
            *track_inventory,
        ),
        ItemDetails::Bundle { .. } => ("BUNDLE", None, 0, None, false),
    }
}

async fn hydrate_items(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    rows: Vec<CatalogItemRow>,
) -> Result<Vec<CatalogItem>, AppError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let item_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    // Batch-load active variants in one query (defense-in-depth: explicit tenant_id)
    let variant_rows = sqlx::query_as::<_, VariantRow>(
        r#"
        SELECT id, tenant_id, item_id, name, sku, price_override, duration_override,
               is_active, created_at, updated_at, deleted_at
        FROM catalog_item_variants
        WHERE tenant_id = $1 AND item_id = ANY($2) AND deleted_at IS NULL
        ORDER BY created_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(&item_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::DatabaseError(format!("Failed to load item variants: {e}")))?;

    let mut variants_map: HashMap<Uuid, Vec<CatalogItemVariant>> = HashMap::new();
    for v in variant_rows {
        variants_map.entry(v.item_id).or_default().push(v.into());
    }

    // Batch-load bundle components in one query for any BUNDLE items
    let bundle_ids: Vec<Uuid> = rows
        .iter()
        .filter(|r| r.item_type == "BUNDLE")
        .map(|r| r.id)
        .collect();

    let mut bundles_map: HashMap<Uuid, Vec<BundleComponent>> = HashMap::new();
    if !bundle_ids.is_empty() {
        let comp_rows = sqlx::query_as::<_, BundleItemRow>(
            r#"
            SELECT bundle_id, item_id, quantity
            FROM bundle_items
            WHERE tenant_id = $1 AND bundle_id = ANY($2)
            "#,
        )
        .bind(tenant_id)
        .bind(&bundle_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to load bundle items: {e}")))?;

        for c in comp_rows {
            bundles_map
                .entry(c.bundle_id)
                .or_default()
                .push(BundleComponent {
                    item_id: c.item_id,
                    quantity: c.quantity,
                });
        }
    }

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let variants = variants_map.remove(&id).unwrap_or_default();
        let components = bundles_map.remove(&id).unwrap_or_default();
        result.push(row_to_item(row, variants, components)?);
    }

    Ok(result)
}

fn map_item_write_err(ctx: &str, err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = err {
        if db_err.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "A catalog item or variant with this SKU already exists in this tenant".to_string(),
            );
        }
    }
    AppError::DatabaseError(format!("{ctx}: {err}"))
}

/// PostgreSQL + RLS implementation of `CatalogItemRepository`.
pub struct SqlxCatalogItemRepository {
    pool: PgPool,
}

impl SqlxCatalogItemRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CatalogItemRepository for SqlxCatalogItemRepository {
    async fn create(&self, tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let (item_type, duration_minutes, buffer_minutes, unit_of_measure, track_inventory) =
            extract_type_columns(&item.details);

        let row = sqlx::query_as::<_, CatalogItemRow>(
            r#"
            INSERT INTO catalog_items (
                id, tenant_id, category_id, item_type, sku, name, description,
                base_price, currency, tax_rate_bps, tax_included,
                duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                is_active, metadata, created_at, updated_at, deleted_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11,
                $12, $13, $14, $15,
                $16, $17, $18, $19, $20
            )
            RETURNING id, tenant_id, category_id, item_type, sku, name, description,
                      base_price, currency, tax_rate_bps, tax_included,
                      duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                      is_active, metadata, created_at, updated_at, deleted_at
            "#,
        )
        .bind(item.id)
        .bind(tenant_id)
        .bind(item.category_id)
        .bind(item_type)
        .bind(&item.sku)
        .bind(&item.name)
        .bind(&item.description)
        .bind(item.base_price)
        .bind(&item.currency)
        .bind(item.tax_rate_bps)
        .bind(item.tax_included)
        .bind(duration_minutes)
        .bind(buffer_minutes)
        .bind(unit_of_measure)
        .bind(track_inventory)
        .bind(item.is_active)
        .bind(&item.metadata)
        .bind(item.created_at)
        .bind(item.updated_at)
        .bind(item.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_item_write_err("Failed to insert catalog item", e))?;

        let mut components = Vec::new();
        if let ItemDetails::Bundle { ref items } = item.details {
            for comp in items {
                sqlx::query(
                    r#"
                    INSERT INTO bundle_items (tenant_id, bundle_id, item_id, quantity)
                    VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(tenant_id)
                .bind(item.id)
                .bind(comp.item_id)
                .bind(comp.quantity)
                .execute(&mut *tx)
                .await
                .map_err(|e| map_item_write_err("Failed to insert bundle component", e))?;
                components.push(comp.clone());
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        row_to_item(row, Vec::new(), components)
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<CatalogItem>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CatalogItemRow>(
            r#"
            SELECT id, tenant_id, category_id, item_type, sku, name, description,
                   base_price, currency, tax_rate_bps, tax_included,
                   duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                   is_active, metadata, created_at, updated_at, deleted_at
            FROM catalog_items
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find catalog item: {e}")))?;

        let hydrated = match row {
            Some(r) => hydrate_items(&mut tx, tenant_id, vec![r])
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

    async fn find_by_sku(
        &self,
        tenant_id: Uuid,
        sku: &str,
    ) -> Result<Option<CatalogItem>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, CatalogItemRow>(
            r#"
            SELECT id, tenant_id, category_id, item_type, sku, name, description,
                   base_price, currency, tax_rate_bps, tax_included,
                   duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                   is_active, metadata, created_at, updated_at, deleted_at
            FROM catalog_items
            WHERE sku = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(sku)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find catalog item by SKU: {e}")))?;

        let hydrated = match row {
            Some(r) => hydrate_items(&mut tx, tenant_id, vec![r])
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

    async fn find_by_ids_batch(
        &self,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> Result<Vec<CatalogItem>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let rows = sqlx::query_as::<_, CatalogItemRow>(
            r#"
            SELECT id, tenant_id, category_id, item_type, sku, name, description,
                   base_price, currency, tax_rate_bps, tax_included,
                   duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                   is_active, metadata, created_at, updated_at, deleted_at
            FROM catalog_items
            WHERE tenant_id = $1 AND id = ANY($2) AND deleted_at IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to batch-load catalog items: {e}")))?;

        let items = hydrate_items(&mut tx, tenant_id, rows).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(items)
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<CatalogItemSearchPage, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut count_qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM catalog_items WHERE tenant_id = ");
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
            .map_err(|e| AppError::DatabaseError(format!("Failed to count catalog items: {e}")))?;

        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, tenant_id, category_id, item_type, sku, name, description, \
             base_price, currency, tax_rate_bps, tax_included, \
             duration_minutes, buffer_minutes, unit_of_measure, track_inventory, \
             is_active, metadata, created_at, updated_at, deleted_at \
             FROM catalog_items WHERE tenant_id = ",
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

        let rows: Vec<CatalogItemRow> = data_qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to search catalog items: {e}")))?;

        let items = hydrate_items(&mut tx, tenant_id, rows).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(CatalogItemSearchPage {
            items,
            total_items: u64::try_from(total_items).unwrap_or(0),
        })
    }

    async fn update(&self, tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let (item_type, duration_minutes, buffer_minutes, unit_of_measure, track_inventory) =
            extract_type_columns(&item.details);

        let row = sqlx::query_as::<_, CatalogItemRow>(
            r#"
            UPDATE catalog_items
            SET category_id = $1, item_type = $2, sku = $3, name = $4, description = $5,
                base_price = $6, currency = $7, tax_rate_bps = $8, tax_included = $9,
                duration_minutes = $10, buffer_minutes = $11, unit_of_measure = $12,
                track_inventory = $13, is_active = $14, metadata = $15, updated_at = NOW()
            WHERE id = $16 AND tenant_id = $17 AND deleted_at IS NULL
            RETURNING id, tenant_id, category_id, item_type, sku, name, description,
                      base_price, currency, tax_rate_bps, tax_included,
                      duration_minutes, buffer_minutes, unit_of_measure, track_inventory,
                      is_active, metadata, created_at, updated_at, deleted_at
            "#,
        )
        .bind(item.category_id)
        .bind(item_type)
        .bind(&item.sku)
        .bind(&item.name)
        .bind(&item.description)
        .bind(item.base_price)
        .bind(&item.currency)
        .bind(item.tax_rate_bps)
        .bind(item.tax_included)
        .bind(duration_minutes)
        .bind(buffer_minutes)
        .bind(unit_of_measure)
        .bind(track_inventory)
        .bind(item.is_active)
        .bind(&item.metadata)
        .bind(item.id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_item_write_err("Failed to update catalog item", e))?
        .ok_or_else(|| {
            AppError::NotFound(format!("Catalog item with ID '{}' not found", item.id))
        })?;

        sqlx::query("DELETE FROM bundle_items WHERE tenant_id = $1 AND bundle_id = $2")
            .bind(tenant_id)
            .bind(item.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AppError::DatabaseError(format!("Failed to clean up bundle components: {e}"))
            })?;

        if let ItemDetails::Bundle { ref items } = item.details {
            for comp in items {
                sqlx::query(
                    r#"
                    INSERT INTO bundle_items (tenant_id, bundle_id, item_id, quantity)
                    VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(tenant_id)
                .bind(item.id)
                .bind(comp.item_id)
                .bind(comp.quantity)
                .execute(&mut *tx)
                .await
                .map_err(|e| map_item_write_err("Failed to insert bundle component", e))?;
            }
        }

        let hydrated = hydrate_items(&mut tx, tenant_id, vec![row])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AppError::NotFound(format!("Catalog item with ID '{}' not found", item.id))
            })?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(hydrated)
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let res = sqlx::query(
            r#"
            UPDATE catalog_items
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete catalog item: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(res.rows_affected() > 0)
    }

    async fn add_variant(
        &self,
        tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, VariantRow>(
            r#"
            INSERT INTO catalog_item_variants (
                id, tenant_id, item_id, name, sku, price_override, duration_override,
                is_active, created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, tenant_id, item_id, name, sku, price_override, duration_override,
                      is_active, created_at, updated_at, deleted_at
            "#,
        )
        .bind(variant.id)
        .bind(tenant_id)
        .bind(variant.item_id)
        .bind(&variant.name)
        .bind(&variant.sku)
        .bind(variant.price_override)
        .bind(variant.duration_override)
        .bind(variant.is_active)
        .bind(variant.created_at)
        .bind(variant.updated_at)
        .bind(variant.deleted_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_item_write_err("Failed to insert variant", e))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn find_variant_by_id(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<Option<CatalogItemVariant>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, VariantRow>(
            r#"
            SELECT id, tenant_id, item_id, name, sku, price_override, duration_override,
                   is_active, created_at, updated_at, deleted_at
            FROM catalog_item_variants
            WHERE id = $1 AND item_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(variant_id)
        .bind(item_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to find variant: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn update_variant(
        &self,
        tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let row = sqlx::query_as::<_, VariantRow>(
            r#"
            UPDATE catalog_item_variants
            SET name = $1, sku = $2, price_override = $3, duration_override = $4,
                is_active = $5, updated_at = NOW()
            WHERE id = $6 AND item_id = $7 AND tenant_id = $8 AND deleted_at IS NULL
            RETURNING id, tenant_id, item_id, name, sku, price_override, duration_override,
                      is_active, created_at, updated_at, deleted_at
            "#,
        )
        .bind(&variant.name)
        .bind(&variant.sku)
        .bind(variant.price_override)
        .bind(variant.duration_override)
        .bind(variant.is_active)
        .bind(variant.id)
        .bind(variant.item_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_item_write_err("Failed to update variant", e))?
        .ok_or_else(|| AppError::NotFound(format!("Variant with ID '{}' not found", variant.id)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(row.into())
    }

    async fn delete_variant(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<bool, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let res = sqlx::query(
            r#"
            UPDATE catalog_item_variants
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND item_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(variant_id)
        .bind(item_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("Failed to soft-delete variant: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Transaction commit failed: {e}")))?;

        Ok(res.rows_affected() > 0)
    }
}
