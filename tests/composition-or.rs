//! `Or<R1, R2>`: either rule may accept.
//!
//! `A::refine` runs first; on `Ok` its output is the result. On
//! `Err` the *original* input is tried against `B::refine`. Both
//! rules must share the same `Rule::Error` type; when both reject
//! the composition returns `[E; 2]` — the left rejection first,
//! the right rejection second.
//!
//! **Newtype pattern.** `[E; 2]` is informationally complete but
//! rarely the shape a public domain API wants. Wrap the `Or` in a
//! nominal newtype and collapse the pair into a single named
//! variant inside `try_new`. The closing test shows the shape;
//! `flat-domain-error.rs` is the canonical reference.
//!
//! Note: `Or` requires `T: Clone` because the original input must
//! be preserved for the second attempt. This is the only `Clone`
//! constraint in the kernel.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::primitive::{AtLeast, AtMost, NumericError};
use whittle::{Or, Refined};

#[test]
fn or_admits_via_either_alternative_and_returns_both_errors_when_neither_accepts() {
    // "Out of the middle band": value <= 10 OR value >= 100. Both
    // rules produce `NumericError`, so the composition's error is
    // `[NumericError; 2]`.
    type OutsideMiddle = Or<AtMost<10>, AtLeast<100>>;

    // Admit via the left alternative.
    let small: Refined<i32, OutsideMiddle> = Refined::try_new(5).unwrap();
    assert_eq!(*small.as_inner(), 5);

    // Admit via the right alternative.
    let big: Refined<i32, OutsideMiddle> = Refined::try_new(150).unwrap();
    assert_eq!(*big.as_inner(), 150);

    // Reject: 50 falls into the forbidden middle. The error is
    // `[NumericError; 2]` carrying both rejections in left-then-
    // right order.
    let both: [NumericError; 2] = Refined::<i32, OutsideMiddle>::try_new(50).unwrap_err();
    assert_eq!(both[0], NumericError::OutOfRange { value: 50 });
    assert_eq!(both[1], NumericError::OutOfRange { value: 50 });
}

#[test]
fn newtype_collapses_or_error_pair_into_a_flat_domain_enum() {
    // ─── Domain newtype around an `Or` composition. ─────────────
    //
    // The pattern to copy: a newtype with a flat error enum.
    // Callers see one variant ("not extreme"), not the
    // composition shape ("both alternatives rejected with
    // NumericError"). Either rejection carries the offending
    // value; collapsing them into a single domain variant is the
    // natural API.
    type OutsideMiddle = Or<AtMost<10>, AtLeast<100>>;

    #[derive(Debug, PartialEq, Eq)]
    enum ExtremeError {
        NotExtreme { value: i128 },
    }

    #[derive(Debug)]
    struct Extreme(Refined<i32, OutsideMiddle>);

    impl Extreme {
        fn try_new(raw: i32) -> Result<Self, ExtremeError> {
            Refined::try_new(raw).map(Self).map_err(|errs: [NumericError; 2]| {
                // Both inner errors carry the same offending value;
                // collapse to a single variant. `NumericError` is
                // `#[non_exhaustive]`, so the match needs a
                // catch-all even though only one variant exists.
                let [left, _right] = errs;
                let value = match left {
                    NumericError::OutOfRange { value } => value,
                    _ => i128::from(raw),
                };
                ExtremeError::NotExtreme { value }
            })
        }
    }

    let edge = Extreme::try_new(150).unwrap();
    assert_eq!(*edge.0.as_inner(), 150);

    let stuck = Extreme::try_new(50).unwrap_err();
    assert_eq!(stuck, ExtremeError::NotExtreme { value: 50 });
}
