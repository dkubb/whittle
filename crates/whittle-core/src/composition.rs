//! Rule composition: binary `And<A, B>` / `Or<A, B>`, unary
//! `Not<R>`, binary `Xor<A, B>`, error-mapping `MapErr<R, M>`,
//! and n-ary `All<(...)>` / `Any<(...)>`.
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

use crate::primitive::collection::StableUnderElementMap;
#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::{PureFilter, Rule};
use crate::schema::{Schema, SchemaInterval, SchemaRule, integer_interval_from_bounds};
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
///
/// Composition schemas are intentionally absent when an operand
/// canonicalises, because the `SchemaRule` set algebra would describe
/// the accepted preimage instead of the carried set:
///
/// ```compile_fail
/// use whittle_core::{And, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<And<LenChars<3, 3>, Trim<NonEmpty>>>();
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
///
/// `Or` schemas have the same purity gate as [`And`]:
///
/// ```compile_fail
/// use whittle_core::{Or, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Or<LenChars<3, 3>, Trim<NonEmpty>>>();
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

/// Zero-sized mapper from one rule error type into another.
///
/// `MapErr<R, M>` uses this trait to shrink or reshape a rule's
/// public error codomain without changing the rule's admissible
/// value set.
pub trait ErrorMapper<E>: 'static {
    /// The mapped error type exposed by `MapErr`.
    type Error;

    /// Convert an inner rule error into the mapped error.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::ErrorMapper;
    /// use whittle_core::primitive::StringError;
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// enum CodeError {
    ///     BadLength,
    ///     BadInput,
    /// }
    ///
    /// enum CodeErrorMapper {}
    ///
    /// impl ErrorMapper<StringError> for CodeErrorMapper {
    ///     type Error = CodeError;
    ///
    ///     fn map_error(error: StringError) -> Self::Error {
    ///         match error {
    ///             StringError::CharCountOutOfRange { .. } => CodeError::BadLength,
    ///             _ => CodeError::BadInput,
    ///         }
    ///     }
    /// }
    ///
    /// let value = CodeErrorMapper::map_error(
    ///     StringError::CharCountOutOfRange { actual: 2 },
    /// );
    ///
    /// assert_eq!(value, CodeError::BadLength);
    /// ```
    fn map_error(error: E) -> Self::Error;
}

/// Preserve a rule's accepted values while mapping its rejection
/// error through `M`.
///
/// This is useful when a composed primitive rule reuses a broad
/// domain error enum but a domain type wants to expose only the
/// rejection cases reachable through that composition.
///
/// # Examples
///
/// ```
/// use whittle_core::{And, ErrorMapper, MapErr, Refined};
/// use whittle_core::primitive::{
///     AsciiAlphanumeric, EachChar, LenChars, StringError,
/// };
///
/// #[derive(Debug, PartialEq, Eq)]
/// enum CodeError {
///     BadLength,
///     BadCharacter,
/// }
///
/// enum CodeErrorMapper {}
/// impl ErrorMapper<StringError> for CodeErrorMapper {
///     type Error = CodeError;
///
///     fn map_error(error: StringError) -> Self::Error {
///         match error {
///             StringError::CharCountOutOfRange { .. } => CodeError::BadLength,
///             StringError::BadChar { .. } => CodeError::BadCharacter,
///             StringError::ByteLenOutOfRange { .. }
///             | StringError::Empty
///             | StringError::BadFirstChar
///             | StringError::BadHexLength { .. } => {
///                 unreachable!("Code rule cannot produce this StringError")
///             }
///         }
///     }
/// }
///
/// type CodeRule = MapErr<
///     And<LenChars<3, 3>, EachChar<AsciiAlphanumeric>>,
///     CodeErrorMapper,
/// >;
///
/// let err = Refined::<String, CodeRule>::try_new("ab".to_string())
///     .unwrap_err();
/// assert_eq!(err, CodeError::BadLength);
/// ```
pub struct MapErr<R, M>(PhantomData<(R, M)>);

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
///
/// The n-ary schema impl keeps the `PureFilter` bound at every
/// supported arity:
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, NonEmpty, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, LenBytes<1, 256>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, LenBytes<1, 256>, NonEmpty, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{All, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<All<(
///     LenChars<0, 64>,
///     NonEmpty,
///     LenBytes<0, 128>,
///     LenChars<1, 128>,
///     LenBytes<1, 256>,
///     NonEmpty,
///     LenChars<2, 256>,
///     Trim<NonEmpty>,
/// )>>();
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
///
/// The n-ary schema impl keeps the `PureFilter` bound at every
/// supported arity:
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, NonEmpty, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, LenBytes<1, 256>, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(LenChars<0, 64>, NonEmpty, LenBytes<0, 128>, LenChars<1, 128>, LenBytes<1, 256>, NonEmpty, Trim<NonEmpty>)>>();
/// ```
///
/// ```compile_fail
/// use whittle_core::{Any, SchemaRule};
/// use whittle_core::primitive::{LenBytes, LenChars, NonEmpty};
/// use whittle_core::transform::Trim;
///
/// fn assert_schema<R: SchemaRule<String>>() {}
/// assert_schema::<Any<(
///     LenChars<0, 64>,
///     NonEmpty,
///     LenBytes<0, 128>,
///     LenChars<1, 128>,
///     LenBytes<1, 256>,
///     NonEmpty,
///     LenChars<2, 256>,
///     Trim<NonEmpty>,
/// )>>();
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

impl<T, R, M> Rule<T> for MapErr<R, M>
where
    T: 'static,
    R: Rule<T>,
    M: ErrorMapper<R::Error>,
{
    type Error = M::Error;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        R::refine(raw).map_err(M::map_error)
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
    ($N:literal; $($Ri:ident => $ei:ident),+ $(,)?) => {
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
                // the first acceptance returns early. Each rejection
                // binds its error to a per-operand local, and the
                // array is assembled directly from those bindings —
                // no fallible `Vec::try_into`, so the `[E; $N]`
                // length is guaranteed by construction with no
                // unreachable branch.
                $(
                    let $ei: E = match $Ri::refine(raw.clone()) {
                        Ok(value) => return Ok(value),
                        Err(err) => err,
                    };
                )+
                Err([$($ei),+])
            }
        }
    };
}

impl_any_for_arity!(2; R1 => e1, R2 => e2);
impl_any_for_arity!(3; R1 => e1, R2 => e2, R3 => e3);
impl_any_for_arity!(4; R1 => e1, R2 => e2, R3 => e3, R4 => e4);
impl_any_for_arity!(5; R1 => e1, R2 => e2, R3 => e3, R4 => e4, R5 => e5);
impl_any_for_arity!(6; R1 => e1, R2 => e2, R3 => e3, R4 => e4, R5 => e5, R6 => e6);
impl_any_for_arity!(7; R1 => e1, R2 => e2, R3 => e3, R4 => e4, R5 => e5, R6 => e6, R7 => e7);
impl_any_for_arity!(
    8; R1 => e1, R2 => e2, R3 => e3, R4 => e4, R5 => e5, R6 => e6, R7 => e7, R8 => e8
);

