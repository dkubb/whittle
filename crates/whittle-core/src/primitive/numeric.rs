//! Numeric primitive rules.
//!
//! Bounded ranges (`Within<MIN, MAX>`, `AtLeast<MIN>`, `AtMost<MAX>`)
//! and sign / non-zero markers (`NonZero`, `Positive`, `Negative`).
//! Each primitive carries a typed error variant that includes the
//! offending value so callers can construct precise diagnostics.

use core::marker::PhantomData;

use thiserror::Error;

use crate::rule::Rule;

/// Inclusive numeric range: `MIN <= value <= MAX`.
///
/// `MIN > MAX` is an empty interval; the rule rejects every input
/// with `NumericError::EmptyRange` on the first call. A `const_assert`
/// would catch this at compile time, but stable Rust does not yet
/// admit the comparison in a `where` clause.
pub struct Within<const MIN: i128, const MAX: i128>;

/// Lower-bound rule: `MIN <= value`.
pub struct AtLeast<const MIN: i128>;

/// Upper-bound rule: `value <= MAX`.
pub struct AtMost<const MAX: i128>;

/// Rejects zero.
pub struct NonZero;

/// `value > 0`.
pub struct Positive;

/// `value < 0`.
pub struct Negative;

/// Error variants common to every numeric primitive.
///
/// The variant carries the offending value as `i128` because every
/// supported numeric type widens losslessly into `i128`. Unsigned
/// inputs that exceed `i128::MAX` are reported via
/// `NumericError::OutOfRange`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericError {
    /// `Within<MIN, MAX>` declared with `MIN > MAX`. The interval
    /// is empty so no input is admissible.
    #[error("empty range")]
    EmptyRange,

    /// Value lies outside the rule's admissible range.
    #[error("value {value} not in admissible range")]
    OutOfRange { value: i128 },

    /// Value cannot be expressed as `i128` (only triggered by
    /// `u128` inputs above `i128::MAX`; reserved for future
    /// primitives that support `u128`).
    #[error("value does not fit in i128")]
    UnrepresentableInI128,
}

/// Conversion from a concrete numeric type into and back out of
/// `i128`, used by every numeric primitive's `Rule` impl.
///
/// Implementations exist for the standard signed and unsigned
/// integer types. `u128` is deliberately omitted: it cannot
/// round-trip through `i128`. A future `WithinUnsigned` family will
/// cover the upper half of `u128`.
pub trait Numeric: Sized + 'static {
    /// Widen `self` into an `i128`.
    ///
    /// # Errors
    ///
    /// Returns `NumericError::UnrepresentableInI128` if `Self` is a
    /// `usize` whose architecture-specific width exceeds 128 bits
    /// (not reachable on current platforms; reserved for future
    /// numeric types).
    fn into_i128(self) -> Result<i128, NumericError>;

    /// Narrow `value` back into `Self`, or return `OutOfRange` when
    /// `value` does not fit.
    ///
    /// # Errors
    ///
    /// Returns `NumericError::OutOfRange { value }` when `value`
    /// cannot be represented as `Self`.
    fn from_i128(value: i128) -> Result<Self, NumericError>;
}

