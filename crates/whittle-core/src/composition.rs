//! Binary rule composition: `And<A, B>` and `Or<A, B>`.
//!
//! N-ary composition is expressed by nesting (`And<A, And<B, C>>`);
//! the future `refinement!` declarative macro performs the nesting
//! on the user's behalf.

use core::marker::PhantomData;

use crate::rule::Rule;

/// Both rules must accept. `A::refine` runs first; on success its
/// (possibly canonicalised) output is passed to `B::refine`.
///
/// # Examples
///
/// ```
/// use whittle_core::{And, AndError, Refined};
/// use whittle_core::primitive::{AtLeast, AtMost, NumericError};
///
/// // `0..=100` expressed as `AtLeast<0> AND AtMost<100>`.
/// type InRange = And<AtLeast<0>, AtMost<100>>;
///
/// // Admit: both rules accept.
/// let ok: Refined<i32, InRange> = Refined::try_new(50).unwrap();
/// assert_eq!(*ok.as_inner(), 50);
///
/// // Reject from the left: below the lower bound.
/// let err_left = Refined::<i32, InRange>::try_new(-1).unwrap_err();
/// assert_eq!(
///     err_left,
///     AndError::Left(NumericError::OutOfRange { value: -1 }),
/// );
///
/// // Reject from the right: above the upper bound.
/// let err_right = Refined::<i32, InRange>::try_new(101).unwrap_err();
/// assert_eq!(
///     err_right,
///     AndError::Right(NumericError::OutOfRange { value: 101 }),
/// );
/// ```
pub struct And<A, B>(PhantomData<(A, B)>);

/// Either rule may accept. `A::refine` runs first; on `Ok` its
/// output is the result, on `Err` `B::refine` is tried against the
/// original input.
///
/// # Examples
///
/// ```
/// use whittle_core::{Or, Refined};
/// use whittle_core::primitive::{AtLeast, AtMost, NumericError};
///
/// // `value <= 10 OR value >= 100`.
/// type Either = Or<AtMost<10>, AtLeast<100>>;
///
/// // Admit-via-left: 5 passes `AtMost<10>`.
/// let small: Refined<i32, Either> = Refined::try_new(5).unwrap();
/// assert_eq!(*small.as_inner(), 5);
///
/// // Admit-via-right: 150 passes `AtLeast<100>`.
/// let big: Refined<i32, Either> = Refined::try_new(150).unwrap();
/// assert_eq!(*big.as_inner(), 150);
///
/// // Reject: neither alternative accepts 50.
/// let err = Refined::<i32, Either>::try_new(50).unwrap_err();
/// assert_eq!(err.left, NumericError::OutOfRange { value: 50 });
/// assert_eq!(err.right, NumericError::OutOfRange { value: 50 });
/// ```
pub struct Or<A, B>(PhantomData<(A, B)>);

/// Error from `And<A, B>`: the side that rejected and its rule's
/// error value.
#[derive(Debug, PartialEq, Eq)]
pub enum AndError<EA, EB> {
    /// `A::refine` rejected the input.
    Left(EA),
    /// `A::refine` accepted, but `B::refine` then rejected.
    Right(EB),
}

impl<EA, EB> core::fmt::Display for AndError<EA, EB>
where
    EA: core::fmt::Display,
    EB: core::fmt::Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Left(err) => write!(f, "left side: {err}"),
            Self::Right(err) => write!(f, "right side: {err}"),
        }
    }
}

impl<EA, EB> core::error::Error for AndError<EA, EB>
where
    EA: core::error::Error + 'static,
    EB: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Left(err) => Some(err),
            Self::Right(err) => Some(err),
        }
    }
}

/// Error from `Or<A, B>`: both sides rejected.
#[derive(Debug, PartialEq, Eq)]
pub struct OrError<EA, EB> {
    /// The left rule's error.
    pub left: EA,
    /// The right rule's error.
    pub right: EB,
}

impl<EA, EB> core::fmt::Display for OrError<EA, EB>
where
    EA: core::fmt::Display,
    EB: core::fmt::Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self { left, right } = self;
        write!(f, "both sides rejected: left {left}, right {right}")
    }
}

impl<EA, EB> core::error::Error for OrError<EA, EB>
where
    EA: core::error::Error + 'static,
    EB: core::error::Error + 'static,
{
}

