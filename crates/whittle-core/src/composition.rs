//! Binary rule composition: `And<A, B>` and `Or<A, B>`.
//!
//! N-ary composition is expressed by nesting (`And<A, And<B, C>>`);
//! the future `refinement!` declarative macro performs the nesting
//! on the user's behalf.

use core::marker::PhantomData;

use thiserror::Error;

use crate::rule::Rule;

/// Both rules must accept. `A::refine` runs first; on success its
/// (possibly canonicalised) output is passed to `B::refine`.
pub struct And<A, B>(PhantomData<(A, B)>);

/// Either rule may accept. `A::refine` runs first; on `Ok` its
/// output is the result, on `Err` `B::refine` is tried against the
/// original input.
pub struct Or<A, B>(PhantomData<(A, B)>);

/// Error from `And<A, B>`: the side that rejected and its rule's
/// error value.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AndError<EA, EB> {
    /// `A::refine` rejected the input.
    #[error("left side: {0}")]
    Left(#[source] EA),
    /// `A::refine` accepted, but `B::refine` then rejected.
    #[error("right side: {0}")]
    Right(#[source] EB),
}

/// Error from `Or<A, B>`: both sides rejected.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("both sides rejected: left {left}, right {right}")]
pub struct OrError<EA, EB> {
    /// The left rule's error.
    pub left: EA,
    /// The right rule's error.
    pub right: EB,
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
#[allow(clippy::unwrap_used, clippy::expect_used,
        reason = "explicit in test code")]
mod tests {
    use alloc::string::{String, ToString};

    use super::{And, AndError, Or, OrError};
    use crate::primitive::{
        AtLeast, AtMost, EachChar, IdentChar, LenChars, NonZero,
        NumericError, StringError,
    };
    use crate::rule::Refined;

    #[test]
    fn and_passes_through_canonicalised_output() {
        // Compose two numeric rules: `>= 0` and `<= 100`.
        let r: Refined<i32, And<AtLeast<0>, AtMost<100>>>
            = Refined::try_new(50_i32).unwrap();
        assert_eq!(*r.as_inner(), 50_i32);
    }

    #[test]
    fn and_reports_left_failure() {
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _>
            = Refined::try_new(-1_i32);
        assert!(matches!(
            result.unwrap_err(),
            AndError::Left(NumericError::OutOfRange { value: -1 }),
        ));
    }

    #[test]
    fn and_reports_right_failure() {
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _>
            = Refined::try_new(101_i32);
        assert!(matches!(
            result.unwrap_err(),
            AndError::Right(NumericError::OutOfRange { value: 101 }),
        ));
    }

    #[test]
    fn and_combines_string_primitives() {
        // 1..=10 char identifier-body string — the shape an
        // `AttributeName` would use.
        type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
        let ok: Refined<String, Ident>
            = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "user_42");

        let bad_len: Result<Refined<String, Ident>, _>
            = Refined::try_new(String::new());
        assert!(matches!(
            bad_len.unwrap_err(),
            AndError::Left(StringError::CharCountOutOfRange { actual: 0 }),
        ));

        let bad_char: Result<Refined<String, Ident>, _>
            = Refined::try_new("user-42".to_string());
        assert!(matches!(
            bad_char.unwrap_err(),
            AndError::Right(StringError::BadChar { offset: 4 }),
        ));
    }

    #[test]
    fn or_accepts_when_either_side_accepts() {
        // Even on the left, divisible-by-3 on the right (simulated
        // here with a simple range — `Or` is most useful when one
        // alternative is a normalising fallback).
        type Either = Or<AtMost<10>, AtLeast<100>>;
        let small: Refined<i32, Either>
            = Refined::try_new(5_i32).unwrap();
        let big: Refined<i32, Either>
            = Refined::try_new(150_i32).unwrap();
        assert_eq!(*small.as_inner(), 5_i32);
        assert_eq!(*big.as_inner(), 150_i32);
    }

    #[test]
    fn or_reports_both_failures() {
        type Either = Or<AtMost<10>, AtLeast<100>>;
        let result: Result<Refined<i32, Either>, _>
            = Refined::try_new(50_i32);
        let err: OrError<NumericError, NumericError> = result.unwrap_err();
        assert!(matches!(err.left, NumericError::OutOfRange { value: 50 }));
        assert!(matches!(err.right, NumericError::OutOfRange { value: 50 }));
    }

    #[test]
    fn nested_and_for_three_rules() {
        // Compose three rules through binary nesting.
        type Triple = And<NonZero, And<AtLeast<-10>, AtMost<10>>>;
        let ok: Refined<i32, Triple> = Refined::try_new(5_i32).unwrap();
        assert_eq!(*ok.as_inner(), 5_i32);
        let bad: Result<Refined<i32, Triple>, _>
            = Refined::try_new(0_i32);
        assert!(matches!(
            bad.unwrap_err(),
            AndError::Left(NumericError::OutOfRange { value: 0 }),
        ));
    }
}
