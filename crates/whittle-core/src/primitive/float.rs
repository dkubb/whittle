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

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
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
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Float;
    ///
    /// assert!(<f64 as Float>::float_is_nan(f64::NAN));
    /// assert!(!<f64 as Float>::float_is_nan(0.0_f64));
    /// assert!(!<f32 as Float>::float_is_nan(f32::INFINITY));
    /// ```
    fn float_is_nan(self) -> bool;
    /// `true` iff the value is `+INF` or `-INF`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Float;
    ///
    /// assert!(<f64 as Float>::float_is_infinite(f64::INFINITY));
    /// assert!(<f64 as Float>::float_is_infinite(f64::NEG_INFINITY));
    /// assert!(!<f64 as Float>::float_is_infinite(0.0_f64));
    /// ```
    fn float_is_infinite(self) -> bool;
    /// Lift `(num, den)` (signed integer literals from a const
    /// generic) to the float domain for range comparison. Precision
    /// loss is acceptable for the small-integer endpoints this rule
    /// is designed for; users needing exact bounds at the limits of
    /// `f64` precision should compose a stricter rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Float;
    ///
    /// assert_eq!(<f64 as Float>::from_ratio(1, 2), 0.5_f64);
    /// assert_eq!(<f32 as Float>::from_ratio(-1, 4), -0.25_f32);
    /// ```
    fn from_ratio(num: i64, den: i64) -> Self;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

// Both `f32` and `f64` implement `Float` with bodies that differ
// only in `Self`. Expand a single template for each.
macro_rules! impl_float {
    ($ty:ty) => {
        impl Float for $ty {
            #[inline]
            fn float_is_nan(self) -> bool {
                self.is_nan()
            }
            #[inline]
            fn float_is_infinite(self) -> bool {
                self.is_infinite()
            }
            #[inline]
            #[expect(
                clippy::cast_precision_loss,
                reason = "endpoints intended to be small integers"
            )]
            fn from_ratio(num: i64, den: i64) -> Self {
                (num as Self) / (den as Self)
            }
        }
    };
}

impl_float!(f32);
impl_float!(f64);

/// `Float` types that expose `proptest` strategies for the float
/// primitive rules to consume.
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait ArbitraryFloat: Float {
    /// Strategy that emits any value of this float type — NaN,
    /// infinities, and finite values all included. Used by `NotNan`
    /// and `NotInfinite`, whose admissible regions are dense.
    type AnyStrategy: proptest::strategy::Strategy<Value = Self>;

    /// Strategy that emits only finite values (no NaN, no
    /// infinities). Used by `Finite`'s `ArbitraryRule` impl so the
    /// strategy is admissible by construction.
    type FiniteStrategy: proptest::strategy::Strategy<Value = Self>;

    /// Strategy that emits values within a closed `[lo, hi]` range
    /// (NaN and infinities excluded; both endpoints reachable).
    type ClosedRangeStrategy: proptest::strategy::Strategy<Value = Self>;

    /// Strategy that emits any value of this float type.
    fn arbitrary_any() -> Self::AnyStrategy;
    /// Strategy that emits only finite values.
    fn arbitrary_finite() -> Self::FiniteStrategy;
    /// Strategy that emits values within an inclusive `[lo, hi]`
    /// closed range (no NaN; both endpoints inclusive).
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy;
}

#[cfg(feature = "proptest")]
impl ArbitraryFloat for f32 {
    type AnyStrategy = proptest::num::f32::Any;
    type FiniteStrategy = proptest::num::f32::Any;
    type ClosedRangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_any() -> Self::AnyStrategy {
        proptest::num::f32::ANY
    }

    #[inline]
    fn arbitrary_finite() -> Self::FiniteStrategy {
        // POSITIVE | NEGATIVE | ZERO covers every finite f32 (the
        // sub-normals included).
        proptest::num::f32::POSITIVE | proptest::num::f32::NEGATIVE | proptest::num::f32::ZERO
    }

