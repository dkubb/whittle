//! Binary rule composition: `And<A, B>` and `Or<A, B>`.
//!
//! Both rules must share the same `Rule::Error` type. The
//! composition's `Self::Error` is that shared type — no positional
//! `Left` / `Right` wrapping is exposed to callers. Domain newtypes
//! pattern-match on the rules' flat error enum directly.
//!
//! N-ary composition is expressed via nesting until N-ary lands —
//! `And<A, And<B, C>>` today; the `All<(...)>` / `Any<(...)>`
//! operators are planned follow-up.

use core::marker::PhantomData;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// Both rules must accept. `A::refine` runs first; on success its
/// (possibly canonicalised) output is passed to `B::refine`.
///
/// Both rules' `Error` types must unify. The composition's
/// `Self::Error` is that shared type — neither rule's failure is
/// wrapped, so callers pattern-match the rules' flat error enum
/// directly.
///
/// # Examples
///
/// ```
/// use whittle_core::{And, Refined};
/// use whittle_core::primitive::{AtLeast, AtMost, NumericError};
///
/// // `0..=100` expressed as `AtLeast<0> AND AtMost<100>`. Both
/// // rules produce `NumericError`, so the composition's error is
/// // `NumericError` directly — no `Left` / `Right` wrapping.
/// type InRange = And<AtLeast<0>, AtMost<100>>;
///
/// // Admit: both rules accept.
/// let ok: Refined<i32, InRange> = Refined::try_new(50).unwrap();
/// assert_eq!(*ok.as_inner(), 50);
///
/// // Reject from the left rule (below the lower bound).
/// let err_left = Refined::<i32, InRange>::try_new(-1).unwrap_err();
/// assert_eq!(err_left, NumericError::OutOfRange { value: -1 });
///
/// // Reject from the right rule (above the upper bound).
/// let err_right = Refined::<i32, InRange>::try_new(101).unwrap_err();
/// assert_eq!(err_right, NumericError::OutOfRange { value: 101 });
/// ```
pub struct And<A, B>(PhantomData<(A, B)>);

/// Either rule may accept. `A::refine` runs first; on `Ok` its
/// output is the result, on `Err` `B::refine` is tried against the
/// original input.
///
/// Both rules' `Error` types must unify. When both rules reject, the
/// composition's `Self::Error` is `[E; 2]` — the two errors are
/// preserved positionally (`[left, right]`) so callers can inspect
/// either rejection without the composition tree leaking into the
/// public surface.
///
/// # Examples
///
/// ```
/// use whittle_core::{Or, Refined};
/// use whittle_core::primitive::{AtLeast, AtMost, NumericError};
///
/// // `value <= 10 OR value >= 100`. Both alternatives produce
/// // `NumericError`, so the composition's error is
/// // `[NumericError; 2]`.
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
/// // Reject: neither alternative accepts 50. Both errors are
/// // returned in order.
/// let err = Refined::<i32, Either>::try_new(50).unwrap_err();
/// assert_eq!(err[0], NumericError::OutOfRange { value: 50 });
/// assert_eq!(err[1], NumericError::OutOfRange { value: 50 });
/// ```
pub struct Or<A, B>(PhantomData<(A, B)>);

impl<T, E, A, B> Rule<T> for And<A, B>
where
    T: 'static,
    E: 'static,
    A: Rule<T, Error = E>,
    B: Rule<T, Error = E>,
{
    type Error = E;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let raw = A::refine(raw)?;
        B::refine(raw)
    }
}

impl<T, E, A, B> Rule<T> for Or<A, B>
where
    T: 'static + Clone,
    E: 'static,
    A: Rule<T, Error = E>,
    B: Rule<T, Error = E>,
{
    type Error = [E; 2];

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        // Clone first so the second attempt can run against the
        // original input if `A` rejects. This is the only place in
        // the kernel that requires `T: Clone`.
        let snapshot = raw.clone();
        match A::refine(raw) {
            Ok(value) => Ok(value),
            Err(left) => match B::refine(snapshot) {
                Ok(value) => Ok(value),
                Err(right) => Err([left, right]),
            },
        }
    }
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// `And<A, B>` drives `A`'s strategy as the generator and filters
// candidates against `B::refine`. The user picks `A` as the
// generator-shaped rule (`Within`, `LenChars`, etc.); `B` may be a
// further predicate (e.g. `EachChar<...>`). If `B` rejects most of
// `A`'s output, swap the operands so the generator-shaped rule is
// on the left.
//
// `Or<A, B>` is the union of admissible regions; `prop_oneof!`
// picks uniformly between the two sub-strategies.

