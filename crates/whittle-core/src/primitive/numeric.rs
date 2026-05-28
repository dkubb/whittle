//! Numeric primitive rules.
//!
//! Bounded ranges (`Within<MIN, MAX>`, `AtLeast<MIN>`, `AtMost<MAX>`)
//! and sign / non-zero markers (`NonZero`, `Positive`, `Negative`).
//! Each primitive carries a typed error variant that includes the
//! offending value so callers can construct precise diagnostics.

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// Inclusive numeric range: `MIN <= value <= MAX`.
///
/// `Within` is a nominal domain newtype. Internally it composes
/// `AtLeast<MIN>` and `AtMost<MAX>` via `And<...>`. Both inner rules
/// share `NumericError`, so the composition's error is `NumericError`
/// directly — the `And`/`Or` machinery is an implementation detail,
/// not part of the domain surface.
///
/// `MIN > MAX` fails to compile: the `refine` impl carries a
/// `const { assert!(MIN <= MAX) }` block that fires at
/// monomorphisation. Degenerate empty-range instantiations are
/// unrepresentable, so their error variant is too.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NumericError, Within};
///
/// // Admit: value is within the inclusive range.
/// let ok: Refined<i32, Within<0, 100>> = Refined::try_new(50).unwrap();
/// assert_eq!(*ok.as_inner(), 50);
///
/// // Reject above MAX.
/// let err = Refined::<i32, Within<0, 100>>::try_new(101).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 101 });
///
/// // Reject below MIN.
/// let err = Refined::<i32, Within<0, 100>>::try_new(-1).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: -1 });
/// ```
pub struct Within<const MIN: i128, const MAX: i128>;

/// Lower-bound rule: `MIN <= value`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{AtLeast, NumericError};
///
/// let ok: Refined<i32, AtLeast<10>> = Refined::try_new(10).unwrap();
/// assert_eq!(*ok.as_inner(), 10);
///
/// let err = Refined::<i32, AtLeast<10>>::try_new(9).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 9 });
/// ```
pub struct AtLeast<const MIN: i128>;

/// Upper-bound rule: `value <= MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{AtMost, NumericError};
///
/// let ok: Refined<i32, AtMost<100>> = Refined::try_new(100).unwrap();
/// assert_eq!(*ok.as_inner(), 100);
///
/// let err = Refined::<i32, AtMost<100>>::try_new(101).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 101 });
/// ```
pub struct AtMost<const MAX: i128>;

/// Rejects zero.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NonZero, NumericError};
///
/// let ok: Refined<i32, NonZero> = Refined::try_new(-3).unwrap();
/// assert_eq!(*ok.as_inner(), -3);
///
/// let err = Refined::<i32, NonZero>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub struct NonZero;

/// `value > 0`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NumericError, Positive};
///
/// let ok: Refined<i32, Positive> = Refined::try_new(1).unwrap();
/// assert_eq!(*ok.as_inner(), 1);
///
/// // Zero is not positive.
/// let err = Refined::<i32, Positive>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub struct Positive;

/// `value < 0`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{Negative, NumericError};
///
/// let ok: Refined<i32, Negative> = Refined::try_new(-1).unwrap();
/// assert_eq!(*ok.as_inner(), -1);
///
/// // Zero is not negative.
/// let err = Refined::<i32, Negative>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub struct Negative;

/// Error variants common to every numeric primitive.
///
/// The variant carries the offending value as `i128` because every
/// supported numeric type widens losslessly into `i128`. Invalid
/// rule configurations (e.g. `Within<MIN, MAX>` with `MIN > MAX`)
/// are rejected at compile time via `const { assert!(...) }`
/// blocks inside the affected `Rule::refine` impls, so their
/// error variant is unrepresentable.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericError {
    /// Value lies outside the rule's admissible range.
    OutOfRange {
        /// Offending value widened losslessly into `i128`.
        value: i128,
    },
}

impl core::fmt::Display for NumericError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::OutOfRange { value } => {
                write!(f, "value {value} not in admissible range")
            }
        }
    }
}