    #[inline]
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy {
        // proptest's `Range<f32>` strategy is half-open
        // `[lo, hi)`. Wrap the inner strategy in a post-`clamp` so
        // the closed upper bound is reachable without rejection
        // sampling; the closure capture is hidden behind
        // `BoxedStrategy`. Guard the degenerate `lo == hi` case
        // by widening the inner range by one ulp; `clamp` collapses
        // it back to the singleton.
        use proptest::strategy::Strategy as _;
        let span_hi = if lo < hi {
            hi
        } else {
            Self::from_bits(lo.to_bits().wrapping_add(1))
        };
        (lo..span_hi).prop_map(move |v| v.clamp(lo, hi)).boxed()
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryFloat for f64 {
    type AnyStrategy = proptest::num::f64::Any;
    type FiniteStrategy = proptest::num::f64::Any;
    type ClosedRangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_any() -> Self::AnyStrategy {
        proptest::num::f64::ANY
    }

    #[inline]
    fn arbitrary_finite() -> Self::FiniteStrategy {
        proptest::num::f64::POSITIVE | proptest::num::f64::NEGATIVE | proptest::num::f64::ZERO
    }

    #[inline]
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy {
        use proptest::strategy::Strategy as _;
        let span_hi = if lo < hi {
            hi
        } else {
            Self::from_bits(lo.to_bits().wrapping_add(1))
        };
        (lo..span_hi).prop_map(move |v| v.clamp(lo, hi)).boxed()
    }
}

/// Errors common to every float primitive.
///
/// `InClosedRange` configuration errors (zero denominator, empty
/// range) are rejected at compile time via `const { assert!(...) }`
/// blocks inside `Rule::refine`, so their error variants are
/// unrepresentable.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FloatError {
    /// Value was NaN.
    IsNan,

    /// Value was `+INF` or `-INF`.
    IsInfinite,

    /// Value is outside the admissible closed range.
    OutOfRange,
}

impl core::fmt::Display for FloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::IsNan => f.write_str("value is NaN"),
            Self::IsInfinite => f.write_str("value is infinite"),
            Self::OutOfRange => f.write_str("value is outside the admissible range"),
        }
    }
}

impl core::error::Error for FloatError {}

/// Reject NaN; admit anything else (including the infinities).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{FloatError, NotNan};
///
/// // Admit: any non-NaN value, including infinities.
/// let ok: Refined<f64, NotNan> = Refined::try_new(1.5_f64).unwrap();
/// assert_eq!(*ok.as_inner(), 1.5_f64);
///
/// // Reject: NaN.
/// let err = Refined::<f64, NotNan>::try_new(f64::NAN).unwrap_err();
/// assert_eq!(err, FloatError::IsNan);
/// ```
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

/// Reject `±INF`; admit NaN and every finite value.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{FloatError, NotInfinite};
///
/// // Admit: an ordinary value.
/// let ok: Refined<f64, NotInfinite>
///     = Refined::try_new(2.5_f64).unwrap();
/// assert_eq!(*ok.as_inner(), 2.5_f64);
///
/// // Reject: positive infinity.
/// let err = Refined::<f64, NotInfinite>::try_new(f64::INFINITY)
///     .unwrap_err();
/// assert_eq!(err, FloatError::IsInfinite);
/// ```
pub struct NotInfinite;

impl<F: Float> Rule<F> for NotInfinite {
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        if raw.float_is_infinite() {
            return Err(FloatError::IsInfinite);
        }
        Ok(raw)
    }
}

