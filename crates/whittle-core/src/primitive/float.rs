//! IEEE-754 float primitive rules.
//!
//! Floats sit outside the integer-widening regime that
//! `primitive::numeric` uses, so they get their own primitive
//! module. The three rules here are the load-bearing invariants
//! that downstream domains (database values, scientific kernels,
//! sample-rate-locked DSP) need:
//!
//! - `NotNan`: forbid `f32::NAN` / `f64::NAN`. The only IEEE-754
//!   value that fails reflexivity (`x == x`), so allowing it breaks
//!   set semantics, hashing, and ordering.
//! - `Finite`: forbid NaN *and* the two infinities. The standard
//!   refinement for code that compares, sums, or divides values.
//! - `InClosedRange<MIN, MAX>`: inclusive bound. Constants are
//!   passed as `(numerator, denominator)` so floating-point endpoints
//!   can be expressed exactly in the const-generic syntax. The
//!   denominator must be non-zero; the rule rejects everything when
//!   the range is empty (`MIN > MAX`) or the denominator is zero.

use thiserror::Error;

use crate::rule::Rule;

/// `f32` / `f64` extras shared by every float rule below.
///
/// Implemented for `f32` and `f64` only; sealed against external
/// implementors so future variants (`f16`, `f128`) can be added
/// without breaking downstream users. Methods are prefixed
/// `float_*` so trait dispatch does not collide with the inherent
/// `f32::is_nan` / `f64::is_nan` already in `core`.
pub trait Float: Copy + PartialOrd + 'static + sealed::Sealed {
    /// `true` iff the value is NaN.
    fn float_is_nan(self) -> bool;
    /// `true` iff the value is `+INF` or `-INF`.
    fn float_is_infinite(self) -> bool;
    /// Lift `(num, den)` (signed integer literals from a const
    /// generic) to the float domain for range comparison. Precision
    /// loss is acceptable for the small-integer endpoints this rule
    /// is designed for; users needing exact bounds at the limits of
    /// `f64` precision should compose a stricter rule.
    fn from_ratio(num: i64, den: i64) -> Self;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

impl Float for f32 {
    #[inline]
    fn float_is_nan(self) -> bool {
        self.is_nan()
    }
    #[inline]
    fn float_is_infinite(self) -> bool {
        self.is_infinite()
    }
    #[inline]
    #[allow(
        clippy::cast_precision_loss,
        reason = "endpoints intended to be small integers"
    )]
    fn from_ratio(num: i64, den: i64) -> Self {
        (num as Self) / (den as Self)
    }
}

impl Float for f64 {
    #[inline]
    fn float_is_nan(self) -> bool {
        self.is_nan()
    }
    #[inline]
    fn float_is_infinite(self) -> bool {
        self.is_infinite()
    }
    #[inline]
    #[allow(
        clippy::cast_precision_loss,
        reason = "endpoints intended to be small integers"
    )]
    fn from_ratio(num: i64, den: i64) -> Self {
        (num as Self) / (den as Self)
    }
}

/// Errors common to every float primitive.
#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum FloatError {
    /// Value was NaN.
    #[error("value is NaN")]
    IsNan,

    /// Value was `+INF` or `-INF`.
    #[error("value is infinite")]
    IsInfinite,

    /// `InClosedRange<NUM_MIN, DEN_MIN, NUM_MAX, DEN_MAX>` declared
    /// an empty range (`MIN > MAX`).
    #[error("empty range")]
    EmptyRange,

    /// `InClosedRange` declared a zero denominator.
    #[error("zero denominator in range bound")]
    ZeroDenominator,

    /// Value is outside the admissible closed range.
    #[error("value is outside the admissible range")]
    OutOfRange,
}

/// Reject NaN; admit anything else (including the infinities).
pub struct NotNan;

impl<F: Float> Rule<F> for NotNan {
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        if raw.float_is_nan() {
            return Err(FloatError::IsNan);
        }
        Ok(raw)
    }
}

/// Reject NaN and the two infinities; admit everything else.
pub struct Finite;

impl<F: Float> Rule<F> for Finite {
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        if raw.float_is_nan() {
            return Err(FloatError::IsNan);
        }
        if raw.float_is_infinite() {
            return Err(FloatError::IsInfinite);
        }
        Ok(raw)
    }
}

/// `MIN_NUM/MIN_DEN <= raw <= MAX_NUM/MAX_DEN`.
///
/// Endpoints are passed as ratios because Rust 2024 does not yet
/// permit `f64` const-generic parameters. To express `0.0..=1.0`,
/// write `InClosedRange<0, 1, 1, 1>`. Endpoints are compared after
/// converting `(num, den)` to the same float type as the value,
/// which keeps the rule cheap and free of platform-dependent
/// rounding for the typical small-integer endpoints. NaN inputs are
/// rejected before the bound check.
pub struct InClosedRange<
    const MIN_NUM: i64,
    const MIN_DEN: i64,
    const MAX_NUM: i64,
    const MAX_DEN: i64,
>;