impl core::error::Error for NumericError {}

/// Conversion from a concrete numeric type into and back out of
/// `i128`, used by every numeric primitive's `Rule` impl.
///
/// Implementations exist for the standard signed and unsigned
/// integer types. `u128` is deliberately omitted: it cannot
/// round-trip through `i128`. A future `WithinUnsigned` family will
/// cover the upper half of `u128`.
///
/// `into_i128` is infallible because every supported type widens
/// losslessly into `i128`. `usize` / `isize` would only fail on a
/// platform whose pointer width exceeds 128 bits, which does not
/// exist; the impl panics in that case rather than carrying a
/// permanently-dead error path through every `Rule::refine` site.
pub trait Numeric: Sized + 'static {
    /// Widen `self` into an `i128`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Numeric;
    ///
    /// assert_eq!(<i32 as Numeric>::into_i128(42_i32), 42_i128);
    /// assert_eq!(<u64 as Numeric>::into_i128(u64::MAX), i128::from(u64::MAX));
    /// ```
    fn into_i128(self) -> i128;

    /// Narrow `value` back into `Self`, or return `OutOfRange` when
    /// `value` does not fit.
    ///
    /// # Errors
    ///
    /// Returns `NumericError::OutOfRange { value }` when `value`
    /// cannot be represented as `Self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::{Numeric, NumericError};
    ///
    /// // Admit: value fits in `i32`.
    /// assert_eq!(<i32 as Numeric>::from_i128(42), Ok(42));
    ///
    /// // Reject: value exceeds `i8::MAX`.
    /// assert_eq!(
    ///     <i8 as Numeric>::from_i128(200),
    ///     Err(NumericError::OutOfRange { value: 200 }),
    /// );
    /// ```
    fn from_i128(value: i128) -> Result<Self, NumericError>;
}

macro_rules! impl_numeric_signed {
    ($($ty:ty),+) => { $(
        impl Numeric for $ty {
            #[inline]
            fn into_i128(self) -> i128 {
                i128::from(self)
            }
            #[inline]
            fn from_i128(value: i128) -> Result<Self, NumericError> {
                <$ty>::try_from(value)
                    .map_err(|_| NumericError::OutOfRange { value })
            }
        }
    )+ };
}

macro_rules! impl_numeric_unsigned {
    ($($ty:ty),+) => { $(
        impl Numeric for $ty {
            #[inline]
            fn into_i128(self) -> i128 {
                i128::from(self)
            }
            #[inline]
            fn from_i128(value: i128) -> Result<Self, NumericError> {
                <$ty>::try_from(value)
                    .map_err(|_| NumericError::OutOfRange { value })
            }
        }
    )+ };
}

impl_numeric_signed!(i8, i16, i32, i64);
impl_numeric_unsigned!(u8, u16, u32, u64);

// i128 needs its own impl because i128::from(self) doesn't exist
// (it would be identity); ditto round-trip.
impl Numeric for i128 {
    #[inline]
    fn into_i128(self) -> i128 {
        self
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Ok(value)
    }
}