/// Reject NaN and the two infinities; admit every other float.
///
/// `Finite` is a nominal domain newtype. Internally it composes
/// `NotNan` and `NotInfinite` via `And<...>`. Both inner rules share
/// `FloatError`, so the composition's error is `FloatError` directly
/// — the `And`/`Or` machinery is an implementation detail, not part
/// of the domain surface.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{Finite, FloatError};
///
/// // Admit: an ordinary finite value.
/// let ok: Refined<f64, Finite> = Refined::try_new(2.5_f64).unwrap();
/// assert_eq!(*ok.as_inner(), 2.5_f64);
///
/// // Reject NaN.
/// let err = Refined::<f64, Finite>::try_new(f64::NAN).unwrap_err();
/// assert_eq!(err, FloatError::IsNan);
///
/// // Reject infinity.
/// let err = Refined::<f64, Finite>::try_new(f64::INFINITY)
///     .unwrap_err();
/// assert_eq!(err, FloatError::IsInfinite);
/// ```
pub struct Finite;

impl<F: Float> Rule<F> for Finite {
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        <crate::composition::And<NotNan, NotInfinite> as Rule<F>>::refine(raw)
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
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{FloatError, InClosedRange};
///
/// // `0.0..=1.0`.
/// type UnitInterval = InClosedRange<0, 1, 1, 1>;
///
/// // Admit: a value within the range.
/// let ok: Refined<f64, UnitInterval>
///     = Refined::try_new(0.5_f64).unwrap();
/// assert_eq!(*ok.as_inner(), 0.5_f64);
///
/// // Reject: a value above the range.
/// let err = Refined::<f64, UnitInterval>::try_new(1.5_f64).unwrap_err();
/// assert_eq!(err, FloatError::OutOfRange);
/// ```
pub struct InClosedRange<
    const MIN_NUM: i64,
    const MIN_DEN: i64,
    const MAX_NUM: i64,
    const MAX_DEN: i64,
>;

impl<F: Float, const MIN_NUM: i64, const MIN_DEN: i64, const MAX_NUM: i64, const MAX_DEN: i64>
    Rule<F> for InClosedRange<MIN_NUM, MIN_DEN, MAX_NUM, MAX_DEN>
{
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        // Denominators must be positive so the cross-multiplied
        // comparison preserves order; range must be non-empty.
        // Both checks are const-evaluable for the const generic
        // parameters, so invalid configurations become compile
        // errors rather than runtime states.
        const {
            assert!(MIN_DEN > 0, "InClosedRange requires MIN_DEN > 0");
            assert!(MAX_DEN > 0, "InClosedRange requires MAX_DEN > 0");
            assert!(
                (MIN_NUM as i128) * (MAX_DEN as i128) <= (MAX_NUM as i128) * (MIN_DEN as i128),
                "InClosedRange requires MIN_NUM/MIN_DEN <= MAX_NUM/MAX_DEN",
            );
        };
        let lo = F::from_ratio(MIN_NUM, MIN_DEN);
        let hi = F::from_ratio(MAX_NUM, MAX_DEN);
        if raw.float_is_nan() {
            return Err(FloatError::IsNan);
        }
        if !(lo..=hi).contains(&raw) {
            return Err(FloatError::OutOfRange);
        }
        Ok(raw)
    }
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────

#[cfg(feature = "proptest")]
fn float_is_not_nan<F: Float>(value: &F) -> bool {
    !value.float_is_nan()
}

#[cfg(feature = "proptest")]
fn float_is_not_infinite<F: Float>(value: &F) -> bool {
    !value.float_is_infinite()
}

#[cfg(feature = "proptest")]
impl<F> ArbitraryRule<F> for NotNan
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    // `NotNan` admits every value except NaN; the admissible
    // region is dense, so a single `prop_filter` on the
    // unrestricted `any` strategy is cheap.
    type Strategy = proptest::strategy::Filter<<F as ArbitraryFloat>::AnyStrategy, fn(&F) -> bool>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        F::arbitrary_any().prop_filter("not NaN", float_is_not_nan::<F>)
    }
}