// ─── Serde `DeserializeRule` impls. ───────────────────────────────
//
// `And<A, B>` and `All<(R1, ...)>` forward to their FIRST operand's
// hook and then run the remaining operands' `refine` on the decoded
// value. A size-bounding first operand (`LenItems` on the left of
// the conjunction) therefore streams its bound for the whole
// composition, while operand order and error semantics stay exactly
// `refine`'s: first operand's rejection first, then each remaining
// operand's, all rendered through `serde::de::Error::custom`.
//
// `Or` / `Any` cannot forward: when the first operand rejects, the
// alternatives must re-run against the ORIGINAL wire input, and a
// `Deserializer` is consumed by the first attempt — retrying would
// require buffering the raw input, which is the allocation this
// feature exists to avoid. They take the default parse-then-refine
// path. `Not` / `Xor` reject based on an operand *accepting*, so
// there is no per-operand streaming to forward either; default path.
//
// `MapErr<R, M>` deliberately takes the default path instead of
// forwarding to `R`'s hook: forwarding would surface `R`'s raw
// error and silently bypass `M`'s mapping, and the mapped
// diagnostics are load-bearing for domain types.

#[cfg(feature = "serde")]
impl<'de, T, E, A, B> crate::DeserializeRule<'de, T> for And<A, B>
where
    T: 'static,
    E: 'static + core::fmt::Display,
    A: crate::DeserializeRule<'de, T> + Rule<T, Error = E>,
    B: Rule<T, Error = E>,
{
    fn deserialize_refined<D>(deserializer: D) -> Result<crate::Refined<T, Self>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let left = A::deserialize_refined(deserializer)?;
        let value = B::refine(left.into_inner()).map_err(serde::de::Error::custom)?;
        // SOUNDNESS (IDEA §5.1): `And::refine` is `B(A(x))`. `A`'s
        // hook produced a value admissible under `A` (its own
        // soundness obligation), and `B::refine` just accepted and
        // possibly canonicalised it — so `value` is exactly what
        // `And::refine` would have returned for the same raw input,
        // and re-running it through `try_new` would re-evaluate the
        // same predicates on an admissible value (idempotency
        // obligation, IDEA §5.14).
        Ok(crate::Refined::from_inner(value))
    }
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, E, A, B] DeserializeRule<T> for Or<A, B>
    where [T: 'static + Clone, E: 'static, A: Rule<T, Error = E>, B: Rule<T, Error = E>]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, R] DeserializeRule<T> for Not<R>
    where [
        T: crate::primitive::Numeric + Copy,
        R: Rule<T, Error = crate::primitive::NumericError>,
    ]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, A, B] DeserializeRule<T> for Xor<A, B>
    where [
        T: crate::primitive::Numeric + Copy,
        A: Rule<T, Error = crate::primitive::NumericError>,
        B: Rule<T, Error = crate::primitive::NumericError>,
    ]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, R, M] DeserializeRule<T> for MapErr<R, M>
    where [T: 'static, R: Rule<T>, M: ErrorMapper<R::Error>]
}

#[cfg(feature = "serde")]
macro_rules! impl_all_deserialize_for_arity {
    ($First:ident $(, $Rest:ident)+ $(,)?) => {
        impl<'de, T, E, $First, $($Rest),+> crate::DeserializeRule<'de, T>
            for All<($First, $($Rest,)+)>
        where
            T: 'static,
            E: 'static + core::fmt::Display,
            $First: crate::DeserializeRule<'de, T> + Rule<T, Error = E>,
            $($Rest: Rule<T, Error = E>,)+
        {
            fn deserialize_refined<D>(
                deserializer: D,
            ) -> ::core::result::Result<crate::Refined<T, Self>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let raw = $First::deserialize_refined(deserializer)?.into_inner();
                $(let raw = $Rest::refine(raw).map_err(serde::de::Error::custom)?;)+
                // SOUNDNESS (IDEA §5.1): same argument as
                // `And<A, B>`'s hook — the first operand's hook
                // discharged its own admissibility obligation and
                // every remaining operand's `refine` ran in
                // `All::refine`'s exact order on the previous
                // operand's output, so `raw` equals what
                // `All::refine` would have returned for the same
                // wire input.
                Ok(crate::Refined::from_inner(raw))
            }
        }
    };
}

#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3, R4);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3, R4, R5);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3, R4, R5, R6);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3, R4, R5, R6, R7);
#[cfg(feature = "serde")]
impl_all_deserialize_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

#[cfg(feature = "serde")]
macro_rules! impl_any_deserialize_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        crate::deserialize_rule! {
            impl[T, E, $($Ri),+] DeserializeRule<T> for Any<($($Ri,)+)>
            where [T: 'static + Clone, E: 'static, $($Ri: Rule<T, Error = E>,)+]
        }
    };
}

#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3, R4);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3, R4, R5);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3, R4, R5, R6);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3, R4, R5, R6, R7);
#[cfg(feature = "serde")]
impl_any_deserialize_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

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

impl<A, B> StableUnderElementMap for And<A, B>
where
    A: StableUnderElementMap,
    B: StableUnderElementMap,
{
}

impl<A, B> StableUnderElementMap for Or<A, B>
where
    A: StableUnderElementMap,
    B: StableUnderElementMap,
{
}

// ─── `SchemaRule` impls. ──────────────────────────────────────────
//
// SOUNDNESS (⟦schema()⟧ = range(refine), the trait contract), with
// every operand bounded `PureFilter` so its accepted set EQUALS its
// carried set (refine is the identity on admissible input, and by
// each operand's own SchemaRule obligation both equal ⟦operand⟧):
//
// - `And<A, B>::refine = B ∘ A`. For pure operands,
//   range(B ∘ A) = C(A) ∩ C(B) = ⟦A⟧ ∩ ⟦B⟧: every output passed
//   both predicates unchanged, and every member of both sets is
//   returned by the chain as itself. The Intersection constructor
//   fuses what it can (same-kind intervals, Str vocabulary) and
//   keeps the rest symbolic — either way the denotation is the
//   intersection.
// - `Or<A, B>` tries `A`, then `B` against the ORIGINAL input:
//   range = C(A) ∪ C(B) = ⟦A⟧ ∪ ⟦B⟧ — the Union node.
// - `All` / `Any` generalise the same two derivations per arity.
// - `MapErr<R, M>` only maps the rejection error, so its range is
//   `R`'s range for ANY `R` — transparent, no purity bound.
//
// The purity bounds are load-bearing, not convenience: a
// canonicalising operand makes the set algebra WRONG, not merely
// imprecise. `And<LenChars<3, 3>, Trim<NonEmpty>>` accepts the raw
// input "a  " (3 chars) and carries "a" — a value in NEITHER
// operand's set claim — while "a  " itself is in both sets but never
// carried. Per the design's absence-over-wrong-impl rule, such
// compositions have NO schema: the `PureFilter` bound is the
// compile-time absence.

