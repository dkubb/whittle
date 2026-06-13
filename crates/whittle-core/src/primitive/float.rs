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
//!   can be expressed exactly in the const-generic syntax. A zero
//!   denominator or an empty range (`MIN > MAX`) fails to compile —
//!   both are rejected by `const { assert!(...) }` at
//!   monomorphisation, so no runtime rejects-everything state exists.

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;
use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

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

    /// Widen `self` into an `f64` for diagnostic reporting.
    ///
    /// Used by `FloatError::OutOfRange` to carry the offending
    /// value in a type-erased way (parity with
    /// `NumericError::OutOfRange { value: i128 }` and
    /// `StringError::CharCountOutOfRange { actual: usize }`). The
    /// widening is lossless for `f32` and identity for `f64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Float;
    ///
    /// assert_eq!(<f64 as Float>::into_f64(1.5_f64), 1.5_f64);
    /// assert_eq!(<f32 as Float>::into_f64(0.25_f32), 0.25_f64);
    /// ```
    fn into_f64(self) -> f64;

    /// This float type's largest finite value, widened losslessly to
    /// `f64`. [`Finite`]'s schema reads it so the described interval
    /// is the carrier's own finite range (`f32::MAX` for `f32`), not
    /// `f64`'s.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Float;
    ///
    /// assert_eq!(<f64 as Float>::FINITE_MAX, f64::MAX);
    /// assert_eq!(<f32 as Float>::FINITE_MAX, f64::from(f32::MAX));
    /// ```
    const FINITE_MAX: f64;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

// Both `f32` and `f64` implement `Float`. The four shared method
// bodies (NaN/infinite check, ratio lift) differ only in `Self`,
// so they expand from a shared macro. `into_f64` differs
// structurally — `f64::from` on `f32`, identity on `f64` — so it
// lives on each impl directly. Covering `f32::into_f64` requires
// a non-closure call site so `cargo coverage` attributes the
// region to a named function.
macro_rules! impl_float_shared {
    ($ty:ty) => {
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
    };
}

impl Float for f32 {
    impl_float_shared!(f32);

    #[inline]
    fn into_f64(self) -> f64 {
        f64::from(self)
    }

    const FINITE_MAX: f64 = Self::MAX as f64;
}

impl Float for f64 {
    impl_float_shared!(f64);

    #[inline]
    fn into_f64(self) -> f64 {
        self
    }

    #[expect(
        clippy::use_self,
        reason = "the trait declares the const as f64; Self happens to be f64 here"
    )]
    const FINITE_MAX: f64 = Self::MAX;
}

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
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "proptest")] {
    /// use proptest::strategy::{Strategy as _, ValueTree as _};
    /// use proptest::test_runner::TestRunner;
    /// use whittle_core::primitive::ArbitraryFloat;
    ///
    /// let strategy = f32::arbitrary_any();
    /// let mut runner = TestRunner::deterministic();
    /// let value = strategy.new_tree(&mut runner).unwrap().current();
    ///
    /// assert!(value.is_finite() || value.is_infinite() || value.is_nan());
    /// # }
    /// ```
    fn arbitrary_any() -> Self::AnyStrategy;
    /// Strategy that emits only finite values.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "proptest")] {
    /// use proptest::strategy::{Strategy as _, ValueTree as _};
    /// use proptest::test_runner::TestRunner;
    /// use whittle_core::primitive::ArbitraryFloat;
    ///
    /// let strategy = f32::arbitrary_finite();
    /// let mut runner = TestRunner::deterministic();
    /// let value = strategy.new_tree(&mut runner).unwrap().current();
    ///
    /// assert!(value.is_finite());
    /// # }
    /// ```
    fn arbitrary_finite() -> Self::FiniteStrategy;
    /// Strategy that emits values within an inclusive `[lo, hi]`
    /// closed range (no NaN; both endpoints inclusive).
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "proptest")] {
    /// use proptest::strategy::{Strategy as _, ValueTree as _};
    /// use proptest::test_runner::TestRunner;
    /// use whittle_core::primitive::ArbitraryFloat;
    ///
    /// let strategy = f32::arbitrary_in_closed_range(-1.0, 1.0);
    /// let mut runner = TestRunner::deterministic();
    /// let value = strategy.new_tree(&mut runner).unwrap().current();
    ///
    /// assert!((-1.0..=1.0).contains(&value));
    /// # }
    /// ```
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy;
}

