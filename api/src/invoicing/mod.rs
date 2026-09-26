//! Invoicing domain logic that doesn't belong to any one storage backend or
//! GraphQL resolver — see CLAUDE.md's "Invoicing" house rule.
//!
//! - [`money`]: quantity/cents parsing, formatting, line amounts and GST.
//! - The billable-item input validators below, shared by
//!   `createBillableItem`/`updateBillableItem`.

pub mod money;

use chrono::NaiveDate;

/// Longest billable-item description, in characters (multi-line allowed).
pub const MAX_DESCRIPTION_LEN: usize = 2000;

/// Validate a `YYYY-MM-DD` calendar date — strictly that shape (zero-padded,
/// no time, no timezone) and a real day (`2026-02-30` is rejected). Returns
/// the canonical string, which is what's stored: a `date` sort key only
/// orders correctly if every value is written in exactly this form.
pub fn validate_item_date(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let parsed = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("{s:?} is not a valid date (expected YYYY-MM-DD)"))?;
    let canonical = parsed.format("%Y-%m-%d").to_string();
    // chrono accepts unpadded fields ("2026-8-1"); the sort key doesn't.
    if canonical != s {
        return Err(format!("{s:?} is not a valid date (expected YYYY-MM-DD)"));
    }
    Ok(canonical)
}

/// Trim and length-check a billable item's description. Multi-line is fine
/// (lines starting `* `/`- ` become bullets on the invoice); only leading/
/// trailing whitespace of the whole string is trimmed, so the line structure
/// survives.
pub fn validate_description(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Description cannot be empty".to_string());
    }
    if trimmed.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(format!(
            "Description cannot be longer than {MAX_DESCRIPTION_LEN} characters"
        ));
    }
    Ok(trimmed.to_string())
}

/// Range-check a unit price in cents: `0 ≤ p ≤` [`money::MAX_UNIT_PRICE_CENTS`].
/// Zero is allowed (a no-charge line); negatives are not (discounts/credit
/// lines are out of scope for v1).
pub fn validate_unit_price_cents(cents: i64) -> Result<i64, String> {
    if cents < 0 {
        return Err("Unit price cannot be negative".to_string());
    }
    if cents > money::MAX_UNIT_PRICE_CENTS {
        return Err("Unit price cannot be more than 10,000,000.00".to_string());
    }
    Ok(cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_item_date_accepts_a_real_padded_date() {
        assert_eq!(validate_item_date("2026-08-19").unwrap(), "2026-08-19");
        assert_eq!(validate_item_date(" 2024-02-29 ").unwrap(), "2024-02-29");
    }

    #[test]
    fn validate_item_date_rejects_bad_shapes_and_impossible_days() {
        for bad in [
            "",
            "2026-8-19",
            "2026-08-9",
            "19/08/2026",
            "2026-02-30",
            "2025-02-29",
            "2026-13-01",
            "2026-08-19T00:00:00",
            "20260819",
            "garbage",
        ] {
            assert!(validate_item_date(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn validate_description_trims_keeps_lines_and_caps_length() {
        assert_eq!(
            validate_description("  Labour\n* site visit\n* report  ").unwrap(),
            "Labour\n* site visit\n* report"
        );
        assert!(validate_description("   \n  ").is_err());
        assert!(validate_description(&"x".repeat(MAX_DESCRIPTION_LEN)).is_ok());
        assert!(validate_description(&"x".repeat(MAX_DESCRIPTION_LEN + 1)).is_err());
    }

    #[test]
    fn validate_unit_price_cents_bounds() {
        assert_eq!(validate_unit_price_cents(0), Ok(0));
        assert_eq!(
            validate_unit_price_cents(money::MAX_UNIT_PRICE_CENTS),
            Ok(money::MAX_UNIT_PRICE_CENTS)
        );
        assert!(validate_unit_price_cents(-1).is_err());
        assert!(validate_unit_price_cents(money::MAX_UNIT_PRICE_CENTS + 1).is_err());
    }
}