macro_rules! impl_numeric_signed {
    ($($ty:ty),+) => { $(
        impl Numeric for $ty {
            #[inline]
            fn into_i128(self) -> Result<i128, NumericError> {
                Ok(i128::from(self))
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
            fn into_i128(self) -> Result<i128, NumericError> {
                Ok(i128::from(self))
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
    fn into_i128(self) -> Result<i128, NumericError> {
        Ok(self)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Ok(value)
    }
}

// usize / isize widen through their architecture-specific size.
impl Numeric for usize {
    #[inline]
    fn into_i128(self) -> Result<i128, NumericError> {
        i128::try_from(self).map_err(|_| NumericError::UnrepresentableInI128)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

impl Numeric for isize {
    #[inline]
    fn into_i128(self) -> Result<i128, NumericError> {
        Ok(i128::try_from(self).unwrap_or_else(|_| unreachable!()))
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

// ─── Rule impls. ──────────────────────────────────────────────────

impl<T, const MIN: i128, const MAX: i128> Rule<T> for Within<MIN, MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        if MIN > MAX {
            return Err(NumericError::EmptyRange);
        }
        let widened = raw.into_i128()?;
        if widened < MIN || widened > MAX {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T, const MIN: i128> Rule<T> for AtLeast<MIN>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128()?;
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
        let widened = raw.into_i128()?;
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
        let widened = raw.into_i128()?;
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
        let widened = raw.into_i128()?;
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
        let widened = raw.into_i128()?;
        if widened >= 0_i128 {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

// `PhantomData` lives here so it's unused by the `impl`s above but
// suppressed by the never-constructed marker shape; restate to keep
// the import live.
const _: PhantomData<()> = PhantomData;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used,
        reason = "explicit in test code")]
mod tests {
    use super::{AtLeast, AtMost, Negative, NonZero, NumericError,
                Positive, Within};
    use crate::rule::{Refined, Rule};

    #[test]
    fn within_accepts_bounds_inclusive() {
        let zero: Refined<i32, Within<0, 100>> = Refined::try_new(0_i32).unwrap();
        let hundred: Refined<i32, Within<0, 100>> = Refined::try_new(100_i32).unwrap();
        assert_eq!(*zero.as_inner(), 0_i32);
        assert_eq!(*hundred.as_inner(), 100_i32);
    }

    #[test]
    fn within_rejects_out_of_range() {
        let neg: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(-1_i32);
        assert_eq!(neg.unwrap_err(), NumericError::OutOfRange { value: -1_i128 });
        let big: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(101_i32);
        assert_eq!(big.unwrap_err(), NumericError::OutOfRange { value: 101_i128 });
    }

    #[test]
    fn within_empty_range_rejects_all() {
        let result: Result<Refined<i32, Within<10, 0>>, _> = Refined::try_new(5_i32);
        assert_eq!(result.unwrap_err(), NumericError::EmptyRange);
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
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: 0_i128 });
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
        assert!(p_zero.is_err());
        let n_zero: Result<Refined<i32, Negative>, _> = Refined::try_new(0_i32);
        assert!(n_zero.is_err());
    }

    #[test]
    fn within_works_for_unsigned_types() {
        let v: Refined<u8, Within<0, 100>> = Refined::try_new(42_u8).unwrap();
        assert_eq!(*v.as_inner(), 42_u8);
    }

    #[test]
    fn out_of_range_for_narrower_type_reports_underlying_value() {
        // i8 narrowing to fit i128 of 200 fails — 200 > i8::MAX.
        let result: Result<Refined<i8, Within<-128, 127>>, _>
            = <i8 as super::Numeric>::from_i128(200_i128)
                .map(|_| -> Refined<i8, Within<-128, 127>> { unreachable!() });
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: 200_i128 });
    }

    proptest::proptest! {
        #[test]
        fn within_round_trips_admissible(x in 0_i32..=100_i32) {
            let r: Refined<i32, Within<0, 100>> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

        #[test]
        fn within_rejects_below_min(x in i32::MIN..0_i32) {
            let result: Result<Refined<i32, Within<0, 100>>, _>
                = Refined::try_new(x);
            proptest::prop_assert!(result.is_err());
        }

        #[test]
        fn at_least_rejects_below_min(x in i32::MIN..10_i32) {
            let result: Result<Refined<i32, AtLeast<10>>, _> = Refined::try_new(x);
            proptest::prop_assert!(result.is_err());
        }

        #[test]
        fn non_zero_round_trips_nonzero(x in proptest::arbitrary::any::<i32>()) {
            proptest::prop_assume!(x != 0_i32);
            let r: Refined<i32, NonZero> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }
    }
}