#[cfg(feature = "proptest")]
impl ArbitraryFloat for f32 {
    type AnyStrategy = proptest::strategy::BoxedStrategy<Self>;
    type FiniteStrategy = proptest::strategy::BoxedStrategy<Self>;
    type ClosedRangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_any() -> Self::AnyStrategy {
        use proptest::strategy::Strategy as _;
        proptest::num::f32::ANY.boxed()
    }

    #[inline]
    fn arbitrary_finite() -> Self::FiniteStrategy {
        use proptest::strategy::Strategy as _;
        // POSITIVE | NEGATIVE | ZERO covers every finite f32 (the
        // sub-normals included).
        (proptest::num::f32::POSITIVE | proptest::num::f32::NEGATIVE | proptest::num::f32::ZERO)
            .boxed()
    }

    #[inline]
    #[expect(
        clippy::float_cmp,
        reason = "exact equality is the intended detector for the singleton case"
    )]
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy {
        // `proptest::Range<F>` is half-open `[lo, hi)`, so the upper
        // endpoint is never sampled and a post-`clamp` cannot
        // synthesize it. The degenerate `lo == hi` case is even
        // worse on negative inputs: IEEE 754 lays out negatives in
        // reverse bit order, so `from_bits(lo.to_bits() + 1)` for
        // `lo = -1.0` produces a value less than `-1.0`, making
        // `lo..span_hi` empty / invalid. Split into a singleton
        // branch plus a three-arm `prop_oneof!` that guarantees
        // both endpoints are reachable.
        use proptest::prelude::*;
        if lo == hi {
            // Degenerate: only one admissible value.
            Just(lo).boxed()
        } else {
            // Interior + both endpoints. proptest's half-open range
            // `lo..hi` excludes `hi`; the `prop_oneof!` branches
            // make both endpoints reachable as distinct samples.
            prop_oneof![
                1 => Just(lo),
                1 => Just(hi),
                8 => lo..hi,
            ]
            .boxed()
        }
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryFloat for f64 {
    type AnyStrategy = proptest::strategy::BoxedStrategy<Self>;
    type FiniteStrategy = proptest::strategy::BoxedStrategy<Self>;
    type ClosedRangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_any() -> Self::AnyStrategy {
        use proptest::strategy::Strategy as _;
        proptest::num::f64::ANY.boxed()
    }

    #[inline]
    fn arbitrary_finite() -> Self::FiniteStrategy {
        use proptest::strategy::Strategy as _;
        (proptest::num::f64::POSITIVE | proptest::num::f64::NEGATIVE | proptest::num::f64::ZERO)
            .boxed()
    }

    #[inline]
    #[expect(
        clippy::float_cmp,
        reason = "exact equality is the intended detector for the singleton case"
    )]
    fn arbitrary_in_closed_range(lo: Self, hi: Self) -> Self::ClosedRangeStrategy {
        // See `f32::arbitrary_in_closed_range` for the rationale;
        // the structure is identical (the bit-widening trick is
        // unsound on negative singletons, and the post-`clamp`
        // version did not let `hi` be sampled).
        use proptest::prelude::*;
        if lo == hi {
            Just(lo).boxed()
        } else {
            prop_oneof![
                1 => Just(lo),
                1 => Just(hi),
                8 => lo..hi,
            ]
            .boxed()
        }
    }
}

/// Errors common to every float primitive.
///
/// `InClosedRange` configuration errors (zero denominator, empty
/// range) are rejected at compile time via `const { assert!(...) }`
/// blocks inside `Rule::refine`, so their error variants are
/// unrepresentable.
///
/// Only `PartialEq` (not `Eq`) is derived: `OutOfRange` carries an
/// `f64` payload and `f64` does not satisfy reflexivity (`NaN !=
/// NaN`). The `OutOfRange` path is unreachable for NaN inputs (NaN
/// surfaces as `IsNan` first), so partial-equality is sufficient in
/// practice and matches the sibling error enums' carrying-the-value
/// pattern (`NumericError::OutOfRange { value: i128 }`,
/// `StringError::CharCountOutOfRange { actual: usize }`).
#[derive(Debug, PartialEq)]
pub enum FloatError {
    /// Value was NaN.
    IsNan,

