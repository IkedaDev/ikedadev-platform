use chrono::{DateTime, Utc};
use ikeda_core::error::{AppError, ValidationErrorDetail};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Input snapshot for a single line item when creating or replacing booking items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BookingItemInput {
    pub catalog_item_id: Uuid,
    #[serde(default)]
    pub variant_id: Option<Uuid>,
    pub name_snapshot: String,
    #[serde(default)]
    pub sku_snapshot: Option<String>,
    #[serde(default = "default_quantity")]
    pub quantity: i32,
    pub unit_price_snapshot: i64,
    #[serde(default)]
    pub tax_rate_bps: i32,
    #[serde(default = "default_tax_included")]
    pub tax_included: bool,
    #[serde(default)]
    pub duration_minutes_snapshot: i32,
    #[serde(default)]
    pub buffer_minutes_snapshot: i32,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

const fn default_quantity() -> i32 {
    1
}

const fn default_tax_included() -> bool {
    true
}

/// Computed integer monetary amounts for a single line item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalculatedLineItem {
    pub line_subtotal: i64,
    pub line_tax: i64,
    pub line_total: i64,
}

/// Aggregated integer totals and total duration for a booking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedBookingTotals {
    pub line_items: Vec<CalculatedLineItem>,
    pub subtotal: i64,
    pub tax_total: i64,
    pub total_price: i64,
    pub total_duration_minutes: i32,
}

/// Validates an ISO 4217 currency code (3 uppercase ASCII letters).
pub fn validate_currency(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.len() != 3 || !trimmed.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::validation(
            "Invalid currency code",
            [ValidationErrorDetail::new(
                "currency",
                "Currency must be a 3-letter uppercase ISO 4217 code (e.g. CLP, USD, EUR)",
            )],
        ));
    }
    Ok(trimmed.to_string())
}

/// Validates that `ends_at` is strictly greater than `starts_at`.
pub fn validate_time_range(
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
) -> Result<(), AppError> {
    if ends_at <= starts_at {
        return Err(AppError::validation(
            "Invalid booking time range",
            [ValidationErrorDetail::new(
                "ends_at",
                "ends_at must be strictly after starts_at",
            )],
        ));
    }
    Ok(())
}

/// Generates a readable booking code such as `BK-20261124-9F3A1C2B`.
///
/// Uses the trailing random entropy hex digits of `id` (critical for UUIDv7,
/// whose leading 12 hex digits encode the coarse timestamp and would collide
/// for bookings created within the same ~4.6-hour window).
#[must_use]
pub fn generate_booking_code(now: DateTime<Utc>, id: Uuid) -> String {
    let date_part = now.format("%Y%m%d");
    let id_hex = id.simple().to_string();
    let suffix = &id_hex[id_hex.len().saturating_sub(8)..];
    format!("BK-{date_part}-{}", suffix.to_ascii_uppercase())
}

