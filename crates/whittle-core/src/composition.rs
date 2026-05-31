//! Rule composition: binary `And<A, B>` / `Or<A, B>`, unary
//! `Not<R>`, binary `Xor<A, B>`, and n-ary `All<(...)>` / `Any<(...)>`.
//!
//! `And` / `Or` / `All` / `Any` are generic over any carrier whose
//! operands share the same `Rule::Error` type. The composition's
//! `Self::Error` is that shared type (or `[E; N]` for the
//! disjunctions on full rejection) — no positional `Left` / `Right`
//! wrapping is exposed to callers. Domain newtypes pattern-match
//! on the rules' flat error enum directly.
//!
//! `Not<R>` and `Xor<A, B>` are restricted to numeric carriers
//! (`T: Numeric + Copy`, inner rules sharing
//! `Rule::Error = NumericError`) because their rejection paths need
//! to fabricate an error variant — `Not` when the inner rule
//! unexpectedly accepts, `Xor` when both inner rules accept. Both
//! reuse `NumericError::OutOfRange { value }`. Other carrier
//! families can add their own impls under the same constraint.
//!
//! The n-ary `All` / `Any` operators are tuple-based:
//! `All<(R1, R2, R3)>` runs three rules sequentially;
//! `Any<(R1, R2, R3)>` returns the first acceptance or `[E; 3]` on
//! full rejection. Arities 2..=8 are supported. For arity 2 they
//! reduce to the same shape as `And` / `Or`.

use core::marker::PhantomData;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;
use crate::transform::{StableUnderAsciiLowercase, StableUnderAsciiUppercase, StableUnderTrim};

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

/// Rejects values that the inner rule `R` admits, and admits values
/// that `R` rejects.
///
/// The current impl is restricted to numeric carriers — `T:
/// Numeric + Copy` and `R: Rule<T, Error = NumericError>` — because
/// rejection mapping needs a variant in `NumericError` to express
/// "the inner rule unexpectedly accepted." Reuses
/// `NumericError::OutOfRange { value }` for that case.
///
/// `type NotEqualTo<const N: i128> = Not<EqualTo<N>>` is the
/// motivating example; `type NonZero = NotEqualTo<0>` chains
/// through it.
///
/// # Examples
///
/// ```
/// use whittle_core::primitive::{EqualTo, NumericError};
/// use whittle_core::{Not, Refined};
///
/// // Admit any value that is NOT 42.
/// let ok: Refined<i32, Not<EqualTo<42>>> = Refined::try_new(7).unwrap();
/// assert_eq!(*ok.as_inner(), 7);
///
/// // Reject the one value that `EqualTo<42>` admits.
/// let err = Refined::<i32, Not<EqualTo<42>>>::try_new(42).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 42 });
/// ```
pub struct Not<R>(PhantomData<fn() -> R>);

/// Exactly one of `A`, `B` must accept; both-accept and
/// both-reject both fail.
///
/// The truth table:
///
/// | `A` | `B` | `Xor<A, B>` |
/// | --- | --- | --- |
/// | accept | reject | accept |
/// | reject | accept | accept |
/// | accept | accept | reject (both matched) |
/// | reject | reject | reject (neither matched) |
///
/// Same carrier and error-type constraints as `Not<R>`: numeric
/// only, both operands sharing `Rule::Error = NumericError`, the
/// rejection paths reuse `NumericError::OutOfRange { value }`.
///
/// # Examples
///
/// ```
/// use whittle_core::primitive::{AtLeast, AtMost, NumericError};
/// use whittle_core::{Refined, Xor};
///
/// // Exactly one bound must hold: outside [0, 10] (so `< 0` xor
/// // `> 10`, where `AtLeast<0>` is `>= 0` and `AtMost<10>` is
/// // `<= 10`, which means inside [0, 10] both accept and Xor
/// // rejects, and outside [0, 10] exactly one accepts).
/// type Outside = Xor<AtLeast<0>, AtMost<10>>;
///
/// // Admit: `-5` only `AtMost<10>` accepts.
/// let r: Refined<i32, Outside> = Refined::try_new(-5).unwrap();
/// assert_eq!(*r.as_inner(), -5);
///
/// // Admit: `100` only `AtLeast<0>` accepts.
/// let r: Refined<i32, Outside> = Refined::try_new(100).unwrap();
/// assert_eq!(*r.as_inner(), 100);
///
/// // Reject: `5` is in `[0, 10]` so both accept.
/// let err = Refined::<i32, Outside>::try_new(5).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 5 });
/// ```
pub struct Xor<A, B>(PhantomData<(A, B)>);