    /// Value was `+INF` or `-INF`.
    IsInfinite,

    /// Value is outside the admissible closed range. Carries the
    /// offending value widened to `f64` for diagnostic parity with
    /// `NumericError::OutOfRange { value: i128 }`.
    ///
    /// `value` is guaranteed to be non-NaN:
    /// `InClosedRange<...>::refine` rejects NaN inputs with
    /// `FloatError::IsNan` before the range check runs, so
    /// `OutOfRange.value` carries only finite or infinite floats.
    OutOfRange {
        /// Offending value widened losslessly into `f64`. Guaranteed
        /// non-NaN (see the variant doc); finite or infinite only.
        value: f64,
    },
}

impl core::fmt::Display for FloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::IsNan => f.write_str("value is NaN"),
            Self::IsInfinite => f.write_str("value is infinite"),
            Self::OutOfRange { value } => {
                write!(f, "value {value} out of admissible range")
            }
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

/// `MIN_NUMERATOR/MIN_DENOMINATOR <= raw <= MAX_NUMERATOR/MAX_DENOMINATOR`.
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
/// assert_eq!(err, FloatError::OutOfRange { value: 1.5_f64 });
/// ```
pub struct InClosedRange<
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
>;

impl<
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
> InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
{
    /// Single source of the bound invariants: positive denominators
    /// and `MIN_NUMERATOR/MIN_DENOMINATOR <= MAX_NUMERATOR/MAX_DENOMINATOR`. Referenced from
    /// `Rule::refine` and `ArbitraryRule::arbitrary_strategy` via
    /// `const { Self::VALID }` so the three asserts cannot drift
    /// between the two sites.
    const VALID: () = {
        assert!(
            MIN_DENOMINATOR > 0,
            "InClosedRange requires MIN_DENOMINATOR > 0"
        );
        assert!(
            MAX_DENOMINATOR > 0,
            "InClosedRange requires MAX_DENOMINATOR > 0"
        );
        assert!(
            (MIN_NUMERATOR as i128) * (MAX_DENOMINATOR as i128)
                <= (MAX_NUMERATOR as i128) * (MIN_DENOMINATOR as i128),
            "InClosedRange requires MIN_NUMERATOR/MIN_DENOMINATOR <= MAX_NUMERATOR/MAX_DENOMINATOR",
        );
    };
}

impl<
    F: Float,
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
> Rule<F> for InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
{
    type Error = FloatError;

    #[inline]
    fn refine(raw: F) -> Result<F, Self::Error> {
        // Denominators must be positive so the cross-multiplied
        // comparison preserves order; range must be non-empty.
        // Both checks are const-evaluable for the const generic
        // parameters, so invalid configurations become compile
        // errors rather than runtime states.
        const { Self::VALID };
        let lo = F::from_ratio(MIN_NUMERATOR, MIN_DENOMINATOR);
        let hi = F::from_ratio(MAX_NUMERATOR, MAX_DENOMINATOR);
        if raw.float_is_nan() {
            return Err(FloatError::IsNan);
        }
        if !(lo..=hi).contains(&raw) {
            return Err(FloatError::OutOfRange {
                value: raw.into_f64(),
            });
        }
        Ok(raw)
    }
}

// ─── `PureFilter` impls. ──────────────────────────────────────────
//
// SOUNDNESS: every float rule's `refine` is an IEEE-754 class or
// range test returning the input itself on acceptance — no
// canonicalisation anywhere in the family.

impl crate::rule::PureFilter for NotNan {}
impl crate::rule::PureFilter for NotInfinite {}
impl crate::rule::PureFilter for Finite {}
impl<
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
> crate::rule::PureFilter
    for InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
{
}

