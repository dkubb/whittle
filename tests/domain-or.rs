//! `Or<R1, R2>` as a genuine disjunction in the domain.
//!
//! Most domain types want a flat enum with named variants — the
//! `Or` shape is rarely the right one for a public API. Here it
//! is: the pipeline admits either a *small batch* (1..=10) or a
//! *large batch* (100..=1000), with the intermediate range
//! (11..=99) deliberately excluded because batches that size are
//! inefficient for our worker pool.
//!
//! The newtype hides the `Or` shape. The flat error collapses the
//! `[NumericError; 2]` that the composition returns into a single
//! named variant — both arms rejected the same value, so the
//! diagnostic only needs to carry the value once.
//!
//! Use this only when the disjunction itself is the domain
//! semantic. If a caller wants to know which arm rejected, prefer
//! two distinct newtypes plus an enum.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use whittle::primitive::{NumericError, Within};
use whittle::{Or, Refined};

/// Batch size for the pipeline worker pool.
///
/// The composition `Or<Within<1, 10>, Within<100, 1000>>` admits
/// the two "efficient" ranges and rejects the intermediate gap.
/// The inner `Refined<...>` is private so callers cannot bypass
/// `try_new` — the disjunction is an implementation detail of the
/// invariant, not a public API choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatchSize(Refined<u32, Or<Within<1, 10>, Within<100, 1000>>>);

/// Flat domain error.
///
/// Both arms of the `Or` reject with `NumericError::OutOfRange`
/// carrying the same offending value, so the two rejections
/// collapse into a single variant — callers don't care which arm
/// rejected, only that the value sits in neither admissible range.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum BatchSizeError {
    /// Value is in neither `1..=10` nor `100..=1000`.
    #[error("batch size {value} is not in 1..=10 or 100..=1000")]
    OutOfRange { value: u32 },
}

impl BatchSize {
    pub fn try_new(raw: u32) -> Result<Self, BatchSizeError> {
        Refined::try_new(raw)
            .map(Self)
            .map_err(|errs: [NumericError; 2]| {
                // Both inner arms carry the same offending value;
                // pull it from the left rejection and drop the right.
                let [left, _right] = errs;
                match left {
                    NumericError::OutOfRange { .. } => BatchSizeError::OutOfRange { value: raw },
                }
            })
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        *self.0.as_inner()
    }
}

#[test]
fn batch_size_admits_small_batch_within_first_range() {
    let small = BatchSize::try_new(5).unwrap();
    assert_eq!(small.get(), 5);
}

#[test]
fn batch_size_admits_large_batch_within_second_range() {
    let large = BatchSize::try_new(500).unwrap();
    assert_eq!(large.get(), 500);
}

#[test]
fn batch_size_rejects_zero_below_small_range() {
    let err = BatchSize::try_new(0).unwrap_err();
    assert_eq!(err, BatchSizeError::OutOfRange { value: 0 });
}

#[test]
fn batch_size_rejects_intermediate_value_in_excluded_gap() {
    // 50 sits in the deliberate gap between the two ranges; the
    // domain semantic is "neither efficient size", which the flat
    // error names.
    let err = BatchSize::try_new(50).unwrap_err();
    assert_eq!(err, BatchSizeError::OutOfRange { value: 50 });
}

#[test]
fn batch_size_rejects_value_above_large_range() {
    let err = BatchSize::try_new(1500).unwrap_err();
    assert_eq!(err, BatchSizeError::OutOfRange { value: 1500 });
}