/// Every operand in the tuple must accept. The n-ary generalisation
/// of [`And`] — `All<(A, B, C)>` is equivalent to
/// `And<A, And<B, C>>` but without the nesting.
///
/// Operands run sequentially, each receiving the previous one's
/// (possibly canonicalised) output. All operands must share
/// `Rule::Error = E`; the composition's `Self::Error` is that
/// shared type. Supported arities: 2..=8.
///
/// # Examples
///
/// ```
/// use whittle_core::primitive::{AtLeast, AtMost, NonZero, NumericError};
/// use whittle_core::{All, Refined};
///
/// // 1..=100, non-zero — three rules composed flat.
/// type SmallNonZero = All<(AtLeast<1>, AtMost<100>, NonZero)>;
///
/// let r: Refined<i32, SmallNonZero> = Refined::try_new(42).unwrap();
/// assert_eq!(*r.as_inner(), 42);
///
/// let err = Refined::<i32, SmallNonZero>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub struct All<TUPLE>(PhantomData<fn() -> TUPLE>);

/// Any operand in the tuple may accept. The n-ary generalisation of
/// [`Or`] — `Any<(A, B, C)>` is equivalent to `Or<A, Or<B, C>>` but
/// without the nested error type.
///
/// Operands are tried in order against a clone of the input; the
/// first to accept wins. If all reject, `Self::Error = [E; N]`
/// collects each operand's rejection (left to right). Requires
/// `T: Clone`. Supported arities: 2..=8.
///
/// # Examples
///
/// ```
/// use whittle_core::primitive::{EqualTo, NumericError};
/// use whittle_core::{Any, Refined};
///
/// // Includes-like: admit only one of these three values.
/// type AllowedRoll = Any<(EqualTo<1>, EqualTo<3>, EqualTo<6>)>;
///
/// let r: Refined<i32, AllowedRoll> = Refined::try_new(3).unwrap();
/// assert_eq!(*r.as_inner(), 3);
///
/// let err = Refined::<i32, AllowedRoll>::try_new(4).unwrap_err();
/// // Three rejections, one per operand.
/// assert_eq!(err.len(), 3);
/// assert_eq!(err[0], NumericError::OutOfRange { value: 4 });
/// ```
pub struct Any<TUPLE>(PhantomData<fn() -> TUPLE>);

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

impl<T, R> Rule<T> for Not<R>
where
    T: crate::primitive::Numeric + Copy,
    R: Rule<T, Error = crate::primitive::NumericError>,
{
    type Error = crate::primitive::NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        // `T: Copy` lets us recover `raw` after `R::refine`
        // consumes it. The widened i128 form of the offending
        // value is what `NumericError::OutOfRange` carries.
        let widened = raw.into_i128();
        if R::refine(raw).is_ok() {
            Err(crate::primitive::NumericError::OutOfRange { value: widened })
        } else {
            T::from_i128(widened)
        }
    }
}

impl<T, A, B> Rule<T> for Xor<A, B>
where
    T: crate::primitive::Numeric + Copy,
    A: Rule<T, Error = crate::primitive::NumericError>,
    B: Rule<T, Error = crate::primitive::NumericError>,
{
    type Error = crate::primitive::NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        // `T: Copy` lets both rules run against the same input
        // without cloning. The widened i128 form of the offending
        // value is what `NumericError::OutOfRange` carries when
        // either both accept or both reject.
        let widened = raw.into_i128();
        let a_ok = A::refine(raw).is_ok();
        let b_ok = B::refine(raw).is_ok();
        if a_ok ^ b_ok {
            T::from_i128(widened)
        } else {
            Err(crate::primitive::NumericError::OutOfRange { value: widened })
        }
    }
}

// ─── N-ary `All` / `Any` impls. ────────────────────────────────────
//
// Each arity gets its own `impl Rule<T>` block. The macro
// generates the body so the same `?`-chaining (`All`) /
// short-circuit-collect (`Any`) shape applies across every
// supported arity. Supported arities: 2..=8.

macro_rules! impl_all_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        impl<T, E, $($Ri),+> Rule<T> for All<($($Ri,)+)>
        where
            T: 'static,
            E: 'static,
            $($Ri: Rule<T, Error = E>,)+
        {
            type Error = E;

            #[inline]
            fn refine(raw: T) -> Result<T, Self::Error> {
                // Each operand receives the previous one's output;
                // `?` short-circuits on the first rejection.
                $(let raw = $Ri::refine(raw)?;)+
                Ok(raw)
            }
        }
    };
}