// ─── `SchemaRule` impls. ──────────────────────────────────────────
//
// Float schemas are closed intervals over `f64` whose membership is
// decided by IEEE-754 comparisons — the same comparisons `refine`
// uses — so NaN (which compares false against every bound) is a
// member of no interval, and NaN exclusion needs no dedicated node:
//
// - `NotNan` is `[-inf, +inf]`: every non-NaN value, including the
//   infinities.
// - `Finite` is `[-FINITE_MAX, FINITE_MAX]` for the carrier's own
//   largest finite value: the infinities fall outside, and NaN is
//   outside every interval.
// - `InClosedRange` reads its endpoints through the same
//   `F::from_ratio` conversion `refine` uses (single determinant).
//
// `NotInfinite` has NO schema, deliberately: its admitted set
// (every finite value AND NaN, excluding exactly the two infinities)
// is not a closed interval under IEEE comparisons — NaN membership
// cannot be expressed by bounds that exclude the infinities. It
// stays schema-less until a node can describe it honestly (absence
// of impl is the design's "no schema" state; there is no Opaque
// node).

impl<F: Float> SchemaRule<F> for NotNan {
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(f64::NEG_INFINITY)),
            Bound::Inclusive(Scalar::Float(f64::INFINITY)),
        )
    }
}

impl<F: Float> SchemaRule<F> for Finite {
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(-F::FINITE_MAX)),
            Bound::Inclusive(Scalar::Float(F::FINITE_MAX)),
        )
    }
}

impl<
    F: Float,
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
> SchemaRule<F> for InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
{
    #[inline]
    fn schema() -> Schema {
        const { Self::VALID };
        // The same `from_ratio` lift `refine` performs, widened to
        // the schema's f64 universe, so the endpoints are bitwise
        // the values the rule compares against.
        let lo = F::from_ratio(MIN_NUMERATOR, MIN_DENOMINATOR).into_f64();
        let hi = F::from_ratio(MAX_NUMERATOR, MAX_DENOMINATOR).into_f64();
        Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(lo)),
            Bound::Inclusive(Scalar::Float(hi)),
        )
    }
}

// ─── Serde `DeserializeRule` impls: default parse-then-refine. ────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[F] DeserializeRule<F> for NotNan
    where [F: Float]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[F] DeserializeRule<F> for NotInfinite
    where [F: Float]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[F] DeserializeRule<F> for Finite
    where [F: Float]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[
        F,
        const MIN_NUMERATOR: i64,
        const MIN_DENOMINATOR: i64,
        const MAX_NUMERATOR: i64,
        const MAX_DENOMINATOR: i64,
    ] DeserializeRule<F>
    for InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
    where [F: Float]
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
    type Strategy = proptest::strategy::BoxedStrategy<F>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        F::arbitrary_any()
            .prop_filter("not NaN", float_is_not_nan::<F>)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<F> ArbitraryRule<F> for NotInfinite
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    // Admits every value except `+/-INF`; the admissible region is
    // dense.
    type Strategy = proptest::strategy::BoxedStrategy<F>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        F::arbitrary_any()
            .prop_filter("not infinite", float_is_not_infinite::<F>)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<F> ArbitraryRule<F> for Finite
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    // Use the finite-only strategy directly: every emitted value
    // is admissible by construction.
    type Strategy = proptest::strategy::BoxedStrategy<F>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        F::arbitrary_finite().boxed()
    }
}