impl<T, E, A, B> SchemaRule<T> for And<A, B>
where
    T: 'static,
    E: 'static,
    A: SchemaRule<T> + Rule<T, Error = E> + PureFilter,
    B: SchemaRule<T> + Rule<T, Error = E> + PureFilter,
{
    #[inline]
    fn schema() -> Schema {
        Schema::intersection(alloc::vec![A::schema(), B::schema()])
    }
}

impl<T, E, A, B> SchemaRule<T> for Or<A, B>
where
    T: 'static + Clone,
    E: 'static,
    A: SchemaRule<T> + Rule<T, Error = E> + PureFilter,
    B: SchemaRule<T> + Rule<T, Error = E> + PureFilter,
{
    #[inline]
    fn schema() -> Schema {
        Schema::union(alloc::vec![A::schema(), B::schema()])
    }
}

impl<T, R, M> SchemaRule<T> for MapErr<R, M>
where
    T: 'static,
    R: SchemaRule<T>,
    M: ErrorMapper<R::Error>,
{
    #[inline]
    fn schema() -> Schema {
        R::schema()
    }
}

macro_rules! impl_all_schema_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        impl<T, E, $($Ri),+> SchemaRule<T> for All<($($Ri,)+)>
        where
            T: 'static,
            E: 'static,
            $($Ri: SchemaRule<T> + Rule<T, Error = E> + PureFilter,)+
        {
            #[inline]
            fn schema() -> Schema {
                Schema::intersection(alloc::vec![$($Ri::schema()),+])
            }
        }
    };
}

impl_all_schema_for_arity!(R1, R2);
impl_all_schema_for_arity!(R1, R2, R3);
impl_all_schema_for_arity!(R1, R2, R3, R4);
impl_all_schema_for_arity!(R1, R2, R3, R4, R5);
impl_all_schema_for_arity!(R1, R2, R3, R4, R5, R6);
impl_all_schema_for_arity!(R1, R2, R3, R4, R5, R6, R7);
impl_all_schema_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

macro_rules! impl_any_schema_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        impl<T, E, $($Ri),+> SchemaRule<T> for Any<($($Ri,)+)>
        where
            T: 'static + Clone,
            E: 'static,
            $($Ri: SchemaRule<T> + Rule<T, Error = E> + PureFilter,)+
        {
            #[inline]
            fn schema() -> Schema {
                Schema::union(alloc::vec![$($Ri::schema()),+])
            }
        }
    };
}

impl_any_schema_for_arity!(R1, R2);
impl_any_schema_for_arity!(R1, R2, R3);
impl_any_schema_for_arity!(R1, R2, R3, R4);
impl_any_schema_for_arity!(R1, R2, R3, R4, R5);
impl_any_schema_for_arity!(R1, R2, R3, R4, R5, R6);
impl_any_schema_for_arity!(R1, R2, R3, R4, R5, R6, R7);
impl_any_schema_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

// ─── `Not` / `Xor`: interval complements. ─────────────────────────
//
// Both operate on the `SchemaInterval` bounds vocabulary (a single
// closed integer interval), matching their `Rule` impls' numeric-
// only carriers. SOUNDNESS:
//
// - `Not<R>` admits exactly what `R` rejects and is pure (its accept
//   path returns the input's widened round-trip), so its carried set
//   is the interval's complement — at most two half-bounded
//   intervals, with an empty side dropped at the `i128` extremes.
// - `Xor<A, B>` admits where exactly one operand does:
//   `(A ∖ B) ∪ (B ∖ A)`, computed as interval intersections with the
//   opposite complement's pieces. The two difference sets are
//   disjoint by construction, so the union members never overlap.
//
// Operands whose schemas are not single integer intervals have no
// `SchemaInterval` impl, so these schemas are ABSENT for them — no
// partial `schema()` anywhere.

/// The complement of one closed integer interval, as 0..=2 pieces of
/// bounds; an empty side at an `i128` extreme is dropped (the
/// integer universe has nothing beyond it). Non-generic so every
/// `Not`/`Xor` instantiation shares one function.
fn integer_complement_pieces(
    bounds: (Option<i128>, Option<i128>),
) -> alloc::vec::Vec<(Option<i128>, Option<i128>)> {
    let mut pieces = alloc::vec::Vec::with_capacity(2);
    if let Some(lo) = bounds.0
        && lo > i128::MIN
    {
        pieces.push((None, Some(lo - 1)));
    }
    if let Some(hi) = bounds.1
        && hi < i128::MAX
    {
        pieces.push((Some(hi + 1), None));
    }
    pieces
}

/// Intersect two closed integer intervals given as bounds; `None`
/// when the intersection is empty. Non-generic, shared by every
/// `Xor` instantiation.
fn intersect_integer_bounds(
    a: (Option<i128>, Option<i128>),
    b: (Option<i128>, Option<i128>),
) -> Option<(Option<i128>, Option<i128>)> {
    let lo = match (a.0, b.0) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => Some(x.max(y)),
    };
    let hi = match (a.1, b.1) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => Some(x.min(y)),
    };
    if let (Some(lo), Some(hi)) = (lo, hi)
        && lo > hi
    {
        return None;
    }
    Some((lo, hi))
}

impl<T, R> SchemaRule<T> for Not<R>
where
    T: crate::primitive::Numeric + Copy,
    R: Rule<T, Error = crate::primitive::NumericError> + SchemaInterval<T>,
{
    /// The complement of `R`'s interval: a union of at most two
    /// half-bounded intervals (one when `R`'s interval reaches an
    /// `i128` extreme).
    ///
    /// # Panics
    ///
    /// Panics when `R`'s interval covers the whole integer universe:
    /// its complement admits nothing, and empty admitted sets are
    /// unrepresentable by construction (the same posture as an empty
    /// interval fusion).
    #[inline]
    fn schema() -> Schema {
        let pieces = integer_complement_pieces(R::interval_bounds());
        Schema::union(
            pieces
                .into_iter()
                .map(integer_interval_from_bounds)
                .collect(),
        )
    }
}

impl<T, A, B> SchemaRule<T> for Xor<A, B>
where
    T: crate::primitive::Numeric + Copy,
    A: Rule<T, Error = crate::primitive::NumericError> + SchemaInterval<T>,
    B: Rule<T, Error = crate::primitive::NumericError> + SchemaInterval<T>,
{
    /// The symmetric difference of the operands' intervals,
    /// desugared as `(A ∧ ¬B) ∨ (¬A ∧ B)` over interval pieces.
    ///
    /// # Panics
    ///
    /// Panics when the symmetric difference is empty (the operands'
    /// intervals are equal, so `Xor` admits nothing): empty admitted
    /// sets are unrepresentable by construction.
    #[inline]
    fn schema() -> Schema {
        let a = A::interval_bounds();
        let b = B::interval_bounds();
        let mut members = alloc::vec::Vec::with_capacity(4);
        for (lhs, rhs) in [(a, b), (b, a)] {
            for piece in integer_complement_pieces(rhs) {
                if let Some(fused) = intersect_integer_bounds(lhs, piece) {
                    members.push(integer_interval_from_bounds(fused));
                }
            }
        }
        Schema::union(members)
    }
}