/// Pure integer financial & duration calculator (no floating-point arithmetic).
///
/// - Enforces at least 1 item (`422 Unprocessable Entity` if empty).
/// - Validates per-item invariants (`quantity >= 1`, `unit_price_snapshot >= 0`,
///   `tax_rate_bps` in `0..=10000`, non-negative duration/buffer, non-empty name).
/// - Computes `line_subtotal`, `line_tax`, `line_total` with half-up integer rounding
///   preserving `line_subtotal + line_tax == line_total`.
/// - Derives `total_duration_minutes` from summed `(duration + buffer) * quantity`,
///   falling back to the slot span `(ends_at - starts_at)` when all items are zero-duration
///   (e.g. product-only bookings).
pub fn calculate_booking_totals(
    items: &[BookingItemInput],
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
) -> Result<CalculatedBookingTotals, AppError> {
    validate_time_range(starts_at, ends_at)?;

    if items.is_empty() {
        return Err(AppError::validation(
            "Booking must contain at least one item",
            [ValidationErrorDetail::new(
                "items",
                "At least one catalog item snapshot is required",
            )],
        ));
    }

    let mut errors = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if item.name_snapshot.trim().is_empty() {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].name_snapshot"),
                "Item name_snapshot cannot be empty",
            ));
        }
        if item.quantity < 1 {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].quantity"),
                "Quantity must be at least 1",
            ));
        }
        if item.unit_price_snapshot < 0 {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].unit_price_snapshot"),
                "Unit price snapshot must be >= 0",
            ));
        }
        if !(0..=10_000).contains(&item.tax_rate_bps) {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].tax_rate_bps"),
                "tax_rate_bps must be between 0 and 10000",
            ));
        }
        if item.duration_minutes_snapshot < 0 {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].duration_minutes_snapshot"),
                "duration_minutes_snapshot must be >= 0",
            ));
        }
        if item.buffer_minutes_snapshot < 0 {
            errors.push(ValidationErrorDetail::new(
                format!("items[{idx}].buffer_minutes_snapshot"),
                "buffer_minutes_snapshot must be >= 0",
            ));
        }
    }

    if !errors.is_empty() {
        return Err(AppError::validation("Invalid booking items", errors));
    }

    let mut line_items = Vec::with_capacity(items.len());
    let mut subtotal_acc: i128 = 0;
    let mut tax_acc: i128 = 0;
    let mut total_acc: i128 = 0;
    let mut duration_acc: i128 = 0;

    for item in items {
        let gross = i128::from(item.unit_price_snapshot) * i128::from(item.quantity);
        let bps = i128::from(item.tax_rate_bps);

        let (line_sub_128, line_tax_128, line_tot_128) = if bps == 0 {
            (gross, 0_i128, gross)
        } else if item.tax_included {
            let denom = 10_000_i128 + bps;
            let net = (gross * 10_000_i128 + denom / 2) / denom;
            let tax = gross - net;
            (net, tax, gross)
        } else {
            let tax = (gross * bps + 5_000_i128) / 10_000_i128;
            (gross, tax, gross + tax)
        };

        let line_subtotal = i64::try_from(line_sub_128)
            .map_err(|_| AppError::BadRequest("Line subtotal overflow".to_string()))?;
        let line_tax = i64::try_from(line_tax_128)
            .map_err(|_| AppError::BadRequest("Line tax overflow".to_string()))?;
        let line_total = i64::try_from(line_tot_128)
            .map_err(|_| AppError::BadRequest("Line total overflow".to_string()))?;

        line_items.push(CalculatedLineItem {
            line_subtotal,
            line_tax,
            line_total,
        });

        subtotal_acc += line_sub_128;
        tax_acc += line_tax_128;
        total_acc += line_tot_128;

        let item_span =
            i128::from(item.duration_minutes_snapshot) + i128::from(item.buffer_minutes_snapshot);
        duration_acc += item_span * i128::from(item.quantity);
    }

    let subtotal = i64::try_from(subtotal_acc)
        .map_err(|_| AppError::BadRequest("Booking subtotal overflow".to_string()))?;
    let tax_total = i64::try_from(tax_acc)
        .map_err(|_| AppError::BadRequest("Booking tax_total overflow".to_string()))?;
    let total_price = i64::try_from(total_acc)
        .map_err(|_| AppError::BadRequest("Booking total_price overflow".to_string()))?;

    let final_duration_128 = if duration_acc > 0 {
        duration_acc
    } else {
        i128::from((ends_at - starts_at).num_minutes().max(1))
    };

    let total_duration_minutes = i32::try_from(final_duration_128)
        .map_err(|_| AppError::BadRequest("Booking total_duration_minutes overflow".to_string()))?;

    Ok(CalculatedBookingTotals {
        line_items,
        subtotal,
        tax_total,
        total_price,
        total_duration_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_item(
        price: i64,
        qty: i32,
        bps: i32,
        tax_included: bool,
        duration: i32,
        buffer: i32,
    ) -> BookingItemInput {
        BookingItemInput {
            catalog_item_id: Uuid::new_v4(),
            variant_id: None,
            name_snapshot: "Servicio Demo".to_string(),
            sku_snapshot: Some("SVC-001".to_string()),
            quantity: qty,
            unit_price_snapshot: price,
            tax_rate_bps: bps,
            tax_included,
            duration_minutes_snapshot: duration,
            buffer_minutes_snapshot: buffer,
            metadata: None,
        }
    }

    #[test]
    fn test_tax_included_true_integer_calculation() {
        let start = Utc::now();
        let end = start + Duration::minutes(60);
        // 11_900 CLP with 19% VAT included -> net 10_000, tax 1_900, total 11_900
        let items = vec![sample_item(11_900, 2, 1_900, true, 30, 5)];
        let totals = calculate_booking_totals(&items, start, end).unwrap();

        assert_eq!(totals.subtotal, 20_000);
        assert_eq!(totals.tax_total, 3_800);
        assert_eq!(totals.total_price, 23_800);
        assert_eq!(totals.total_duration_minutes, 70);
        assert_eq!(totals.line_items[0].line_subtotal, 20_000);
        assert_eq!(totals.line_items[0].line_tax, 3_800);
        assert_eq!(totals.line_items[0].line_total, 23_800);
    }

    #[test]
    fn test_tax_included_false_multiple_items() {
        let start = Utc::now();
        let end = start + Duration::minutes(90);
        let items = vec![
            sample_item(10_000, 2, 1_900, false, 45, 0),
            sample_item(5_000, 1, 1_000, false, 15, 5),
        ];
        let totals = calculate_booking_totals(&items, start, end).unwrap();

        // Item 1: sub 20_000, tax 3_800, total 23_800, dur 90
        // Item 2: sub 5_000, tax 500, total 5_500, dur 20
        assert_eq!(totals.subtotal, 25_000);
        assert_eq!(totals.tax_total, 4_300);
        assert_eq!(totals.total_price, 29_300);
        assert_eq!(totals.total_duration_minutes, 110);
    }

    #[test]
    fn test_invariants_reject_empty_items_inverted_dates_and_bad_currency() {
        let start = Utc::now();
        let end = start + Duration::minutes(30);

        assert!(matches!(
            calculate_booking_totals(&[], start, end),
            Err(AppError::UnprocessableEntity { .. })
        ));
        assert!(matches!(
            calculate_booking_totals(&[sample_item(1000, 1, 0, true, 30, 0)], end, start),
            Err(AppError::UnprocessableEntity { .. })
        ));
        assert!(matches!(
            validate_currency("clp"),
            Err(AppError::UnprocessableEntity { .. })
        ));
        assert!(matches!(
            validate_currency("US"),
            Err(AppError::UnprocessableEntity { .. })
        ));
        assert_eq!(validate_currency("CLP").unwrap(), "CLP");
    }
}