// usize / isize widen through their architecture-specific size.
// A const-asserted upper bound on `<int>::BITS` keeps the cast
// in `into_i128` provably lossless, so we can use `i128::from`
// on the corresponding fixed-width primitive without any
// fallible-conversion path.
impl Numeric for usize {
    #[inline]
    fn into_i128(self) -> i128 {
        const {
            assert!(Self::BITS <= 64, "usize wider than 64 bits is unsupported");
        };
        i128::from(self as u64)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

impl Numeric for isize {
    #[inline]
    fn into_i128(self) -> i128 {
        const {
            assert!(Self::BITS <= 64, "isize wider than 64 bits is unsupported");
        };
        i128::from(self as i64)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

// ─── Proptest support: `NumericArbitrary`. ───────────────────────
//
// Each numeric `Rule` admits a contiguous (or near-contiguous)
// integer range. `NumericArbitrary` exposes a per-type strategy that
// emits values within a clamped range, so rules can compose their
// admissible region from `i128` bounds without rejection sampling.

/// Numeric types that expose a `proptest` range strategy.
///
/// Implementations clamp the requested `[min, max]` bounds (widened
/// to `i128`) to the type's own range and return a strategy whose
/// values are guaranteed to fit in `Self`. Rules like `Within<MIN,
/// MAX>` and `Positive` use this trait to construct their
/// `ArbitraryRule` strategies without rejection sampling.
///
/// `Copy` is required so the strategy can yield values through a
/// `fn(&T) -> bool` filter without `Clone`-ing.
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait NumericArbitrary: Numeric + Copy {
    /// Strategy type emitted by [`Self::arbitrary_in_range`].
    type RangeStrategy: proptest::strategy::Strategy<Value = Self>;

    /// Strategy that emits values in the inclusive `[min, max]`
    /// range, clamped to `Self`'s own range.
    ///
    /// If the clamped range is empty, implementations must still
    /// return a non-empty strategy (typically a single endpoint).
    /// Callers should not pass ranges that, after clamping, would
    /// be empty for the type they are generating; the rules in this
    /// module never do.
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy;
}

/// Generate `NumericArbitrary` impls for the supported integer
/// types. Each impl clamps the requested `[min, max]` bounds to the
/// type's own representable range before constructing a proptest
/// range strategy.
#[cfg(feature = "proptest")]
macro_rules! impl_numeric_arbitrary {
    ($($ty:ty),+) => { $(
        impl NumericArbitrary for $ty {
            type RangeStrategy = core::ops::RangeInclusive<$ty>;

            #[inline]
            fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
                let ty_min = i128::from(<$ty>::MIN);
                let ty_max = i128::from(<$ty>::MAX);
                let lo = if min < ty_min { ty_min } else { min };
                let hi = if max > ty_max { ty_max } else { max };
                // `lo, hi` are clamped to `[ty_min, ty_max]`,
                // which fits `Self` by construction; the
                // `try_from` fallbacks pin to the type's endpoints
                // for safety even though they cannot be reached
                // here.
                let lo = <$ty>::try_from(lo).unwrap_or(<$ty>::MIN);
                let hi = <$ty>::try_from(hi).unwrap_or(<$ty>::MAX);
                lo..=hi
            }
        }
    )+ };
}

#[cfg(feature = "proptest")]
impl_numeric_arbitrary!(i8, i16, i32, i64, u8, u16, u32, u64);

// `i128` cannot widen losslessly *from* itself via `i128::from`,
// but the bounds are already `i128`. Skip the conversion entirely
// and clamp in `i128`.
#[cfg(feature = "proptest")]
impl NumericArbitrary for i128 {
    type RangeStrategy = core::ops::RangeInclusive<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        min..=max
    }
}

// `usize` / `isize` widen through their architecture-specific size.
// On every supported platform `<int>::BITS <= 64`, so the i128
// bounds fit comfortably; clamp through that.
#[cfg(feature = "proptest")]
impl NumericArbitrary for usize {
    type RangeStrategy = core::ops::RangeInclusive<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        let ty_min: i128 = 0;
        let ty_max: i128 = i128::from(u64::MAX);
        let lo = if min < ty_min { ty_min } else { min };
        let hi = if max > ty_max { ty_max } else { max };
        // `lo, hi` are clamped to `[0, u64::MAX]`; both fit usize
        // on every supported platform (BITS <= 64 enforced
        // elsewhere).
        let lo = Self::try_from(lo).unwrap_or(0);
        let hi = Self::try_from(hi).unwrap_or(Self::MAX);
        lo..=hi
    }
}

#[cfg(feature = "proptest")]
impl NumericArbitrary for isize {
    type RangeStrategy = core::ops::RangeInclusive<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        let ty_min: i128 = i128::from(i64::MIN);
        let ty_max: i128 = i128::from(i64::MAX);
        let lo = if min < ty_min { ty_min } else { min };
        let hi = if max > ty_max { ty_max } else { max };
        let lo = Self::try_from(lo).unwrap_or(Self::MIN);
        let hi = Self::try_from(hi).unwrap_or(Self::MAX);
        lo..=hi
    }
}