#[cfg(feature = "proptest")]
impl<F> ArbitraryRule<F> for NotInfinite
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    // Admits every value except `+/-INF`; the admissible region is
    // dense.
    type Strategy = proptest::strategy::Filter<<F as ArbitraryFloat>::AnyStrategy, fn(&F) -> bool>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        F::arbitrary_any().prop_filter("not infinite", float_is_not_infinite::<F>)
    }
}

#[cfg(feature = "proptest")]
impl<F> ArbitraryRule<F> for Finite
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    // Use the finite-only strategy directly: every emitted value
    // is admissible by construction.
    type Strategy = <F as ArbitraryFloat>::FiniteStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        F::arbitrary_finite()
    }
}

#[cfg(feature = "proptest")]
impl<F, const MIN_NUM: i64, const MIN_DEN: i64, const MAX_NUM: i64, const MAX_DEN: i64>
    ArbitraryRule<F> for InClosedRange<MIN_NUM, MIN_DEN, MAX_NUM, MAX_DEN>
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    type Strategy = <F as ArbitraryFloat>::ClosedRangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        const {
            assert!(MIN_DEN > 0, "InClosedRange requires MIN_DEN > 0");
            assert!(MAX_DEN > 0, "InClosedRange requires MAX_DEN > 0");
            assert!(
                (MIN_NUM as i128) * (MAX_DEN as i128) <= (MAX_NUM as i128) * (MIN_DEN as i128),
                "InClosedRange requires MIN_NUM/MIN_DEN <= MAX_NUM/MAX_DEN",
            );
        };
        let lo = F::from_ratio(MIN_NUM, MIN_DEN);
        let hi = F::from_ratio(MAX_NUM, MAX_DEN);
        F::arbitrary_in_closed_range(lo, hi)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::float_cmp,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString;

    use super::{Finite, FloatError, InClosedRange, NotNan};
    use crate::rule::{Refined, Rule};

    refinement! {
        /// Macro-generated newtype for testing: `f64` finite probability
        /// in the unit interval `[0.0, 1.0]`. Exercises `refinement!`
        /// from the float primitive test module.
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
        pub UnitProbability: f64, InClosedRange<0, 1, 1, 1>;
    }

    // ─── NotNan. ─────────────────────────────────────────────────

    #[test]
    fn not_nan_admits_zero_negative_and_inf() {
        NotNan::refine(0.0_f64).unwrap();
        NotNan::refine(-1.5_f64).unwrap();
        NotNan::refine(f64::INFINITY).unwrap();
        NotNan::refine(f64::NEG_INFINITY).unwrap();
    }

    #[test]
    fn not_nan_rejects_nan() {
        let result: Result<Refined<f64, NotNan>, _> = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    #[test]
    fn not_nan_works_on_f32() {
        let r: Refined<f32, NotNan> = Refined::try_new(1.5_f32).unwrap();
        assert_eq!(*r.as_inner(), 1.5_f32);
    }

    #[test]
    fn finite_rejects_f32_infinity() {
        // Reaches Float for f32's float_is_infinite arm. Both inner
        // rules of `Finite`'s `And<NotNan, NotInfinite>` composition
        // share `FloatError`, so the domain error surfaces directly.
        let result: Result<Refined<f32, Finite>, _> = Refined::try_new(f32::INFINITY);
        assert_eq!(result.unwrap_err(), FloatError::IsInfinite);
    }

    #[test]
    fn closed_range_admits_f32_endpoint() {
        // Reaches Float for f32's from_ratio arm.
        type UnitF32 = InClosedRange<0, 1, 1, 1>;
        let r: Refined<f32, UnitF32> = Refined::try_new(0.5_f32).unwrap();
        assert_eq!(*r.as_inner(), 0.5_f32);
    }

    // ─── Finite. ─────────────────────────────────────────────────

    #[test]
    fn finite_admits_ordinary_values() {
        let r: Refined<f64, Finite> = Refined::try_new(2.5_f64).unwrap();
        assert_eq!(*r.as_inner(), 2.5_f64);
    }

    #[test]
    fn finite_rejects_positive_infinity() {
        let result: Result<Refined<f64, Finite>, _> = Refined::try_new(f64::INFINITY);
        // The shared-error composition surfaces the domain
        // rejection directly, without any positional wrapping.
        assert_eq!(result.unwrap_err(), FloatError::IsInfinite);
    }

    #[test]
    fn finite_rejects_negative_infinity() {
        let result: Result<Refined<f64, Finite>, _> = Refined::try_new(f64::NEG_INFINITY);
        assert_eq!(result.unwrap_err(), FloatError::IsInfinite);
    }

    #[test]
    fn finite_rejects_nan() {
        let result: Result<Refined<f64, Finite>, _> = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    // ─── InClosedRange. ──────────────────────────────────────────

    type UnitInterval = InClosedRange<0, 1, 1, 1>;

    #[test]
    fn closed_range_admits_endpoints() {
        let lo: Refined<f64, UnitInterval> = Refined::try_new(0.0_f64).unwrap();
        let hi: Refined<f64, UnitInterval> = Refined::try_new(1.0_f64).unwrap();
        assert_eq!(*lo.as_inner(), 0.0_f64);
        assert_eq!(*hi.as_inner(), 1.0_f64);
    }

    #[test]
    fn closed_range_rejects_below_min() {
        let result: Result<Refined<f64, UnitInterval>, _> = Refined::try_new(-0.5_f64);
        assert_eq!(result.unwrap_err(), FloatError::OutOfRange);
    }

    #[test]
    fn closed_range_rejects_above_max() {
        let result: Result<Refined<f64, UnitInterval>, _> = Refined::try_new(1.5_f64);
        assert_eq!(result.unwrap_err(), FloatError::OutOfRange);
    }

    #[test]
    fn closed_range_rejects_nan() {
        let result: Result<Refined<f64, UnitInterval>, _> = Refined::try_new(f64::NAN);
        assert_eq!(result.unwrap_err(), FloatError::IsNan);
    }

    // InClosedRange's zero-denominator and empty-range
    // configurations are now compile-time errors via
    // `const { assert!(...) }`. The previous runtime tests that
    // exercised those error variants are no longer needed because
    // the offending monomorphizations are unrepresentable.

    #[test]
    fn refinement_macro_unit_probability_admits_and_rejects() {
        // Macro-generated newtype: admit 0.25, reject 1.5.
        let ok = UnitProbability::try_new(0.25_f64).unwrap();
        assert_eq!(*ok.as_inner(), 0.25_f64);
        let owned: f64 = ok.into_inner();
        assert_eq!(owned, 0.25_f64);
        let bad = UnitProbability::try_new(1.5_f64);
        bad.unwrap_err();
    }

    #[test]
    fn display_formats_every_variant() {
        // Hand-rolled `Display` arms — one assertion per variant so
        // each arm is exercised. `core::error::Error` is implemented
        // (no source chaining), confirmed via the trait cast.
        assert_eq!(FloatError::IsNan.to_string(), "value is NaN");
        assert_eq!(FloatError::IsInfinite.to_string(), "value is infinite");
        assert_eq!(
            FloatError::OutOfRange.to_string(),
            "value is outside the admissible range",
        );
        let dyn_err: &dyn core::error::Error = &FloatError::IsNan;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn closed_range_with_rational_endpoint() {
        // -0.5..=0.5 expressed as -1/2 ..= 1/2.
        type Half = InClosedRange<-1, 2, 1, 2>;
        let r: Refined<f64, Half> = Refined::try_new(0.25_f64).unwrap();
        assert_eq!(*r.as_inner(), 0.25_f64);
        let bad: Result<Refined<f64, Half>, _> = Refined::try_new(0.75_f64);
        assert_eq!(bad.unwrap_err(), FloatError::OutOfRange);
    }

    proptest::proptest! {
        // ─── Self-hosted Arbitrary on `Refined<f64, R>`. Float's
        //     default `Arbitrary` strategy includes NaN and the
        //     two infinities, so the kernel's rejection-sampling
        //     filter is the only thing keeping values admissible
        //     downstream.

        #[test]
        fn arbitrary_not_nan_value_is_not_nan(
            r in proptest::arbitrary::any::<Refined<f64, NotNan>>()
        ) {
            proptest::prop_assert!(!r.as_inner().is_nan());
        }

        #[test]
        fn arbitrary_not_infinite_value_is_not_infinite(
            r in proptest::arbitrary::any::<
                Refined<f64, super::NotInfinite>,
            >()
        ) {
            // NaN is admitted (NaN is not infinite); only +/-INF
            // must be excluded.
            proptest::prop_assert!(!r.as_inner().is_infinite());
        }

        #[test]
        fn arbitrary_finite_value_is_finite(
            r in proptest::arbitrary::any::<Refined<f64, Finite>>()
        ) {
            proptest::prop_assert!(r.as_inner().is_finite());
        }

        #[test]
        fn arbitrary_unit_interval_in_closed_range(
            r in proptest::arbitrary::any::<
                Refined<f64, InClosedRange<0, 1, 1, 1>>,
            >()
        ) {
            let value = *r.as_inner();
            // `InClosedRange` rejects NaN, so the inner value must
            // be a non-NaN scalar in `[0.0, 1.0]`.
            proptest::prop_assert!(!value.is_nan());
            proptest::prop_assert!((0.0_f64..=1.0_f64).contains(&value));
        }

        // ─── `ArbitraryFloat` impls for f32. Each rule's strategy
        //     is its own monomorphisation; touching one per rule
        //     pins the f32 impl's branches to the coverage graph.

        #[test]
        fn arbitrary_not_nan_f32_value_is_not_nan(
            r in proptest::arbitrary::any::<Refined<f32, NotNan>>()
        ) {
            proptest::prop_assert!(!r.as_inner().is_nan());
        }

        #[test]
        fn arbitrary_not_infinite_f32_value_is_not_infinite(
            r in proptest::arbitrary::any::<Refined<f32, super::NotInfinite>>()
        ) {
            proptest::prop_assert!(!r.as_inner().is_infinite());
        }

        #[test]
        fn arbitrary_finite_f32_value_is_finite(
            r in proptest::arbitrary::any::<Refined<f32, Finite>>()
        ) {
            proptest::prop_assert!(r.as_inner().is_finite());
        }

        #[test]
        fn arbitrary_unit_interval_f32_in_closed_range(
            r in proptest::arbitrary::any::<Refined<f32, InClosedRange<0, 1, 1, 1>>>()
        ) {
            let value = *r.as_inner();
            proptest::prop_assert!(!value.is_nan());
            proptest::prop_assert!((0.0_f32..=1.0_f32).contains(&value));
        }
    }

    // ─── Degenerate `lo == hi` strategy: exercises the
    //     `from_bits(...).wrapping_add(1)` fixup that lets
    //     proptest's half-open `Range<F>` strategy still produce a
    //     value when both endpoints collapse to a singleton.
    #[cfg(feature = "proptest")]
    #[test]
    fn closed_range_singleton_strategy_is_well_formed() {
        use super::ArbitraryFloat;
        use proptest::strategy::Strategy as _;
        use proptest::test_runner::TestRunner;
        let strategy_f32 = <f32 as ArbitraryFloat>::arbitrary_in_closed_range(1.0_f32, 1.0_f32);
        let mut runner = TestRunner::default();
        let tree = strategy_f32.new_tree(&mut runner).unwrap();
        assert_eq!(tree.current(), 1.0_f32);

        let strategy_f64 = <f64 as ArbitraryFloat>::arbitrary_in_closed_range(1.0_f64, 1.0_f64);
        let tree = strategy_f64.new_tree(&mut runner).unwrap();
        assert_eq!(tree.current(), 1.0_f64);
    }
}
