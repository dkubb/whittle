//! Float primitives: NaN-free, infinity-free, finite, bounded.
//!
//! Covers `NotNan`, `NotInfinite`, `Finite`, and
//! `InClosedRange<NUM, DEN, NUM, DEN>`. Floats live outside the
//! integer-widening regime, so they have their own primitive set.
//!
//! Headline pattern: `Finite` is a nominal newtype with a flat
//! `FloatError`, exactly like `Within` for integers. Internally it
//! composes `And<NotNan, NotInfinite>`; both inner rules share
//! `FloatError`, so the composition's error is `FloatError`
//! directly — callers see `FloatError::IsNan` or
//! `FloatError::IsInfinite`.
//!
//! `InClosedRange` takes endpoints as `(numerator, denominator)`
//! because Rust 2024 does not yet permit `f64` const generics.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::float_cmp,
    reason = "integration test: unwrap keeps the focus on the API; exact float comparisons assert canonical values"
)]

use whittle::Refined;
use whittle::primitive::{Finite, FloatError, InClosedRange, NotInfinite, NotNan};

/// Unit interval `[0.0, 1.0]` for probabilities or normalized
/// scalars.
type UnitInterval = InClosedRange<0, 1, 1, 1>;

/// Half-open band `[-0.5, 0.5]` expressed via rational endpoints.
type SignedHalf = InClosedRange<-1, 2, 1, 2>;

#[test]
fn not_nan_admits_infinities_and_rejects_nan() {
    // `NotNan` admits infinities; only NaN is rejected.
    let inf: Refined<f64, NotNan> = Refined::try_new(f64::INFINITY).unwrap();
    assert!(inf.as_inner().is_infinite());

    let nan_err = Refined::<f64, NotNan>::try_new(f64::NAN).unwrap_err();
    assert_eq!(nan_err, FloatError::IsNan);
}

#[test]
fn not_infinite_admits_nan_and_rejects_infinities() {
    // `NotInfinite` admits NaN; only infinities are rejected.
    let zero: Refined<f64, NotInfinite> = Refined::try_new(0.0).unwrap();
    assert_eq!(*zero.as_inner(), 0.0);

    let inf_err = Refined::<f64, NotInfinite>::try_new(f64::INFINITY).unwrap_err();
    assert_eq!(inf_err, FloatError::IsInfinite);
}

#[test]
fn finite_admits_finite_values_and_rejects_nan_and_infinities_with_flat_error() {
    // `Finite` is the nominal newtype. Its flat `FloatError`
    // surfaces directly because both inner rules share the same
    // error type — no positional `Left`/`Right` wrapping even
    // though the implementation composes `NotNan` + `NotInfinite`.
    let val: Refined<f64, Finite> = Refined::try_new(1.5).unwrap();
    assert_eq!(*val.as_inner(), 1.5);
    let finite_nan = Refined::<f64, Finite>::try_new(f64::NAN).unwrap_err();
    assert_eq!(finite_nan, FloatError::IsNan);
    let finite_inf = Refined::<f64, Finite>::try_new(f64::NEG_INFINITY).unwrap_err();
    assert_eq!(finite_inf, FloatError::IsInfinite);
}

#[test]
fn in_closed_range_admits_endpoints_via_rational_constants() {
    // `InClosedRange` with `(num, den)` endpoints. The two type
    // aliases above name the interval; clients call `try_new`.
    let prob: Refined<f64, UnitInterval> = Refined::try_new(0.5).unwrap();
    assert_eq!(*prob.as_inner(), 0.5);

    let oor = Refined::<f64, UnitInterval>::try_new(1.5).unwrap_err();
    assert_eq!(oor, FloatError::OutOfRange);

    let half: Refined<f64, SignedHalf> = Refined::try_new(-0.25).unwrap();
    assert_eq!(*half.as_inner(), -0.25);
}