impl_all_for_arity!(R1, R2);
impl_all_for_arity!(R1, R2, R3);
impl_all_for_arity!(R1, R2, R3, R4);
impl_all_for_arity!(R1, R2, R3, R4, R5);
impl_all_for_arity!(R1, R2, R3, R4, R5, R6);
impl_all_for_arity!(R1, R2, R3, R4, R5, R6, R7);
impl_all_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

macro_rules! impl_any_for_arity {
    ($N:literal; $($Ri:ident),+ $(,)?) => {
        impl<T, E, $($Ri),+> Rule<T> for Any<($($Ri,)+)>
        where
            T: 'static + Clone,
            E: 'static,
            $($Ri: Rule<T, Error = E>,)+
        {
            type Error = [E; $N];

            #[inline]
            fn refine(raw: T) -> Result<T, Self::Error> {
                // Try each operand against a clone of the input;
                // the first acceptance wins. Collect each
                // rejection so the caller gets the full failure
                // story when none accept.
                let mut errors: ::alloc::vec::Vec<E> =
                    ::alloc::vec::Vec::with_capacity($N);
                $(
                    match $Ri::refine(raw.clone()) {
                        Ok(value) => return Ok(value),
                        Err(err) => errors.push(err),
                    }
                )+
                // `errors` contains exactly `$N` items by
                // construction; the `try_into` cannot fail.
                let arr: [E; $N] = match errors.try_into() {
                    Ok(arr) => arr,
                    Err(_) => unreachable!("any: collected exactly N rejections"),
                };
                Err(arr)
            }
        }
    };
}

impl_any_for_arity!(2; R1, R2);
impl_any_for_arity!(3; R1, R2, R3);
impl_any_for_arity!(4; R1, R2, R3, R4);
impl_any_for_arity!(5; R1, R2, R3, R4, R5);
impl_any_for_arity!(6; R1, R2, R3, R4, R5, R6);
impl_any_for_arity!(7; R1, R2, R3, R4, R5, R6, R7);
impl_any_for_arity!(8; R1, R2, R3, R4, R5, R6, R7, R8);

// ─── Transformer stability. If both operands are stable under a
//      transformation, the composition's admissible region is the
//      intersection / union of regions that are each stable, so the
//      composition is stable too. ──────────────────────────────────

impl<A, B> StableUnderTrim for And<A, B>
where
    A: StableUnderTrim,
    B: StableUnderTrim,
{
}

impl<A, B> StableUnderTrim for Or<A, B>
where
    A: StableUnderTrim,
    B: StableUnderTrim,
{
}

impl<A, B> StableUnderAsciiLowercase for And<A, B>
where
    A: StableUnderAsciiLowercase,
    B: StableUnderAsciiLowercase,
{
}

impl<A, B> StableUnderAsciiLowercase for Or<A, B>
where
    A: StableUnderAsciiLowercase,
    B: StableUnderAsciiLowercase,
{
}

impl<A, B> StableUnderAsciiUppercase for And<A, B>
where
    A: StableUnderAsciiUppercase,
    B: StableUnderAsciiUppercase,
{
}