// ─── `PureFilter` propagation. ────────────────────────────────────
//
// SOUNDNESS: `And` / `Or` / `All` / `Any` / `MapErr` only forward
// their operands' `refine` outputs, so they are the identity on
// admissible input exactly when every operand is — the marker
// propagates through operand bounds. `Not` and `Xor` are pure
// UNCONDITIONALLY: their accept path returns
// `T::from_i128(raw.into_i128())`, the lossless widening round-trip
// of the input itself, regardless of what the (rejecting or
// accepting) operands would have produced.

impl<A, B> PureFilter for And<A, B>
where
    A: PureFilter,
    B: PureFilter,
{
}

impl<A, B> PureFilter for Or<A, B>
where
    A: PureFilter,
    B: PureFilter,
{
}

impl<R> PureFilter for Not<R> {}

impl<A, B> PureFilter for Xor<A, B> {}

impl<R, M> PureFilter for MapErr<R, M> where R: PureFilter {}

macro_rules! impl_pure_filter_for_arity {
    ($($Ri:ident),+ $(,)?) => {
        impl<$($Ri),+> PureFilter for All<($($Ri,)+)>
        where
            $($Ri: PureFilter,)+
        {
        }

        impl<$($Ri),+> PureFilter for Any<($($Ri,)+)>
        where
            $($Ri: PureFilter,)+
        {
        }
    };
}

impl_pure_filter_for_arity!(R1, R2);
impl_pure_filter_for_arity!(R1, R2, R3);
impl_pure_filter_for_arity!(R1, R2, R3, R4);
impl_pure_filter_for_arity!(R1, R2, R3, R4, R5);
impl_pure_filter_for_arity!(R1, R2, R3, R4, R5, R6);
impl_pure_filter_for_arity!(R1, R2, R3, R4, R5, R6, R7);
impl_pure_filter_for_arity!(R1, R2, R3, R4, R5, R6, R7, R8);

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// `And<A, B>` samples both operands together and keeps whichever
// candidate also satisfies the opposite rule. This keeps dense
// intersections cheap while giving sparse, asymmetric intersections
// a constructive path from whichever operand already knows how to
// generate the narrower shape (e.g. `EachChar<P>` provides the
// alphabet, while `LenChars<N, N>` provides the exact length).
//
// `Or<A, B>` is the union of admissible regions; `prop_oneof!`
// picks uniformly between the two sub-strategies.

#[cfg(feature = "proptest")]
impl<T, E, A, B> ArbitraryRule<T> for And<A, B>
where
    T: core::fmt::Debug + 'static,
    E: 'static,
    A: ArbitraryRule<T> + Rule<T, Error = E>,
    B: ArbitraryRule<T> + Rule<T, Error = E>,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        (A::arbitrary_strategy(), B::arbitrary_strategy())
            .prop_filter_map(
                "And: no sampled operand satisfied both rules",
                |(left, right)| A::refine(right).or_else(|_| B::refine(left)).ok(),
            )
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