#[cfg(feature = "proptest")]
impl<
    F,
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
> ArbitraryRule<F> for InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>
where
    F: ArbitraryFloat + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<F>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        let lo = F::from_ratio(MIN_NUMERATOR, MIN_DENOMINATOR);
        let hi = F::from_ratio(MAX_NUMERATOR, MAX_DENOMINATOR);
        F::arbitrary_in_closed_range(lo, hi).boxed()
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

    #[test]
    fn closed_range_rejects_above_max_f32() {
        // Reaches `f32::into_f64`'s widening arm via the `OutOfRange`
        // path. The exact-value match also documents that f32 -> f64
        // widening is lossless for the test endpoint.
        type UnitF32 = InClosedRange<0, 1, 1, 1>;
        let result: Result<Refined<f32, UnitF32>, _> = Refined::try_new(1.5_f32);
        assert_eq!(
            result.unwrap_err(),
            FloatError::OutOfRange {
                value: f64::from(1.5_f32),
            },
        );
    }

    #[test]
    fn into_f64_widens_f32_and_returns_f64_identity() {
        // Direct exercise of `Float::into_f64` on both types so the
        // per-monomorphization regions both reach 100% (the f32 arm
        // is only otherwise reached through the `OutOfRange` path).
        use super::Float;
        assert_eq!(<f32 as Float>::into_f64(0.25_f32), 0.25_f64);
        assert_eq!(<f64 as Float>::into_f64(1.5_f64), 1.5_f64);
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
        assert_eq!(
            result.unwrap_err(),
            FloatError::OutOfRange { value: -0.5_f64 },
        );
    }

    #[test]
    fn closed_range_rejects_above_max() {
        let result: Result<Refined<f64, UnitInterval>, _> = Refined::try_new(1.5_f64);
        assert_eq!(
            result.unwrap_err(),
            FloatError::OutOfRange { value: 1.5_f64 },
        );
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
            FloatError::OutOfRange { value: 1.5_f64 }.to_string(),
            "value 1.5 out of admissible range",
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
        assert_eq!(bad.unwrap_err(), FloatError::OutOfRange { value: 0.75_f64 },);
    }

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        // ─── Self-hosted Arbitrary on `Refined<f64, R>`. Float's
        //     default `Arbitrary` strategy includes NaN and the two
        //     infinities, so `NotNan`/`NotInfinite`/`Finite` apply a
        //     `prop_filter` on the dense admissible region (see the
        //     `prop_filter` calls in this file). The filter rate is
        //     one in ~2^N for IEEE-754 floats — negligible cost.

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

    // ─── Degenerate `lo == hi` strategy: the singleton branch
    //     returns `Just(lo)`. Exercised on both positive and
    //     negative singletons because the previous
    //     `from_bits(...).wrapping_add(1)` widening trick was
    //     unsound for negatives (IEEE 754 lays negatives out in
    //     reverse bit order, so adding one ulp to `-1.0` produces a
    //     value LESS than `-1.0`).
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

        // Negative singleton: regression for the bit-widening bug.
        let strategy_neg = <f64 as ArbitraryFloat>::arbitrary_in_closed_range(-1.0_f64, -1.0_f64);
        let tree = strategy_neg.new_tree(&mut runner).unwrap();
        assert_eq!(tree.current(), -1.0_f64);
    }

    // ─── SchemaRule: the constructive descriptions. ────────────────

    use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

    fn float_interval(lo: f64, hi: f64) -> Schema {
        Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(lo)),
            Bound::Inclusive(Scalar::Float(hi)),
        )
    }

    #[test]
    fn schema_not_nan_is_the_full_extended_interval() {
        // [-inf, +inf] under IEEE membership: every non-NaN value,
        // the infinities included, NaN excluded (it compares false
        // against both bounds). Same schema for both carriers.
        let expected = float_interval(f64::NEG_INFINITY, f64::INFINITY);
        assert_eq!(<NotNan as SchemaRule<f64>>::schema(), expected);
        assert_eq!(
            <NotNan as SchemaRule<f32>>::schema(),
            float_interval(f64::NEG_INFINITY, f64::INFINITY),
        );
    }

    #[test]
    fn schema_finite_is_the_carrier_finite_range() {
        // Per-carrier: f32's finite range is narrower than f64's.
        assert_eq!(
            <Finite as SchemaRule<f64>>::schema(),
            float_interval(f64::MIN, f64::MAX),
        );
        assert_eq!(
            <Finite as SchemaRule<f32>>::schema(),
            float_interval(-f64::from(f32::MAX), f64::from(f32::MAX)),
        );
    }

    #[test]
    fn schema_in_closed_range_reads_the_same_ratios_refine_reads() {
        // 1/2 ..= 3/4, lifted through the same from_ratio path.
        assert_eq!(
            <InClosedRange<1, 2, 3, 4> as SchemaRule<f64>>::schema(),
            float_interval(0.5, 0.75),
        );
        assert_eq!(
            <InClosedRange<1, 2, 3, 4> as SchemaRule<f32>>::schema(),
            float_interval(f64::from(0.5_f32), f64::from(0.75_f32)),
        );
    }

    #[cfg(feature = "proptest")]
    mod schema_cross_checks {
        use super::super::{Finite, InClosedRange, NotNan};
        use crate::schema::{Scalar, ScalarKind};
        use crate::testing::{assert_schema_boundary_matrix, prop_schema_cross_check};

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
        )]
        fn embed_f64(value: &f64) -> (ScalarKind, Scalar) {
            (ScalarKind::Float, Scalar::Float(*value))
        }

        fn extract_f64(_kind: ScalarKind, scalar: Scalar) -> Option<f64> {
            scalar.as_float()
        }

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
        )]
        fn embed_f32(value: &f32) -> (ScalarKind, Scalar) {
            (ScalarKind::Float, Scalar::Float(f64::from(*value)))
        }

        #[expect(
            clippy::cast_possible_truncation,
            clippy::return_and_then,
            reason = "narrowing probe, branch-free: the total_cmp round-trip check \
                      rejects any point f32 cannot represent losslessly, and the \
                      and_then chain avoids a None arm no candidate reaches"
        )]
        fn extract_f32(_kind: ScalarKind, scalar: Scalar) -> Option<f32> {
            scalar.as_float().and_then(|wide| {
                let narrow = wide as f32;
                (f64::from(narrow).total_cmp(&wide) == core::cmp::Ordering::Equal).then_some(narrow)
            })
        }

        /// Schema endpoints pass refine and strategy samples are
        /// schema members, for every float rule over `f64`.
        #[test]
        fn schema_cross_checks_float_rules_over_f64() {
            prop_schema_cross_check::<f64, NotNan>(embed_f64, extract_f64);
            prop_schema_cross_check::<f64, Finite>(embed_f64, extract_f64);
            prop_schema_cross_check::<f64, InClosedRange<0, 1, 1, 1>>(embed_f64, extract_f64);
        }

        /// Second carrier monomorphisations: the f32 schemas'
        /// endpoints all round-trip through the f32 carrier.
        #[test]
        fn schema_cross_checks_float_rules_over_f32() {
            prop_schema_cross_check::<f32, NotNan>(embed_f32, extract_f32);
            prop_schema_cross_check::<f32, Finite>(embed_f32, extract_f32);
            prop_schema_cross_check::<f32, InClosedRange<0, 1, 1, 1>>(embed_f32, extract_f32);
        }

        /// The schema-derived R-T1 boundary matrix. Each endpoint is
        /// stepped by one f64 ULP (`next_up`/`next_down`); the f32
        /// carrier skips neighbours it cannot represent losslessly,
        /// so its matrix tests the exact endpoints only.
        #[test]
        fn boundary_matrices_for_float_rules() {
            assert_schema_boundary_matrix::<f64, NotNan>(embed_f64, extract_f64);
            assert_schema_boundary_matrix::<f64, Finite>(embed_f64, extract_f64);
            assert_schema_boundary_matrix::<f64, InClosedRange<-1, 2, 3, 4>>(
                embed_f64,
                extract_f64,
            );
            assert_schema_boundary_matrix::<f32, Finite>(embed_f32, extract_f32);
            assert_schema_boundary_matrix::<f32, InClosedRange<0, 1, 1, 1>>(embed_f32, extract_f32);
        }
    }

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_in_closed_range_admits_endpoints_and_interior(
            v in <f64 as super::ArbitraryFloat>::arbitrary_in_closed_range(-1.0, 1.0)
        ) {
            proptest::prop_assert!(v >= -1.0);
            proptest::prop_assert!(v <= 1.0);
        }

        #[test]
        fn arbitrary_in_closed_range_singleton_yields_exact_value(
            v in <f64 as super::ArbitraryFloat>::arbitrary_in_closed_range(-3.5, -3.5)
        ) {
            proptest::prop_assert_eq!(v, -3.5);
        }
    }
}