impl<T, A, B> Rule<T> for And<A, B>
where
    T: 'static,
    A: Rule<T>,
    B: Rule<T>,
{
    type Error = AndError<A::Error, B::Error>;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        match A::refine(raw) {
            Ok(value) => B::refine(value).map_err(AndError::Right),
            Err(err) => Err(AndError::Left(err)),
        }
    }
}

impl<T, A, B> Rule<T> for Or<A, B>
where
    T: 'static + Clone,
    A: Rule<T>,
    B: Rule<T>,
{
    type Error = OrError<A::Error, B::Error>;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        // `Or` retries the original input against `B` if `A`
        // rejects, so the input must be cloned before the first
        // attempt. This is the only place in the kernel that
        // requires `T: Clone`.
        let snapshot = raw.clone();
        match A::refine(raw) {
            Ok(value) => Ok(value),
            Err(left) => match B::refine(snapshot) {
                Ok(value) => Ok(value),
                Err(right) => Err(OrError { left, right }),
            },
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{And, AndError, Or, OrError};
    use crate::primitive::{
        AtLeast, AtMost, EachChar, IdentChar, LenChars, NonZero, NumericError, StringError,
    };
    use crate::rule::Refined;

    crate::refinement! {
        /// Macro-generated newtype for testing: `i32` in `0..=100`,
        /// expressed as a binary `And` composition. Exercises
        /// `refinement!` from the composition test module.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub Bounded100: i32, And<AtLeast<0>, AtMost<100>>;
    }

    crate::refinement! {
        /// Macro-generated newtype for testing: `i32` outside the
        /// `10..=99` band (i.e. `<=10 OR >=100`). Exercises the
        /// `Or` composition through the macro.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub OutOfMiddle: i32, Or<AtMost<10>, AtLeast<100>>;
    }

    #[test]
    fn and_passes_through_canonicalised_output() {
        // Compose two numeric rules: `>= 0` and `<= 100`.
        let r: Refined<i32, And<AtLeast<0>, AtMost<100>>> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*r.as_inner(), 50_i32);
    }

    #[test]
    fn and_reports_left_failure() {
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _> =
            Refined::try_new(-1_i32);
        assert_eq!(
            result.unwrap_err(),
            AndError::Left(NumericError::OutOfRange { value: -1 }),
        );
    }

    #[test]
    fn and_reports_right_failure() {
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _> =
            Refined::try_new(101_i32);
        assert_eq!(
            result.unwrap_err(),
            AndError::Right(NumericError::OutOfRange { value: 101 }),
        );
    }

    #[test]
    fn and_combines_string_primitives() {
        // 1..=10 char identifier-body string — the shape an
        // `AttributeName` would use.
        type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
        let ok: Refined<String, Ident> = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "user_42");

        let bad_len: Result<Refined<String, Ident>, _> = Refined::try_new(String::new());
        assert_eq!(
            bad_len.unwrap_err(),
            AndError::Left(StringError::CharCountOutOfRange { actual: 0 }),
        );

        let bad_char: Result<Refined<String, Ident>, _> = Refined::try_new("user-42".to_string());
        assert_eq!(
            bad_char.unwrap_err(),
            AndError::Right(StringError::BadChar { offset: 4 }),
        );
    }

    #[test]
    fn or_accepts_when_either_side_accepts() {
        // Even on the left, divisible-by-3 on the right (simulated
        // here with a simple range — `Or` is most useful when one
        // alternative is a normalising fallback).
        type Either = Or<AtMost<10>, AtLeast<100>>;
        let small: Refined<i32, Either> = Refined::try_new(5_i32).unwrap();
        let big: Refined<i32, Either> = Refined::try_new(150_i32).unwrap();
        assert_eq!(*small.as_inner(), 5_i32);
        assert_eq!(*big.as_inner(), 150_i32);
    }

    #[test]
    fn or_reports_both_failures() {
        type Either = Or<AtMost<10>, AtLeast<100>>;
        let result: Result<Refined<i32, Either>, _> = Refined::try_new(50_i32);
        let err: OrError<NumericError, NumericError> = result.unwrap_err();
        assert_eq!(err.left, NumericError::OutOfRange { value: 50 });
        assert_eq!(err.right, NumericError::OutOfRange { value: 50 });
    }

    #[test]
    fn and_error_display_and_source_chain() {
        // Hand-rolled `Display` covers both `Left` / `Right` arms;
        // `Error::source` chains the inner error for both arms.
        let left: AndError<NumericError, NumericError> =
            AndError::Left(NumericError::OutOfRange { value: -1_i128 });
        let right: AndError<NumericError, NumericError> =
            AndError::Right(NumericError::OutOfRange { value: 7_i128 });
        assert_eq!(
            left.to_string(),
            "left side: value -1 not in admissible range",
        );
        assert_eq!(
            right.to_string(),
            "right side: value 7 not in admissible range",
        );
        let dyn_left: &dyn core::error::Error = &left;
        let dyn_right: &dyn core::error::Error = &right;
        assert!(dyn_left.source().is_some());
        assert!(dyn_right.source().is_some());
    }

    #[test]
    fn or_error_display_has_no_source_chain() {
        // Hand-rolled `Display` joins both inner errors; `OrError`
        // intentionally does not expose a source (no single inner
        // error is "the" cause).
        let err: OrError<NumericError, NumericError> = OrError {
            left: NumericError::OutOfRange { value: 50_i128 },
            right: NumericError::OutOfRange { value: 50_i128 },
        };
        assert_eq!(
            err.to_string(),
            "both sides rejected: left value 50 not in admissible range, \
             right value 50 not in admissible range",
        );
        let dyn_err: &dyn core::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn refinement_macro_bounded_admits_and_rejects_and() {
        // Macro-generated `And` newtype: admit a mid-range value,
        // reject above MAX through the right branch.
        let ok = Bounded100::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let owned: i32 = ok.into_inner();
        assert_eq!(owned, 50_i32);
        let bad = Bounded100::try_new(101_i32);
        assert!(bad.is_err());
    }

    #[test]
    fn refinement_macro_out_of_middle_admits_and_rejects_or() {
        // Macro-generated `Or` newtype: admit on either alternative,
        // reject when both alternatives fail.
        let small = OutOfMiddle::try_new(5_i32).unwrap();
        let big = OutOfMiddle::try_new(150_i32).unwrap();
        assert_eq!(*small.as_inner(), 5_i32);
        assert_eq!(*big.as_inner(), 150_i32);
        let owned: i32 = big.into_inner();
        assert_eq!(owned, 150_i32);
        let bad = OutOfMiddle::try_new(50_i32);
        assert!(bad.is_err());
    }

    #[test]
    fn nested_and_for_three_rules() {
        // Compose three rules through binary nesting.
        type Triple = And<NonZero, And<AtLeast<-10>, AtMost<10>>>;
        let ok: Refined<i32, Triple> = Refined::try_new(5_i32).unwrap();
        assert_eq!(*ok.as_inner(), 5_i32);
        let bad: Result<Refined<i32, Triple>, _> = Refined::try_new(0_i32);
        assert_eq!(
            bad.unwrap_err(),
            AndError::Left(NumericError::OutOfRange { value: 0 }),
        );
    }

    proptest::proptest! {
        // ─── Self-hosted Arbitrary on composed rules. The kernel's
        //     `Refined<T, R>` Arbitrary impl applies rejection
        //     sampling, so any value emitted by these strategies
        //     must satisfy the composition.

        #[test]
        fn arbitrary_and_admits_only_in_range(
            x in 0_i32..=100_i32
        ) {
            // `And<AtLeast<0>, AtMost<100>>` admits 101 / 2^32
            // values — too sparse for `arbitrary::any::<Refined<…>>`
            // rejection sampling. Drive with a bounded inner
            // strategy and route through `try_new` so the rule's
            // composed refine path is still exercised.
            let r: Refined<i32, And<AtLeast<0>, AtMost<100>>>
                = Refined::try_new(x).unwrap();
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_or_admits_only_outside_middle(
            r in proptest::arbitrary::any::<
                Refined<i32, Or<AtMost<0>, AtLeast<100>>>,
            >()
        ) {
            let value = *r.as_inner();
            // `Or<AtMost<0>, AtLeast<100>>` admits roughly half
            // of i32 (everything except the 1..=99 band), so
            // rejection sampling is cheap here.
            proptest::prop_assert!(value <= 0 || value >= 100);
        }
    }
}