impl<
    F: Float,
    const MIN_NUM: i64,
    const MIN_DEN: i64,
    const MAX_NUM: i64,
    const MAX_DEN: i64,
> Rule<F> for InClosedRange<MIN_NUM, MIN_DEN, MAX_NUM, MAX_DEN>
{
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        if MIN_DEN == 0 || MAX_DEN == 0 {
            return Err(FloatError::ZeroDenominator);
        }
        let lo = F::from_ratio(MIN_NUM, MIN_DEN);
        let hi = F::from_ratio(MAX_NUM, MAX_DEN);
        if lo > hi {
            return Err(FloatError::EmptyRange);
        }
        if raw.float_is_nan() {
            return Err(FloatError::IsNan);
        }
        if raw < lo || raw > hi {
            return Err(FloatError::OutOfRange);
        }
        Ok(raw)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used,
        clippy::float_cmp,
        reason = "explicit in test code")]
mod tests {
    use super::{Finite, FloatError, InClosedRange, NotNan};
    use crate::rule::{Refined, Rule};

    // ─── NotNan. ─────────────────────────────────────────────────

    #[test]
    fn not_nan_admits_zero_negative_and_inf() {
        assert!(NotNan::refine(0.0_f64).is_ok());
        assert!(NotNan::refine(-1.5_f64).is_ok());
        assert!(NotNan::refine(f64::INFINITY).is_ok());
        assert!(NotNan::refine(f64::NEG_INFINITY).is_ok());
    }

    #[test]
    fn not_nan_rejects_nan() {
        let result: Result<Refined<f64, NotNan>, _>
            = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    #[test]
    fn not_nan_works_on_f32() {
        let r: Refined<f32, NotNan> = Refined::try_new(1.5_f32).unwrap();
        assert_eq!(*r.as_inner(), 1.5_f32);
    }

    // ─── Finite. ─────────────────────────────────────────────────

    #[test]
    fn finite_admits_ordinary_values() {
        let r: Refined<f64, Finite> = Refined::try_new(2.5_f64).unwrap();
        assert_eq!(*r.as_inner(), 2.5_f64);
    }

    #[test]
    fn finite_rejects_positive_infinity() {
        let result: Result<Refined<f64, Finite>, _>
            = Refined::try_new(f64::INFINITY);
        assert_eq!(result.unwrap_err(), FloatError::IsInfinite);
    }

    #[test]
    fn finite_rejects_negative_infinity() {
        let result: Result<Refined<f64, Finite>, _>
            = Refined::try_new(f64::NEG_INFINITY);
        assert_eq!(result.unwrap_err(), FloatError::IsInfinite);
    }

    #[test]
    fn finite_rejects_nan() {
        let result: Result<Refined<f64, Finite>, _>
            = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    // ─── InClosedRange. ──────────────────────────────────────────

    type UnitInterval = InClosedRange<0, 1, 1, 1>;

    #[test]
    fn closed_range_admits_endpoints() {
        let lo: Refined<f64, UnitInterval>
            = Refined::try_new(0.0_f64).unwrap();
        let hi: Refined<f64, UnitInterval>
            = Refined::try_new(1.0_f64).unwrap();
        assert_eq!(*lo.as_inner(), 0.0_f64);
        assert_eq!(*hi.as_inner(), 1.0_f64);
    }

    #[test]
    fn closed_range_rejects_below_min() {
        let result: Result<Refined<f64, UnitInterval>, _>
            = Refined::try_new(-0.5_f64);
        assert_eq!(result.unwrap_err(), FloatError::OutOfRange);
    }

    #[test]
    fn closed_range_rejects_above_max() {
        let result: Result<Refined<f64, UnitInterval>, _>
            = Refined::try_new(1.5_f64);
        assert_eq!(result.unwrap_err(), FloatError::OutOfRange);
    }

    #[test]
    fn closed_range_rejects_nan() {
        let result: Result<Refined<f64, UnitInterval>, _>
            = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    #[test]
    fn closed_range_rejects_empty_range() {
        type Empty = InClosedRange<10, 1, 5, 1>;
        let result: Result<Refined<f64, Empty>, _>
            = Refined::try_new(0.0_f64);
        assert_eq!(result.unwrap_err(), FloatError::EmptyRange);
    }

    #[test]
    fn closed_range_rejects_zero_denominator() {
        type Zero = InClosedRange<1, 0, 1, 1>;
        let result: Result<Refined<f64, Zero>, _>
            = Refined::try_new(0.5_f64);
        assert_eq!(result.unwrap_err(), FloatError::ZeroDenominator);
    }

    #[test]
    fn closed_range_with_rational_endpoint() {
        // -0.5..=0.5 expressed as -1/2 ..= 1/2.
        type Half = InClosedRange<-1, 2, 1, 2>;
        let r: Refined<f64, Half> = Refined::try_new(0.25_f64).unwrap();
        assert_eq!(*r.as_inner(), 0.25_f64);
        let bad: Result<Refined<f64, Half>, _>
            = Refined::try_new(0.75_f64);
        assert_eq!(bad.unwrap_err(), FloatError::OutOfRange);
    }
}