// ─── Rule impls. ──────────────────────────────────────────────────
//
// `Within<MIN, MAX>` is a nominal newtype that delegates to the
// internal `And<AtLeast<MIN>, AtMost<MAX>>` composition. Both inner
// rules share `NumericError`, so the composition's error is
// `NumericError` directly — no flattening shim is needed.

impl<T, const MIN: i128, const MAX: i128> Rule<T> for Within<MIN, MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        const { assert!(MIN <= MAX, "Within: MIN must be <= MAX") };
        <crate::composition::And<AtLeast<MIN>, AtMost<MAX>> as Rule<T>>::refine(raw)
    }
}

impl<T, const MIN: i128> Rule<T> for AtLeast<MIN>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened < MIN {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T, const MAX: i128> Rule<T> for AtMost<MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened > MAX {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T> Rule<T> for NonZero
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened == 0_i128 {
            return Err(NumericError::OutOfRange { value: 0_i128 });
        }
        T::from_i128(widened)
    }
}

impl<T> Rule<T> for Positive
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened <= 0_i128 {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T> Rule<T> for Negative
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened >= 0_i128 {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// Each rule's strategy emits values that are admissible by
// construction: a `[min, max]` window clamped to the target type's
// range, plus a `prop_filter` for `NonZero` (where the admissible
// region is dense and the rejection-sampling cost is one in
// ~2^N).

#[cfg(feature = "proptest")]
impl<T, const MIN: i128, const MAX: i128> ArbitraryRule<T> for Within<MIN, MAX>
where
    T: NumericArbitrary + core::fmt::Debug,
{
    type Strategy = <T as NumericArbitrary>::RangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        const { assert!(MIN <= MAX, "Within: MIN must be <= MAX") };
        T::arbitrary_in_range(MIN, MAX)
    }
}

#[cfg(feature = "proptest")]
impl<T, const MIN: i128> ArbitraryRule<T> for AtLeast<MIN>
where
    T: NumericArbitrary + core::fmt::Debug,
{
    type Strategy = <T as NumericArbitrary>::RangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        T::arbitrary_in_range(MIN, i128::MAX)
    }
}

#[cfg(feature = "proptest")]
impl<T, const MAX: i128> ArbitraryRule<T> for AtMost<MAX>
where
    T: NumericArbitrary + core::fmt::Debug,
{
    type Strategy = <T as NumericArbitrary>::RangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        T::arbitrary_in_range(i128::MIN, MAX)
    }
}

#[cfg(feature = "proptest")]
fn numeric_is_non_zero<T: Numeric + Copy>(value: &T) -> bool {
    (*value).into_i128() != 0_i128
}

#[cfg(feature = "proptest")]
impl<T> ArbitraryRule<T> for NonZero
where
    T: NumericArbitrary + core::fmt::Debug,
{
    // `NonZero` admits every value except `0`; the admissible
    // region is dense (one excluded value out of ~2^N). Rejection
    // sampling on the full range is cheap.
    type Strategy =
        proptest::strategy::Filter<<T as NumericArbitrary>::RangeStrategy, fn(&T) -> bool>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        T::arbitrary_in_range(i128::MIN, i128::MAX)
            .prop_filter("non-zero", numeric_is_non_zero::<T>)
    }
}

#[cfg(feature = "proptest")]
impl<T> ArbitraryRule<T> for Positive
where
    T: NumericArbitrary + core::fmt::Debug,
{
    type Strategy = <T as NumericArbitrary>::RangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        T::arbitrary_in_range(1_i128, i128::MAX)
    }
}