#[cfg(feature = "proptest")]
impl<T, R, M> ArbitraryRule<T> for MapErr<R, M>
where
    T: core::fmt::Debug + 'static,
    R: ArbitraryRule<T>,
    M: ErrorMapper<R::Error>,
{
    type Strategy = R::Strategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        R::arbitrary_strategy()
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

    use super::{All, And, Any, ErrorMapper, MapErr, Or, Xor};
    use crate::primitive::{
        AsciiAlphanumeric, AtLeast, AtMost, EachChar, EqualTo, GreaterThan, IdentChar, LenChars,
        LessThan, NonZero, NumericError, StringError, Within,
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

    #[derive(Debug, PartialEq, Eq)]
    enum CodeError {
        BadLength,
        BadCharacter,
    }

    impl core::fmt::Display for CodeError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match *self {
                Self::BadLength => f.write_str("bad length"),
                Self::BadCharacter => f.write_str("bad character"),
            }
        }
    }

    enum CodeErrorMapper {}
    impl ErrorMapper<StringError> for CodeErrorMapper {
        type Error = CodeError;

        fn map_error(error: StringError) -> Self::Error {
            match error {
                StringError::CharCountOutOfRange { .. } => CodeError::BadLength,
                StringError::BadChar { .. } => CodeError::BadCharacter,
                StringError::ByteLenOutOfRange { .. }
                | StringError::Empty
                | StringError::BadFirstChar
                | StringError::BadHexLength { .. } => {
                    unreachable!("code rule cannot produce this StringError")
                }
            }
        }
    }

    type CodeRule = MapErr<And<LenChars<3, 3>, EachChar<AsciiAlphanumeric>>, CodeErrorMapper>;

    #[derive(Debug, PartialEq, Eq)]
    enum UpperHalfError {
        Outside,
    }

    enum UpperHalfErrorMapper {}
    impl ErrorMapper<NumericError> for UpperHalfErrorMapper {
        type Error = UpperHalfError;

        fn map_error(_error: NumericError) -> Self::Error {
            UpperHalfError::Outside
        }
    }

    type UpperHalf = MapErr<And<Within<0, 100>, AtLeast<50>>, UpperHalfErrorMapper>;

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
    fn map_err_preserves_acceptance_and_maps_left_failure() {
        let ok: Refined<String, CodeRule> = Refined::try_new("A1z".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "A1z");

        let bad_len: Result<Refined<String, CodeRule>, _> = Refined::try_new("A1".to_string());
        assert_eq!(bad_len.unwrap_err(), CodeError::BadLength);
    }

    #[test]
    fn map_err_maps_right_failure() {
        let bad_char: Result<Refined<String, CodeRule>, _> = Refined::try_new("A-z".to_string());
        assert_eq!(bad_char.unwrap_err(), CodeError::BadCharacter);
    }

    #[test]
    #[should_panic(expected = "code rule cannot produce this StringError")]
    fn map_err_mapper_rejects_unreachable_inner_error() {
        CodeErrorMapper::map_error(StringError::Empty);
    }

    #[test]
    fn map_err_maps_numeric_failure() {
        let bad: Result<Refined<i32, UpperHalf>, _> = Refined::try_new(10_i32);
        assert_eq!(bad.unwrap_err(), UpperHalfError::Outside);
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

    // ─── `Xor<A, B>` numeric combinator. ──────────────────────────

    #[test]
    fn xor_admits_when_exactly_one_operand_accepts() {
        // `<= 10` XOR `>= 0`: inside [0, 10] both accept (rejected),
        // outside exactly one accepts (admitted). `-5` is `<= 10`
        // only; `15` is `>= 0` only.
        type Outside = Xor<AtMost<10>, AtLeast<0>>;
        let low: Refined<i32, Outside> = Refined::try_new(-5_i32).unwrap();
        let high: Refined<i32, Outside> = Refined::try_new(15_i32).unwrap();
        assert_eq!(*low.as_inner(), -5_i32);
        assert_eq!(*high.as_inner(), 15_i32);
    }

    #[test]
    fn xor_rejects_when_both_operands_accept() {
        // `5` is inside [0, 10], so both `<= 10` and `>= 0` accept;
        // `Xor` rejects and reports the offending value.
        type Outside = Xor<AtMost<10>, AtLeast<0>>;
        let result: Result<Refined<i32, Outside>, _> = Refined::try_new(5_i32);
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: 5 });
    }

    #[test]
    fn xor_rejects_when_neither_operand_accepts() {
        // `>= 100` XOR `<= -100`: `0` satisfies neither, so both
        // reject and `Xor` rejects with the offending value.
        type Extremes = Xor<AtLeast<100>, AtMost<-100>>;
        let result: Result<Refined<i32, Extremes>, _> = Refined::try_new(0_i32);
        assert_eq!(result.unwrap_err(), NumericError::OutOfRange { value: 0 });
    }

    // ─── N-ary `All<(...)>` `Rule` impl, arities 2..=8. Each arity
    //     is a separate monomorphisation; admit a value passing all
    //     operands, then reject with a value failing one — the
    //     shared `NumericError` surfaces directly. ─────────────────

    #[test]
    fn all_arity_2_admits_and_rejects() {
        type R = All<(AtLeast<0>, AtMost<100>)>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(101_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 101 });
    }

    #[test]
    fn all_arity_3_admits_and_rejects() {
        type R = All<(AtLeast<0>, AtMost<100>, NonZero)>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(0_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 0 });
    }

    #[test]
    fn all_arity_4_admits_and_rejects() {
        type R = All<(AtLeast<0>, AtMost<100>, NonZero, GreaterThan<-1>)>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(0_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 0 });
    }

    #[test]
    fn all_arity_5_admits_and_rejects() {
        type R = All<(
            AtLeast<0>,
            AtMost<100>,
            NonZero,
            GreaterThan<-1>,
            LessThan<200>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(101_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 101 });
    }

    #[test]
    fn all_arity_6_admits_and_rejects() {
        type R = All<(
            AtLeast<0>,
            AtMost<100>,
            NonZero,
            GreaterThan<-1>,
            LessThan<200>,
            Within<0, 100>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(0_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 0 });
    }

    #[test]
    fn all_arity_7_admits_and_rejects() {
        type R = All<(
            AtLeast<0>,
            AtMost<100>,
            NonZero,
            GreaterThan<-1>,
            LessThan<200>,
            Within<0, 100>,
            AtLeast<10>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(5_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 5 });
    }

    #[test]
    fn all_arity_8_admits_and_rejects() {
        type R = All<(
            AtLeast<0>,
            AtMost<100>,
            NonZero,
            GreaterThan<-1>,
            LessThan<200>,
            Within<0, 100>,
            AtLeast<10>,
            AtMost<90>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(50_i32).unwrap();
        assert_eq!(*ok.as_inner(), 50_i32);
        let bad: Result<Refined<i32, R>, _> = Refined::try_new(91_i32);
        assert_eq!(bad.unwrap_err(), NumericError::OutOfRange { value: 91 });
    }

    // ─── N-ary `Any<(...)>` `Rule` impl, arities 2..=8. Admit via
    //     one branch; reject with a value failing every operand and
    //     assert the full `[NumericError; N]` rejection array. ─────

    #[test]
    fn any_arity_2_admits_and_rejects() {
        type R = Any<(EqualTo<1>, EqualTo<2>)>;
        let ok: Refined<i32, R> = Refined::try_new(2_i32).unwrap();
        assert_eq!(*ok.as_inner(), 2_i32);
        let bad: [NumericError; 2] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_3_admits_and_rejects() {
        type R = Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>)>;
        let ok: Refined<i32, R> = Refined::try_new(3_i32).unwrap();
        assert_eq!(*ok.as_inner(), 3_i32);
        let bad: [NumericError; 3] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_4_admits_and_rejects() {
        type R = Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>, EqualTo<4>)>;
        let ok: Refined<i32, R> = Refined::try_new(4_i32).unwrap();
        assert_eq!(*ok.as_inner(), 4_i32);
        let bad: [NumericError; 4] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_5_admits_and_rejects() {
        type R = Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>, EqualTo<4>, EqualTo<5>)>;
        let ok: Refined<i32, R> = Refined::try_new(5_i32).unwrap();
        assert_eq!(*ok.as_inner(), 5_i32);
        let bad: [NumericError; 5] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_6_admits_and_rejects() {
        type R = Any<(
            EqualTo<1>,
            EqualTo<2>,
            EqualTo<3>,
            EqualTo<4>,
            EqualTo<5>,
            EqualTo<6>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(6_i32).unwrap();
        assert_eq!(*ok.as_inner(), 6_i32);
        let bad: [NumericError; 6] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_7_admits_and_rejects() {
        type R = Any<(
            EqualTo<1>,
            EqualTo<2>,
            EqualTo<3>,
            EqualTo<4>,
            EqualTo<5>,
            EqualTo<6>,
            EqualTo<7>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(7_i32).unwrap();
        assert_eq!(*ok.as_inner(), 7_i32);
        let bad: [NumericError; 7] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[test]
    fn any_arity_8_admits_and_rejects() {
        type R = Any<(
            EqualTo<1>,
            EqualTo<2>,
            EqualTo<3>,
            EqualTo<4>,
            EqualTo<5>,
            EqualTo<6>,
            EqualTo<7>,
            EqualTo<8>,
        )>;
        let ok: Refined<i32, R> = Refined::try_new(8_i32).unwrap();
        assert_eq!(*ok.as_inner(), 8_i32);
        let bad: [NumericError; 8] = Refined::<i32, R>::try_new(9_i32).unwrap_err();
        assert_eq!(
            bad,
            core::array::from_fn(|_| NumericError::OutOfRange { value: 9 })
        );
    }

    #[cfg(feature = "proptest")]
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
            // `And<A, B>`'s `ArbitraryRule` impl can generate from
            // either operand and filters through the other. Every
            // emitted value must satisfy both operands.
            proptest::prop_assert!((50..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_and_string_length_and_alphabet_does_not_reject_to_death(
            r in proptest::arbitrary::any::<
                Refined<String, And<LenChars<3, 3>, EachChar<AsciiAlphanumeric>>>,
            >()
        ) {
            let value = r.as_inner();
            proptest::prop_assert_eq!(value.chars().count(), 3);
            proptest::prop_assert!(value.chars().all(|ch| ch.is_ascii_alphanumeric()));
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

        #[test]
        fn arbitrary_map_err_preserves_inner_strategy(
            r in proptest::arbitrary::any::<Refined<i32, UpperHalf>>()
        ) {
            proptest::prop_assert!((50..=100).contains(r.as_inner()));
        }

        // ─── `Xor<A, B>`'s `ArbitraryRule` impl: union of both
        //     strategies filtered to the symmetric difference. ─────

        #[test]
        fn arbitrary_xor_admits_only_symmetric_difference(
            r in proptest::arbitrary::any::<
                Refined<i32, Xor<AtMost<0>, AtLeast<100>>>,
            >()
        ) {
            // `AtMost<0>` and `AtLeast<100>` are disjoint, so the
            // symmetric difference is their union; every emitted
            // value satisfies exactly one operand.
            let value = *r.as_inner();
            proptest::prop_assert!((value <= 0) ^ (value >= 100));
        }

        // ─── N-ary `All<(...)>`'s `ArbitraryRule` impl, arities
        //     2..=8. `Within<0, 100>` is the generator; the
        //     remaining operands trim densely so the filter chain
        //     does not exhaust the retry budget. Every emitted
        //     value lies in the intersection. ────────────────────

        #[test]
        fn arbitrary_all_arity_2_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(Within<0, 100>, AtLeast<50>)>>,
            >()
        ) {
            proptest::prop_assert!((50..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_3_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(Within<0, 100>, AtLeast<50>, AtMost<90>)>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_4_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(Within<0, 100>, AtLeast<50>, AtMost<90>, NonZero)>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_5_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(
                    Within<0, 100>,
                    AtLeast<50>,
                    AtMost<90>,
                    NonZero,
                    GreaterThan<49>,
                )>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_6_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(
                    Within<0, 100>,
                    AtLeast<50>,
                    AtMost<90>,
                    NonZero,
                    GreaterThan<49>,
                    LessThan<91>,
                )>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_7_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(
                    Within<0, 100>,
                    AtLeast<50>,
                    AtMost<90>,
                    NonZero,
                    GreaterThan<49>,
                    LessThan<91>,
                    AtLeast<0>,
                )>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_all_arity_8_in_intersection(
            r in proptest::arbitrary::any::<
                Refined<i32, All<(
                    Within<0, 100>,
                    AtLeast<50>,
                    AtMost<90>,
                    NonZero,
                    GreaterThan<49>,
                    LessThan<91>,
                    AtLeast<0>,
                    AtMost<100>,
                )>>,
            >()
        ) {
            proptest::prop_assert!((50..=90).contains(r.as_inner()));
        }

        // ─── N-ary `Any<(...)>`'s `ArbitraryRule` impl, arities
        //     2..=8. `prop_oneof!` over each `EqualTo<N>` strategy;
        //     every emitted value equals one of the operands. ─────

        #[test]
        fn arbitrary_any_arity_2_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(EqualTo<1>, EqualTo<2>)>>,
            >()
        ) {
            proptest::prop_assert!([1, 2].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_3_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>)>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_4_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>, EqualTo<4>)>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3, 4].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_5_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(EqualTo<1>, EqualTo<2>, EqualTo<3>, EqualTo<4>, EqualTo<5>)>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3, 4, 5].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_6_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(
                    EqualTo<1>,
                    EqualTo<2>,
                    EqualTo<3>,
                    EqualTo<4>,
                    EqualTo<5>,
                    EqualTo<6>,
                )>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3, 4, 5, 6].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_7_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(
                    EqualTo<1>,
                    EqualTo<2>,
                    EqualTo<3>,
                    EqualTo<4>,
                    EqualTo<5>,
                    EqualTo<6>,
                    EqualTo<7>,
                )>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3, 4, 5, 6, 7].contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_any_arity_8_in_union(
            r in proptest::arbitrary::any::<
                Refined<i32, Any<(
                    EqualTo<1>,
                    EqualTo<2>,
                    EqualTo<3>,
                    EqualTo<4>,
                    EqualTo<5>,
                    EqualTo<6>,
                    EqualTo<7>,
                    EqualTo<8>,
                )>>,
            >()
        ) {
            proptest::prop_assert!([1, 2, 3, 4, 5, 6, 7, 8].contains(r.as_inner()));
        }
    }

    // ─── Serde: `And` / `All` forward to the first operand's hook
    //     (streaming when it is `LenItems`), then refine through the
    //     remaining operands. Accept/reject set and error text are
    //     identical to the parse-then-refine path. ──────────────────

    #[cfg(feature = "serde")]
    mod serde_forwarding {
        use alloc::string::{String, ToString};
        use alloc::vec;
        use alloc::vec::Vec;

        use super::CodeRule;
        use crate::composition::{All, And};
        use crate::primitive::{Distinct, IdentityKey, LenItems, Sorted};
        use crate::rule::Refined;

        #[test]
        fn serde_and_admits_through_forwarded_hook() {
            type R = And<LenItems<1, 3>, Distinct<i32>>;
            let refined: Refined<Vec<i32>, R> = serde_json::from_str("[1,2]").unwrap();
            assert_eq!(refined.as_inner(), &[1, 2]);
        }

        #[test]
        fn serde_and_left_operand_streams_length_rejection() {
            // The left `LenItems` hook rejects with the true total
            // length — same text as `try_new` on the full payload.
            type R = And<LenItems<1, 3>, Distinct<i32>>;
            let direct = Refined::<Vec<i32>, R>::try_new(vec![1, 2, 3, 4, 5])
                .unwrap_err()
                .to_string();
            let message = serde_json::from_str::<Refined<Vec<i32>, R>>("[1,2,3,4,5]")
                .unwrap_err()
                .to_string();
            assert_eq!(direct, "length 5 not in admissible range");
            assert!(
                message.contains(&direct),
                "serde error {message:?} must embed the rule error {direct:?}",
            );
        }

        #[test]
        fn serde_and_right_operand_rejection_matches_try_new() {
            // Element-rule failure after the streamed length bound:
            // the right operand's typed error surfaces with the same
            // text the construction path produces.
            type R = And<LenItems<1, 3>, Distinct<i32>>;
            let direct = Refined::<Vec<i32>, R>::try_new(vec![1, 2, 1])
                .unwrap_err()
                .to_string();
            let message = serde_json::from_str::<Refined<Vec<i32>, R>>("[1,2,1]")
                .unwrap_err()
                .to_string();
            assert_eq!(direct, "duplicate key at index 2");
            assert!(
                message.contains(&direct),
                "serde error {message:?} must embed the rule error {direct:?}",
            );
        }

        /// Element type whose `Deserialize` impl counts
        /// materializations; proves the `And` hook streams its left
        /// `LenItems` operand. Equal values keep `Sorted` (non-strict)
        /// admissible on the accept path.
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        struct CountedAnd;

        static COUNTED_AND_MATERIALIZED: core::sync::atomic::AtomicUsize =
            core::sync::atomic::AtomicUsize::new(0);

        impl<'de> serde::Deserialize<'de> for CountedAnd {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                COUNTED_AND_MATERIALIZED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                serde::de::IgnoredAny::deserialize(deserializer).map(|_ignored| Self)
            }
        }

        #[test]
        fn serde_and_streams_left_len_items_bound() {
            // Early-abort proof through the `And` hook: 8 elements
            // against MAX = 3 materialize at most 3 `CountedAnd`s, and
            // the error still reports the true total length.
            type R = And<LenItems<0, 3>, Sorted<CountedAnd, IdentityKey<CountedAnd>>>;
            let result: Result<Refined<Vec<CountedAnd>, R>, _> =
                serde_json::from_str("[0,1,2,3,4,5,6,7]");
            let message = result.unwrap_err().to_string();
            assert!(
                message.contains("length 8 not in admissible range"),
                "error must report the true total length: {message}",
            );
            let materialized = COUNTED_AND_MATERIALIZED.load(core::sync::atomic::Ordering::Relaxed);
            assert!(
                materialized <= 3,
                "at most MAX elements may be materialized, got {materialized}",
            );
        }

        #[test]
        fn serde_all_arity_2_admits_and_rejects_like_try_new() {
            type R = All<(LenItems<1, 3>, Distinct<i32>)>;
            let ok: Refined<Vec<i32>, R> = serde_json::from_str("[1,2]").unwrap();
            assert_eq!(ok.as_inner(), &[1, 2]);

            let direct = Refined::<Vec<i32>, R>::try_new(vec![1, 2, 3, 4])
                .unwrap_err()
                .to_string();
            let message = serde_json::from_str::<Refined<Vec<i32>, R>>("[1,2,3,4]")
                .unwrap_err()
                .to_string();
            assert!(
                message.contains(&direct),
                "serde error {message:?} must embed the rule error {direct:?}",
            );

            // Remaining-operand rejection through the same instantiation.
            let duplicate = serde_json::from_str::<Refined<Vec<i32>, R>>("[1,1]")
                .unwrap_err()
                .to_string();
            assert!(
                duplicate.contains("duplicate key at index 1"),
                "unexpected diagnostic: {duplicate}",
            );
        }

        #[test]
        fn serde_all_arity_3_runs_remaining_operands_in_order() {
            type R = All<(LenItems<1, 3>, Distinct<i32>, Sorted<i32, IdentityKey<i32>>)>;
            let ok: Refined<Vec<i32>, R> = serde_json::from_str("[1,2]").unwrap();
            assert_eq!(ok.as_inner(), &[1, 2]);

            // Second operand rejects first ([2, 2] is sorted but not
            // distinct) — operand order is `refine`'s.
            let duplicate = serde_json::from_str::<Refined<Vec<i32>, R>>("[2,2]")
                .unwrap_err()
                .to_string();
            assert!(
                duplicate.contains("duplicate key at index 1"),
                "unexpected diagnostic: {duplicate}",
            );

            // Third operand rejects ([2, 1] is distinct but unsorted).
            let unsorted = serde_json::from_str::<Refined<Vec<i32>, R>>("[2,1]")
                .unwrap_err()
                .to_string();
            assert!(
                unsorted.contains("element at index 1 breaks ascending order"),
                "unexpected diagnostic: {unsorted}",
            );

            // First operand (streaming `LenItems`) rejects over-MAX.
            let too_long = serde_json::from_str::<Refined<Vec<i32>, R>>("[1,2,3,4]")
                .unwrap_err()
                .to_string();
            assert!(
                too_long.contains("length 4 not in admissible range"),
                "unexpected diagnostic: {too_long}",
            );
        }

        #[test]
        fn serde_map_err_default_path_keeps_error_mapping() {
            // `MapErr` must NOT forward to `R`'s hook: the mapped
            // diagnostics are load-bearing, so the default
            // parse-then-refine path runs `M`'s mapping.
            let message = serde_json::from_str::<Refined<String, CodeRule>>(r#""ab""#)
                .unwrap_err()
                .to_string();
            assert!(
                message.contains("bad length"),
                "mapped error must surface, got: {message}",
            );

            // Both mapped variants render through the default path.
            let bad_char = serde_json::from_str::<Refined<String, CodeRule>>(r#""A-z""#)
                .unwrap_err()
                .to_string();
            assert!(
                bad_char.contains("bad character"),
                "mapped error must surface, got: {bad_char}",
            );
        }
    }

    // ─── `SchemaRule`: combinator schemas compose constructively. ──

    mod schema {
        use alloc::string::String;
        use alloc::vec;

        use super::{All, And, Any, Or};
        use crate::primitive::{
            AsciiAlphanumeric, AtLeast, AtMost, EachChar, EqualTo, IdentChar, LenChars, NonZero,
        };
        use crate::schema::SchemaRule;
        use crate::schema::{Bound, CharSet, LenBound, LenUnit, Scalar, ScalarKind, Schema};

        fn int_interval(lo: i128, hi: i128) -> Schema {
            Schema::interval(
                ScalarKind::Integer,
                Bound::Inclusive(Scalar::Int(lo)),
                Bound::Inclusive(Scalar::Int(hi)),
            )
        }

        /// `And` over numeric operands fuses to the single interval
        /// the equivalent nominal rule would carry: the schema-level
        /// `And<AtLeast<0>, AtMost<100>> ≡ Within<0, 100>` law.
        #[test]
        fn and_schema_is_the_fused_intersection() {
            assert_eq!(
                <And<AtLeast<0>, AtMost<100>> as SchemaRule<i32>>::schema(),
                int_interval(0, 100),
            );
        }

        /// `And` over string operands rides the Str fusion: the
        /// `BoundedLine` shape collapses to one `Str` node.
        #[test]
        fn and_schema_fuses_string_vocabulary() {
            type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
            assert_eq!(
                <Ident as SchemaRule<String>>::schema(),
                Schema::string(
                    LenBound::new(1, 10),
                    LenUnit::Chars,
                    CharSet::from_ranges([('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')]),
                    None,
                ),
            );
        }

        /// `Or` is the union of its operands' sets (same-kind
        /// operands keep every verdict decidable).
        #[test]
        fn or_schema_is_the_union() {
            assert_eq!(
                <Or<AtMost<10>, AtLeast<100>> as SchemaRule<i32>>::schema(),
                Schema::union(vec![
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Unbounded,
                        Bound::Inclusive(Scalar::Int(10)),
                    ),
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Inclusive(Scalar::Int(100)),
                        Bound::Unbounded,
                    ),
                ]),
            );
        }

        /// `All` intersects every operand; `NonZero` (a `Not`
        /// composition) contributes its two-interval union, which
        /// the fused range then narrows.
        #[test]
        fn all_schema_intersects_every_operand() {
            type R = All<(AtLeast<0>, AtMost<100>, NonZero)>;
            assert_eq!(
                <R as SchemaRule<i32>>::schema(),
                Schema::intersection(vec![
                    int_interval(0, 100),
                    Schema::union(vec![
                        Schema::interval(
                            ScalarKind::Integer,
                            Bound::Unbounded,
                            Bound::Inclusive(Scalar::Int(-1)),
                        ),
                        Schema::interval(
                            ScalarKind::Integer,
                            Bound::Inclusive(Scalar::Int(1)),
                            Bound::Unbounded,
                        ),
                    ]),
                ]),
            );
        }

        /// `Any` unions every operand's point set.
        #[test]
        fn any_schema_unions_every_operand() {
            type R = Any<(EqualTo<1>, EqualTo<3>, EqualTo<6>)>;
            assert_eq!(
                <R as SchemaRule<i32>>::schema(),
                Schema::union(vec![
                    int_interval(1, 1),
                    int_interval(3, 3),
                    int_interval(6, 6),
                ]),
            );
        }

        /// `Not` over an interval rule is the interval's complement:
        /// a union of at most two half-bounded intervals.
        #[test]
        fn not_schema_is_the_interval_complement() {
            use crate::Not;
            use crate::primitive::Within;

            assert_eq!(
                <Not<Within<10, 20>> as SchemaRule<i32>>::schema(),
                Schema::union(vec![
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Unbounded,
                        Bound::Inclusive(Scalar::Int(9)),
                    ),
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Inclusive(Scalar::Int(21)),
                        Bound::Unbounded,
                    ),
                ]),
            );
            // A half-bounded operand leaves a single complement piece.
            assert_eq!(
                <Not<AtLeast<5>> as SchemaRule<i32>>::schema(),
                Schema::interval(
                    ScalarKind::Integer,
                    Bound::Unbounded,
                    Bound::Inclusive(Scalar::Int(4)),
                ),
            );
            assert_eq!(
                <Not<AtMost<5>> as SchemaRule<i32>>::schema(),
                Schema::interval(
                    ScalarKind::Integer,
                    Bound::Inclusive(Scalar::Int(6)),
                    Bound::Unbounded,
                ),
            );
        }

        /// The complement of an everything-admitting interval admits
        /// nothing — unrepresentable by construction.
        #[test]
        #[should_panic(expected = "at least one member is required")]
        fn not_schema_panics_for_an_everything_admitting_operand() {
            use crate::Not;
            let _schema = <Not<AtLeast<{ i128::MIN }>> as SchemaRule<i128>>::schema();
        }

        /// `Xor` is the symmetric difference, desugared as
        /// `(A ∧ ¬B) ∨ (¬A ∧ B)` over interval pieces.
        #[test]
        fn xor_schema_is_the_symmetric_difference() {
            use crate::Xor;
            use crate::primitive::Within;

            // Inside [0, 10] both accept (rejected); outside exactly
            // one does (admitted) — the two-tail union.
            assert_eq!(
                <Xor<AtLeast<0>, AtMost<10>> as SchemaRule<i32>>::schema(),
                Schema::union(vec![
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Unbounded,
                        Bound::Inclusive(Scalar::Int(-1)),
                    ),
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Inclusive(Scalar::Int(11)),
                        Bound::Unbounded,
                    ),
                ]),
            );
            // Overlapping closed intervals: the two one-sided
            // leftovers survive, the overlap drops.
            assert_eq!(
                <Xor<Within<0, 10>, Within<5, 15>> as SchemaRule<i32>>::schema(),
                Schema::union(vec![int_interval(0, 4), int_interval(11, 15)]),
            );
        }

        /// Equal operands have an empty symmetric difference: `Xor`
        /// admits nothing — unrepresentable by construction.
        #[test]
        #[should_panic(expected = "at least one member is required")]
        fn xor_schema_panics_for_equal_operands() {
            use crate::Xor;
            let _schema = <Xor<AtLeast<0>, AtLeast<0>> as SchemaRule<i32>>::schema();
        }

        /// `MapErr` is transparent: the mapped rule's schema IS the
        /// inner rule's schema (error mapping never moves the set).
        #[test]
        fn map_err_schema_is_transparent() {
            type Inner = And<LenChars<3, 3>, EachChar<AsciiAlphanumeric>>;
            assert_eq!(
                <super::CodeRule as SchemaRule<String>>::schema(),
                <Inner as SchemaRule<String>>::schema(),
            );
        }

        #[cfg(feature = "proptest")]
        mod cross_checks {
            use super::super::{All, And, Any, Or};
            use crate::primitive::{
                AtLeast, AtMost, EachChar, EqualTo, IdentChar, LenChars, NonZero, Within,
            };
            use crate::schema::{Scalar, ScalarKind};
            use crate::testing::{
                assert_schema_boundary_matrix, prop_schema_cross_check,
                prop_string_schema_cross_check,
            };

            #[expect(
                clippy::trivially_copy_pass_by_ref,
                reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
            )]
            fn embed_i32(value: &i32) -> (ScalarKind, Scalar) {
                (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
            }

            #[expect(
                clippy::return_and_then,
                reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                          would add a None arm no boundary candidate reaches"
            )]
            fn extract_i32(_kind: ScalarKind, scalar: Scalar) -> Option<i32> {
                scalar
                    .as_int()
                    .and_then(|widened| i32::try_from(widened).ok())
            }

            /// Every fusion path is wired against the oracles: the
            /// derived boundary matrix and the strategy-membership
            /// cross-check agree with refine for conjunctions,
            /// disjunctions, and their n-ary forms. (Conjunction
            /// operands overlap densely — the And/All strategies
            /// rejection-sample the opposite operand, so a sparse
            /// intersection cannot drive the sample obligation.)
            #[test]
            fn schema_cross_checks_numeric_combinators() {
                prop_schema_cross_check::<i32, And<Within<0, 100>, AtLeast<50>>>(
                    embed_i32,
                    extract_i32,
                );
                prop_schema_cross_check::<i32, Or<AtMost<10>, AtLeast<100>>>(
                    embed_i32,
                    extract_i32,
                );
                prop_schema_cross_check::<i32, All<(Within<0, 100>, AtLeast<10>, NonZero)>>(
                    embed_i32,
                    extract_i32,
                );
                prop_schema_cross_check::<i32, Any<(EqualTo<1>, EqualTo<3>, EqualTo<6>)>>(
                    embed_i32,
                    extract_i32,
                );
            }

            /// The boundary matrix alone, for the standalone helper's
            /// combinator instantiation.
            #[test]
            fn boundary_matrices_for_numeric_combinators() {
                assert_schema_boundary_matrix::<i32, And<AtLeast<0>, AtMost<100>>>(
                    embed_i32,
                    extract_i32,
                );
                assert_schema_boundary_matrix::<i32, Or<AtMost<10>, AtLeast<100>>>(
                    embed_i32,
                    extract_i32,
                );
            }

            /// The fused string schema agrees with the composed
            /// refine at every derived length edge and alphabet
            /// near-miss, and the And strategy emits only members.
            #[test]
            fn schema_cross_checks_string_conjunction() {
                prop_string_schema_cross_check::<And<LenChars<1, 10>, EachChar<IdentChar>>>();
            }

            /// The complement and symmetric-difference schemas agree
            /// with refine at every derived boundary, and the
            /// filter-based Not/Xor strategies emit only members.
            #[test]
            fn schema_cross_checks_not_and_xor() {
                use crate::{Not, Xor};

                prop_schema_cross_check::<i32, Not<Within<10, 20>>>(embed_i32, extract_i32);
                prop_schema_cross_check::<i32, Xor<AtLeast<0>, AtMost<10>>>(embed_i32, extract_i32);
            }
        }
    }
}