#[cfg(feature = "proptest")]
impl<T, E, A, B> ArbitraryRule<T> for And<A, B>
where
    T: core::fmt::Debug + 'static,
    E: 'static,
    A: ArbitraryRule<T> + Rule<T, Error = E>,
    B: Rule<T, Error = E>,
{
    type Strategy = proptest::strategy::FilterMap<A::Strategy, fn(T) -> Option<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        A::arbitrary_strategy()
            .prop_filter_map("And: right rule rejected", |raw| B::refine(raw).ok())
    }
}

#[cfg(feature = "proptest")]
impl<T, E, A, B> ArbitraryRule<T> for Or<A, B>
where
    T: core::fmt::Debug + Clone + 'static,
    E: 'static,
    A: ArbitraryRule<T> + Rule<T, Error = E>,
    B: ArbitraryRule<T> + Rule<T, Error = E>,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::prop_oneof![A::arbitrary_strategy(), B::arbitrary_strategy()].boxed()
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

    use super::{And, Or};
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
    fn and_reports_left_failure_with_shared_error() {
        // `AtLeast<0>` rejects first; its `NumericError` surfaces
        // directly because both rules share the same error type.
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _> =
            Refined::try_new(-1_i32);
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: -1 },);
    }

    #[test]
    fn and_reports_right_failure_with_shared_error() {
        // `AtLeast<0>` accepts; `AtMost<100>` then rejects, and its
        // `NumericError` surfaces directly.
        let result: Result<Refined<i32, And<AtLeast<0>, AtMost<100>>>, _> =
            Refined::try_new(101_i32);
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: 101 },);
    }

    #[test]
    fn and_combines_string_primitives() {
        // 1..=10 char identifier-body string — the shape an
        // `AttributeName` would use. Both rules produce
        // `StringError`, so the composition's error is
        // `StringError`.
        type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
        let ok: Refined<String, Ident> = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "user_42");

        let bad_len: Result<Refined<String, Ident>, _> = Refined::try_new(String::new());
        assert_eq!(
            bad_len.unwrap_err(),
            StringError::CharCountOutOfRange { actual: 0 },
        );

        let bad_char: Result<Refined<String, Ident>, _> = Refined::try_new("user-42".to_string());
        assert_eq!(bad_char.unwrap_err(), StringError::BadChar { offset: 4 },);
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
    fn or_reports_both_failures_as_array() {
        // Both rules reject; the composition returns `[E; 2]` with
        // the left error first, the right error second.
        type Either = Or<AtMost<10>, AtLeast<100>>;
        let result: Result<Refined<i32, Either>, _> = Refined::try_new(50_i32);
        let err: [NumericError; 2] = result.unwrap_err();
        assert_eq!(err[0], NumericError::OutOfRange { value: 50 });
        assert_eq!(err[1], NumericError::OutOfRange { value: 50 });
    }

    #[test]
    fn refinement_macro_bounded_admits_and_rejects_and() {
        // Macro-generated `And` newtype: admit a mid-range value,
        // reject above MAX. The shared error type means the macro's
        // forwarded error is the rules' flat enum directly.
        let ok = Bounded100::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let owned: i32 = ok.into_inner();
        assert_eq!(owned, 50_i32);
        let bad = Bounded100::try_new(101_i32).unwrap_err();
        assert_eq!(bad, NumericError::OutOfRange { value: 101 });
    }

    #[test]
    fn refinement_macro_out_of_middle_admits_and_rejects_or() {
        // Macro-generated `Or` newtype: admit on either alternative,
        // reject when both alternatives fail. The shared error type
        // means the rejection surfaces as `[NumericError; 2]`.
        let small = OutOfMiddle::try_new(5_i32).unwrap();
        let big = OutOfMiddle::try_new(150_i32).unwrap();
        assert_eq!(*small.as_inner(), 5_i32);
        assert_eq!(*big.as_inner(), 150_i32);
        let owned: i32 = big.into_inner();
        assert_eq!(owned, 150_i32);
        let bad: [NumericError; 2] = OutOfMiddle::try_new(50_i32).unwrap_err();
        assert_eq!(bad[0], NumericError::OutOfRange { value: 50 });
        assert_eq!(bad[1], NumericError::OutOfRange { value: 50 });
    }

    #[test]
    fn nested_and_for_three_rules() {
        // Compose three rules through binary nesting. All three
        // rules produce `NumericError`, so the composition's error
        // is `NumericError` directly.
        type Triple = And<NonZero, And<AtLeast<-10>, AtMost<10>>>;
        let ok: Refined<i32, Triple> = Refined::try_new(5_i32).unwrap();
        assert_eq!(*ok.as_inner(), 5_i32);
        let bad: Result<Refined<i32, Triple>, _> = Refined::try_new(0_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 0 },);
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
            //
            // kernel-only: domain code wraps this composition in a
            // newtype with a flat error enum — see SKILL.md
            // "Newtype hiding rule composition".
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