impl<A, B> StableUnderAsciiUppercase for Or<A, B>
where
    A: StableUnderAsciiUppercase,
    B: StableUnderAsciiUppercase,
{
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
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        A::arbitrary_strategy()
            .prop_filter_map("And: right rule rejected", |raw| B::refine(raw).ok())
            .boxed()
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

#[cfg(feature = "proptest")]
impl<T, R> ArbitraryRule<T> for Not<R>
where
    T: crate::primitive::ArbitraryNumeric + core::fmt::Debug,
    R: Rule<T, Error = crate::primitive::NumericError>,
{
    // `Not<R>` admits every value `R` rejects. For `R = EqualTo<N>`
    // the admissible region is dense (one excluded value out of
    // ~2^N), so a `prop_filter` over the full numeric range is
    // cheap. Other inner rules with sparser admissible regions
    // make this strategy correspondingly noisier; in that case
    // wrap a narrower-domain rule on the left of an `And`.
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        T::arbitrary_in_range(i128::MIN, i128::MAX)
            .prop_filter("Not: inner rule unexpectedly accepted", |v| {
                R::refine(*v).is_err()
            })
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, A, B> ArbitraryRule<T> for Xor<A, B>
where
    T: crate::primitive::ArbitraryNumeric + core::fmt::Debug,
    A: ArbitraryRule<T> + Rule<T, Error = crate::primitive::NumericError>,
    B: ArbitraryRule<T> + Rule<T, Error = crate::primitive::NumericError>,
{
    // Strategy: union of `A` and `B`'s strategies, filtered to the
    // symmetric difference. When `A` and `B`'s admissible regions
    // are nearly disjoint the filter is cheap; when they overlap
    // heavily the filter rate climbs. Documented tradeoff.
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::prop_oneof![A::arbitrary_strategy(), B::arbitrary_strategy()]
            .prop_filter("Xor: exactly one operand must accept", |v| {
                A::refine(*v).is_ok() ^ B::refine(*v).is_ok()
            })
            .boxed()
    }
}

// ─── N-ary `All` / `Any` `ArbitraryRule` impls. ───────────────────
//
// `All<(R1, R2, ..., RN)>` drives the leftmost rule's strategy and
// filters each candidate through the remaining rules in order, the
// same shape as `And<A, B>`'s strategy with one more level for each
// extra operand. `Any<(R1, R2, ..., RN)>` is `prop_oneof!` over
// each operand's strategy.

#[cfg(feature = "proptest")]
macro_rules! impl_all_arbitrary_for_arity {
    ($First:ident $(, $Rest:ident)+ $(,)?) => {
        impl<T, E, $First, $($Rest),+> ArbitraryRule<T>
            for All<($First, $($Rest,)+)>
        where
            T: core::fmt::Debug + 'static,
            E: 'static,
            $First: ArbitraryRule<T> + Rule<T, Error = E>,
            $($Rest: Rule<T, Error = E>,)+
        {
            type Strategy = proptest::strategy::BoxedStrategy<T>;

            #[inline]
            fn arbitrary_strategy() -> Self::Strategy {
                use proptest::strategy::Strategy as _;
                $First::arbitrary_strategy()
                    $(.prop_filter_map(
                        concat!(
                            "All: operand ",
                            stringify!($Rest),
                            " rejected",
                        ),
                        |raw| $Rest::refine(raw).ok(),
                    ))+
                    .boxed()
            }
        }
    };
}

#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3, R4);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3, R4, R5);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6, R7);
#[cfg(feature = "proptest")]
impl_all_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

#[cfg(feature = "proptest")]
macro_rules! impl_any_arbitrary_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        impl<T, E, $($Ri),+> ArbitraryRule<T> for Any<($($Ri,)+)>
        where
            T: core::fmt::Debug + Clone + 'static,
            E: 'static,
            $($Ri: ArbitraryRule<T> + Rule<T, Error = E>,)+
        {
            type Strategy = proptest::strategy::BoxedStrategy<T>;

            #[inline]
            fn arbitrary_strategy() -> Self::Strategy {
                use proptest::strategy::Strategy as _;
                proptest::prop_oneof![
                    $($Ri::arbitrary_strategy()),+
                ].boxed()
            }
        }
    };
}

#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3, R4);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3, R4, R5);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6, R7);
#[cfg(feature = "proptest")]
impl_any_arbitrary_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

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
        //     `Refined<T, R>` Arbitrary impl maps the rule's
        //     strategy through `try_new` and panics on contract
        //     violation; it does not retry. `And<A, B>`'s strategy
        //     applies a bounded `prop_filter_map` over `A`'s output
        //     (see the impl below). Every value emitted here is
        //     admissible by construction.

        #[test]
        fn arbitrary_and_admits_only_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, And<crate::primitive::Within<0, 100>, crate::primitive::AtLeast<50>>>,
            >()
        ) {
            // `And<A, B>`'s `ArbitraryRule` impl uses `A`'s
            // strategy filtered through `B::refine`. Pick `A` to
            // be the narrowing generator (`Within<0, 100>` emits
            // values in `[0, 100]`); `B` (`AtLeast<50>`) trims to
            // the upper half. The admissible region is dense
            // enough — 51 values out of 101 — that filtering does
            // not exhaust the retry budget. For broader
            // `A`-strategies (`AtLeast<0>` over `i32` is one), the
            // intersection may be too sparse; pick the narrowing
            // rule as `A`, or use the nominal newtype that already
            // composes them.
            proptest::prop_assert!((50..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_or_admits_only_outside_middle(
            r in proptest::arbitrary::any::<
                Refined<i32, Or<AtMost<0>, AtLeast<100>>>,
            >()
        ) {
            // `Or<A, B>`'s `ArbitraryRule` impl is `prop_oneof!`
            // over `A`'s and `B`'s strategies; every emitted value
            // is admissible under at least one alternative by
            // construction.
            let value = *r.as_inner();
            proptest::prop_assert!(value <= 0 || value >= 100);
        }
    }
}
