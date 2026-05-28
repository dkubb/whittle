// Examples are interactive demonstrations: they use `println!` to
// confirm what was demonstrated and `unwrap()` to keep the focus on
// the API, not error plumbing. The workspace lints would otherwise
// deny both.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    clippy::items_after_statements,
    clippy::float_cmp
)]

//! Float primitives: NaN-free, infinity-free, finite, bounded.
//!
//! Covers `NotNan`, `NotInfinite`, `Finite`, and
//! `InClosedRange<NUM, DEN, NUM, DEN>`. Floats live outside the
//! integer-widening regime, so they have their own primitive set.
//!
//! Headline pattern: `Finite` is a nominal newtype with a flat
//! `FloatError`, exactly like `Within` for integers. Internally
//! it composes `And<NotNan, NotInfinite>`, but the composition
//! does not leak — callers see `FloatError::IsNan` or
//! `FloatError::IsInfinite`, never `AndError`.
//!
//! `InClosedRange` takes endpoints as `(numerator, denominator)`
//! because Rust 2024 does not yet permit `f64` const generics.

use whittle::primitive::{Finite, FloatError, InClosedRange, NotInfinite, NotNan};
use whittle::Refined;

/// Unit interval `[0.0, 1.0]` for probabilities or normalized
/// scalars.
type UnitInterval = InClosedRange<0, 1, 1, 1>;

/// Half-open band `[-0.5, 0.5]` expressed via rational endpoints.
type SignedHalf = InClosedRange<-1, 2, 1, 2>;

fn main() {
    // `NotNan` admits infinities; only NaN is rejected.
    let inf: Refined<f64, NotNan> = Refined::try_new(f64::INFINITY).unwrap();
    assert!(inf.as_inner().is_infinite());

    let nan_err = Refined::<f64, NotNan>::try_new(f64::NAN).unwrap_err();
    assert_eq!(nan_err, FloatError::IsNan);

    // `NotInfinite` admits NaN; only infinities are rejected.
    let zero: Refined<f64, NotInfinite> = Refined::try_new(0.0).unwrap();
    assert_eq!(*zero.as_inner(), 0.0);

    let inf_err = Refined::<f64, NotInfinite>::try_new(f64::INFINITY).unwrap_err();
    assert_eq!(inf_err, FloatError::IsInfinite);

    // `Finite` is the nominal newtype. Its flat `FloatError`
    // surfaces directly — no `AndError<FloatError, FloatError>`,
    // even though the implementation composes `NotNan` + `NotInfinite`.
    let val: Refined<f64, Finite> = Refined::try_new(1.5).unwrap();
    assert_eq!(*val.as_inner(), 1.5);
    let finite_nan = Refined::<f64, Finite>::try_new(f64::NAN).unwrap_err();
    assert_eq!(finite_nan, FloatError::IsNan);
    let finite_inf = Refined::<f64, Finite>::try_new(f64::NEG_INFINITY).unwrap_err();
    assert_eq!(finite_inf, FloatError::IsInfinite);

    // `InClosedRange` with `(num, den)` endpoints. The two type
    // aliases above name the interval; clients call `try_new`.
    let prob: Refined<f64, UnitInterval> = Refined::try_new(0.5).unwrap();
    assert_eq!(*prob.as_inner(), 0.5);

    let oor = Refined::<f64, UnitInterval>::try_new(1.5).unwrap_err();
    assert_eq!(oor, FloatError::OutOfRange);

    let half: Refined<f64, SignedHalf> = Refined::try_new(-0.25).unwrap();
    assert_eq!(*half.as_inner(), -0.25);

    println!("OK: float primitives — Finite carries the flat FloatError");
}
