//! Cross-field invariants that whittle's primitives can't encode.
//!
//! Whittle's rules validate a single value at a time — `Year`'s
//! `Within<2000, 2100>` is per-field. A constraint like `end >=
//! start` spans two fields, so it lives in the parent struct's
//! `try_new` after each field's `try_new` succeeds. The flat
//! error mixes per-field failures (wrapping the field type's own
//! flat error) with cross-field failures as named variants.
//!
//! This is the standard pattern: each field stays orthogonal and
//! reusable; the parent owns the cross-field rule. Use it for
//! date ranges, min/max pairs, account/sub-account pairings, and
//! anywhere two refined fields have to agree.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use whittle::Refined;
use whittle::primitive::{NumericError, Within};

/// Year in `2000..=2100`. Private inner `Refined<...>` forces
/// construction through `try_new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Year(Refined<u16, Within<2000, 2100>>);

/// Flat domain error for `Year` — one variant for the single
/// range-check rule.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum YearError {
    #[error("year {value} is not in 2000..=2100")]
    OutOfRange { value: u16 },
}

impl Year {
    pub fn try_new(raw: u16) -> Result<Self, YearError> {
        Refined::try_new(raw)
            .map(Self)
            .map_err(|err: NumericError| match err {
                NumericError::OutOfRange { .. } => YearError::OutOfRange { value: raw },
                other => unreachable!("unexpected inner NumericError variant: {other:?}"),
            })
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        *self.0.as_inner()
    }
}

/// A closed date range. Each field is a refined `Year`; the
/// parent enforces the cross-field `end >= start` invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateRange {
    start: Year,
    end: Year,
}

/// Flat domain error for `DateRange`.
///
/// Per-field failures wrap the field's `YearError`; the
/// cross-field failure is its own variant carrying both endpoints
/// as `u16` so the diagnostic stays meaningful.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DateRangeError {
    #[error("invalid start year: {0}")]
    InvalidStart(#[source] YearError),
    #[error("invalid end year: {0}")]
    InvalidEnd(#[source] YearError),
    #[error("end year {end} is before start year {start}")]
    EndBeforeStart { start: u16, end: u16 },
}

impl DateRange {
    pub fn try_new(start: u16, end: u16) -> Result<Self, DateRangeError> {
        // Validate each field; per-field failures surface as
        // `InvalidStart` / `InvalidEnd` wrapping the field error.
        let start = Year::try_new(start).map_err(DateRangeError::InvalidStart)?;
        let end = Year::try_new(end).map_err(DateRangeError::InvalidEnd)?;

        // Cross-field check runs only after both fields are valid.
        if end < start {
            return Err(DateRangeError::EndBeforeStart {
                start: start.get(),
                end: end.get(),
            });
        }

        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> Year {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> Year {
        self.end
    }
}

#[test]
fn date_range_admits_valid_ordered_endpoints() {
    let range = DateRange::try_new(2020, 2025).unwrap();
    assert_eq!(range.start().get(), 2020);
    assert_eq!(range.end().get(), 2025);
}

#[test]
fn date_range_admits_equal_endpoints_as_single_year_window() {
    // `end >= start` is non-strict — single-year windows are legal.
    let range = DateRange::try_new(2024, 2024).unwrap();
    assert_eq!(range.start().get(), 2024);
    assert_eq!(range.end().get(), 2024);
}

#[test]
fn date_range_rejects_end_before_start_with_cross_field_variant() {
    // Both 2025 and 2020 are valid years individually; the failure
    // is the cross-field invariant, named in the flat error.
    let err = DateRange::try_new(2025, 2020).unwrap_err();
    assert_eq!(
        err,
        DateRangeError::EndBeforeStart {
            start: 2025,
            end: 2020
        },
    );
}

#[test]
fn date_range_rejects_invalid_start_with_wrapped_field_error() {
    // 1999 is below `Year`'s range; the flat error wraps the
    // field's `YearError` in `InvalidStart`.
    let err = DateRange::try_new(1999, 2025).unwrap_err();
    assert_eq!(
        err,
        DateRangeError::InvalidStart(YearError::OutOfRange { value: 1999 })
    );
}

#[test]
fn date_range_rejects_invalid_end_with_wrapped_field_error() {
    // 2200 is above `Year`'s range; the flat error wraps the
    // field's `YearError` in `InvalidEnd`.
    let err = DateRange::try_new(2025, 2200).unwrap_err();
    assert_eq!(
        err,
        DateRangeError::InvalidEnd(YearError::OutOfRange { value: 2200 })
    );
}