#[cfg(feature = "proptest")]
impl<T> ArbitraryRule<T> for Negative
where
    T: NumericArbitrary + core::fmt::Debug,
{
    type Strategy = <T as NumericArbitrary>::RangeStrategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        T::arbitrary_in_range(i128::MIN, -1_i128)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString;

    use super::{AtLeast, AtMost, Negative, NonZero, NumericError, Positive, Within};
    use crate::rule::{Refined, Rule};

    refinement! {
        /// Macro-generated newtype for testing: `i32` in `0..=100`.
        ///
        /// Exists to exercise `refinement!` from the numeric test
        /// module so the macro is reached by more than just
        /// `macros.rs`'s own tests.
        #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
        pub Percent: i32, Within<0, 100>;
    }

    #[test]
    fn within_accepts_bounds_inclusive() {
        let zero: Refined<i32, Within<0, 100>> = Refined::try_new(0_i32).unwrap();
        let hundred: Refined<i32, Within<0, 100>> = Refined::try_new(100_i32).unwrap();
        assert_eq!(*zero.as_inner(), 0_i32);
        assert_eq!(*hundred.as_inner(), 100_i32);
    }

    #[test]
    fn within_rejects_out_of_range() {
        // Both inner rules of `Within`'s composition share
        // `NumericError`, so the domain error surfaces directly for
        // both sides — no flattening shim required.
        let neg: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(-1_i32);
        assert_eq!(
            neg.unwrap_err(),
            NumericError::OutOfRange { value: -1_i128 },
        );
        let big: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(101_i32);
        assert_eq!(
            big.unwrap_err(),
            NumericError::OutOfRange { value: 101_i128 },
        );
    }

    #[test]
    fn at_least_at_most_compose_via_within() {
        // The primitives are independent; chained checks are demonstrated
        // here through manual sequencing.
        let inside = <AtLeast<5> as Rule<i32>>::refine(7_i32).unwrap();
        let inside = <AtMost<10> as Rule<i32>>::refine(inside).unwrap();
        assert_eq!(inside, 7_i32);
    }

    #[test]
    fn non_zero_rejects_zero_and_accepts_nonzero() {
        let result: Result<Refined<i32, NonZero>, _> = Refined::try_new(0_i32);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 0_i128 }
        );
        let accept: Refined<i32, NonZero> = Refined::try_new(-3_i32).unwrap();
        assert_eq!(*accept.as_inner(), -3_i32);
    }

    #[test]
    fn positive_negative_partition() {
        let p: Refined<i32, Positive> = Refined::try_new(1_i32).unwrap();
        let n: Refined<i32, Negative> = Refined::try_new(-1_i32).unwrap();
        assert_eq!(*p.as_inner(), 1_i32);
        assert_eq!(*n.as_inner(), -1_i32);

        let p_zero: Result<Refined<i32, Positive>, _> = Refined::try_new(0_i32);
        p_zero.unwrap_err();
        let n_zero: Result<Refined<i32, Negative>, _> = Refined::try_new(0_i32);
        n_zero.unwrap_err();
    }

    #[test]
    fn within_works_for_unsigned_types() {
        let v: Refined<u8, Within<0, 100>> = Refined::try_new(42_u8).unwrap();
        assert_eq!(*v.as_inner(), 42_u8);
    }

    #[test]
    fn out_of_range_for_narrower_type_reports_underlying_value() {
        // i8 narrowing to fit i128 of 200 fails — 200 > i8::MAX.
        let result = <i8 as super::Numeric>::from_i128(200_i128);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    // ─── Numeric trait coverage for every integer impl. ──────────
    //
    // Each Numeric impl is its own monomorphization, so we round-trip
    // every variant through Within to exercise both `into_i128` and
    // `from_i128`. The cases here are deliberately minimal: a single
    // admissible round-trip per type is enough to take all branches.

    #[test]
    fn within_round_trip_i16() {
        let v: Refined<i16, Within<-100, 100>> = Refined::try_new(42_i16).unwrap();
        assert_eq!(*v.as_inner(), 42_i16);
    }

    #[test]
    fn within_round_trip_i64() {
        let v: Refined<i64, Within<-100, 100>> = Refined::try_new(42_i64).unwrap();
        assert_eq!(*v.as_inner(), 42_i64);
    }

    #[test]
    fn within_round_trip_u16() {
        let v: Refined<u16, Within<0, 100>> = Refined::try_new(42_u16).unwrap();
        assert_eq!(*v.as_inner(), 42_u16);
    }

    #[test]
    fn within_round_trip_u32() {
        let v: Refined<u32, Within<0, 100>> = Refined::try_new(42_u32).unwrap();
        assert_eq!(*v.as_inner(), 42_u32);
    }

    #[test]
    fn within_round_trip_u64() {
        let v: Refined<u64, Within<0, 100>> = Refined::try_new(42_u64).unwrap();
        assert_eq!(*v.as_inner(), 42_u64);
    }

    #[test]
    fn within_round_trip_i128() {
        let v: Refined<i128, Within<-100, 100>> = Refined::try_new(42_i128).unwrap();
        assert_eq!(*v.as_inner(), 42_i128);
    }

    #[test]
    fn within_round_trip_usize() {
        let v: Refined<usize, Within<0, 100>> = Refined::try_new(42_usize).unwrap();
        assert_eq!(*v.as_inner(), 42_usize);
    }

    #[test]
    fn within_round_trip_isize() {
        let v: Refined<isize, Within<-100, 100>> = Refined::try_new(42_isize).unwrap();
        assert_eq!(*v.as_inner(), 42_isize);
    }

    // Failure-path coverage for the from_i128 narrowing branch on
    // usize and isize. (i128 cannot fail conversion; its from_i128
    // is Ok((self)).)
    #[test]
    fn within_rejects_overflow_for_usize() {
        let result: Result<Refined<usize, Within<0, 100>>, _> = Refined::try_new(200_usize);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    #[test]
    fn within_rejects_overflow_for_isize() {
        let result: Result<Refined<isize, Within<-100, 100>>, _> = Refined::try_new(200_isize);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    // `Numeric::from_i128` is a per-type entry point reached not
    // only through `Within::refine` but also as part of the wider
    // `Rule` surface; covering its narrowing failure branch
    // requires a value that fits in i128 but not in the target.
    //
    // Within<MIN, MAX>::refine clamps before calling from_i128, so
    // the closure is only reached when the value passes the
    // refinement check but is then out of the target's range. We
    // call from_i128 directly here to exercise each impl's
    // narrowing-error closure once.

    #[test]
    fn within_round_trip_i8() {
        // Exercises i8's into_i128 + Within::refine monomorphization.
        let v: Refined<i8, Within<-100, 100>> = Refined::try_new(42_i8).unwrap();
        assert_eq!(*v.as_inner(), 42_i8);
    }

    fn assert_from_i128_overflow<T: super::Numeric>(value: i128) {
        let result = <T as super::Numeric>::from_i128(value);
        assert_eq!(result.err(), Some(NumericError::OutOfRange { value }),);
    }

    #[test]
    fn from_i128_overflow_signed() {
        // Each macro-generated signed impl gets its narrowing
        // closure exercised once.
        assert_from_i128_overflow::<i8>(200_i128);
        assert_from_i128_overflow::<i16>(40_000_i128);
        assert_from_i128_overflow::<i32>(3_000_000_000_i128);
        assert_from_i128_overflow::<i64>(i128::MAX);
    }

    #[test]
    fn from_i128_overflow_unsigned() {
        // Each macro-generated unsigned impl. Negative values are
        // always out of range for unsigned types.
        assert_from_i128_overflow::<u8>(-1_i128);
        assert_from_i128_overflow::<u16>(-1_i128);
        assert_from_i128_overflow::<u32>(-1_i128);
        assert_from_i128_overflow::<u64>(-1_i128);
    }

    #[test]
    fn from_i128_overflow_pointer_sized() {
        assert_from_i128_overflow::<usize>(-1_i128);
        assert_from_i128_overflow::<isize>(i128::MAX);
    }

    #[test]
    fn display_formats_out_of_range_variant() {
        // Exercise the hand-rolled `Display` arm for the only
        // `NumericError` variant. Pairing with `core::error::Error`
        // via the `dyn Error` cast confirms the trait impl is live.
        let err = NumericError::OutOfRange { value: -7_i128 };
        assert_eq!(err.to_string(), "value -7 not in admissible range");
        let dyn_err: &dyn core::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn refinement_macro_percent_admits_and_rejects() {
        // Exercises the macro-generated newtype: admit a mid-range
        // value, reject above MAX. Confirms `refinement!` reaches the
        // numeric primitive test module.
        let ok = Percent::try_new(42_i32).unwrap();
        assert_eq!(*ok.as_inner(), 42_i32);
        let owned: i32 = ok.into_inner();
        assert_eq!(owned, 42_i32);
        let bad = Percent::try_new(101_i32);
        bad.unwrap_err();
    }

    proptest::proptest! {
        #[test]
        fn within_round_trips_admissible(x in 0_i32..=100_i32) {
            let r: Refined<i32, Within<0, 100>> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

        #[test]
        fn within_rejects_below_min(x in i32::MIN..0_i32) {
            // kernel-only: domain code wraps this composition in a
            // newtype with a flat error enum — see SKILL.md
            // "Newtype hiding rule composition".
            let result: Result<Refined<i32, Within<0, 100>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_least_rejects_below_min(x in i32::MIN..10_i32) {
            let result: Result<Refined<i32, AtLeast<10>>, _> = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn non_zero_round_trips_nonzero(x in proptest::arbitrary::any::<i32>()) {
            proptest::prop_assume!(x != 0_i32);
            let r: Refined<i32, NonZero> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

        // ─── Self-hosted Arbitrary round-trips. Every value
        //     generated by the `Refined<T, R>` Arbitrary strategy
        //     must satisfy `R` by construction.

        #[test]
        fn arbitrary_within_is_in_range(x in 0_i32..=100_i32) {
            // `Within<0, 100>` admits 101 values out of 2^32 — too
            // sparse for `arbitrary::any::<Refined<…>>()` rejection
            // sampling. Drive with a bounded inner strategy and
            // route through `try_new` to exercise the rule on the
            // full admissible region instead.
            //
            // kernel-only: domain code wraps this composition in a
            // newtype with a flat error enum — see SKILL.md
            // "Newtype hiding rule composition".
            let r: Refined<i32, Within<0, 100>>
                = Refined::try_new(x).unwrap();
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_at_least_is_above_min(
            r in proptest::arbitrary::any::<Refined<i32, AtLeast<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() >= 10);
        }

        #[test]
        fn arbitrary_at_most_is_below_max(
            r in proptest::arbitrary::any::<Refined<i32, AtMost<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() <= 10);
        }

        #[test]
        fn arbitrary_non_zero_is_nonzero(
            r in proptest::arbitrary::any::<Refined<i32, NonZero>>()
        ) {
            proptest::prop_assert!(*r.as_inner() != 0);
        }

        #[test]
        fn arbitrary_positive_is_positive(
            r in proptest::arbitrary::any::<Refined<i32, Positive>>()
        ) {
            proptest::prop_assert!(*r.as_inner() > 0);
        }

        #[test]
        fn arbitrary_negative_is_negative(
            r in proptest::arbitrary::any::<Refined<i32, Negative>>()
        ) {
            proptest::prop_assert!(*r.as_inner() < 0);
        }

        // ─── Reject properties: bounded ranges. ────────────────

        #[test]
        fn within_rejects_strictly_above_max(x in 101_i32..=i32::MAX) {
            let result: Result<Refined<i32, Within<0, 100>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_least_rejects_strictly_below_min_band(
            x in i32::MIN..10_i32
        ) {
            let result: Result<Refined<i32, AtLeast<10>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_most_rejects_strictly_above_max_band(
            x in 11_i32..=i32::MAX
        ) {
            let result: Result<Refined<i32, AtMost<10>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }
    }
}
