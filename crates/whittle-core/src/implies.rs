//! Implication edges between rules and the `weaken` upcast.
//!
//! IDEA.md §5.7: when rule `S` is logically stronger than rule `W`
//! (`adm(S) ⊆ adm(W)`), the implication is expressed through an
//! explicit [`Implies`] impl and [`Refined::weaken`] converts the
//! carrier without re-running either rule's narrowing morphism. No
//! implication is ever inferred from const expressions or generic
//! constraints — every edge is declared, either here (the documented
//! common cases: numeric range narrowing and length narrowing) or by
//! the user.
//!
//! # Library-supplied edges
//!
//! | Stronger | Weaker | Side condition |
//! |---|---|---|
//! | `Within<A, B>` | `Within<C, D>` | `C <= A && B <= D` |
//! | `Within<A, B>` | `AtLeast<C>` | `C <= A` |
//! | `Within<A, B>` | `AtMost<D>` | `B <= D` |
//! | `AtLeast<A>` | `AtLeast<C>` | `C <= A` |
//! | `AtMost<B>` | `AtMost<D>` | `B <= D` |
//! | `GreaterThan<A>` | `GreaterThan<C>` | `C <= A` |
//! | `LessThan<B>` | `LessThan<D>` | `B <= D` |
//!
//! Each side condition is checked at compile time through the
//! impl's [`Implies::VALID`] const: a `weaken` call whose
//! instantiation violates the condition fails at monomorphisation
//! (the same `const { assert!(...) }` mechanism as `Within`'s
//! `MIN <= MAX` gate).
//!
//! # Contract discharge for the library edges
//!
//! IDEA §5.7 obliges every implication impl to establish three
//! properties. For all seven edges above:
//!
//! 1. **Admissibility containment** — the side condition in the
//!    table is exactly interval containment of the stronger rule's
//!    admissible set in the weaker rule's, documented per impl.
//! 2. **Canonical-form compatibility** — every listed rule is a pure
//!    predicate: `refine` returns the input unchanged on admissible
//!    values (the numeric rules round-trip losslessly through `i128`,
//!    which is the identity for every `Numeric` carrier). No rule
//!    canonicalises, so the property holds trivially.
//! 3. **No re-run dependence** — the listed rules' only observable
//!    behaviour is accept/reject at construction; nothing downstream
//!    depends on re-running the weaker rule's narrowing morphism.
//!
//! # Edges deliberately not supplied (v1)
//!
//! - **Cross-shape strict/inclusive edges** (`GreaterThan<A>` →
//!   `AtLeast<C>`, `LessThan<B>` → `AtMost<D>`, `AtLeast`/`AtMost` →
//!   `GreaterThan`/`LessThan`, `Within` → `GreaterThan`/`LessThan`).
//!   The order-theoretic side condition (e.g. `C <= A`) is sound but
//!   strictly tighter than the integer-exact one (`C <= A + 1`,
//!   since `GreaterThan<A>` ≡ `AtLeast<A + 1>` over the discrete
//!   `Numeric` carriers). Shipping the conservative condition would
//!   reject valid widenings and could only be loosened later (an
//!   observable contract change); shipping the exact condition
//!   couples the edge to carrier discreteness. Declare these
//!   per-site as user edges when needed.
//! - **`EqualTo` / `NotEqualTo` edges** (`EqualTo<N>` →
//!   `Within<C, D>` when `C <= N && N <= D`, ...). Clean but outside
//!   §5.7's documented common cases (range and length narrowing);
//!   deferred until dogfooding demands them.
//! - **Transformer and composition edges** (`And`, `Or`, `All`,
//!   `Trim<R>`, ...). Out of v1 scope per §5.7.
//!
//! # Irreflexivity
//!
//! §5.7 requires the trait to be irreflexive *at the user level*: no
//! implementer declares a self-edge such as
//! `impl Implies<MyRule> for MyRule`. The const-generic family impls
//! above necessarily include the degenerate instantiation where the
//! source and target parameters coincide (`Within<0, 100>` →
//! `Within<0, 100>`); that is not a declared self-edge — the
//! declared edge is the *family* `Within<A, B>` → `Within<C, D>`,
//! and the degenerate member is a trivially-valid containment under
//! which `weaken` is a no-op. Transitive edges are not derived
//! (OPTIONAL per §5.7): if `A: Implies<B>` and `B: Implies<C>` hold,
//! `A: Implies<C>` must be declared explicitly.

use crate::primitive::{AtLeast, AtMost, GreaterThan, LessThan, Within};
use crate::rule::{Refined, Rule};

/// Marker trait: `Self` is logically stronger than `W`
/// (`adm(Self) ⊆ adm(W)`).
///
/// Declaring `S: Implies<W>` unlocks [`Refined::weaken`], the
/// proof-preserving upcast from `Refined<T, S>` to `Refined<T, W>`
/// that does **not** re-run either rule's narrowing morphism.
///
/// # Contract (IDEA §5.7)
///
/// Implementers MUST establish, and MUST document, all three:
///
/// 1. `adm(S) ⊆ adm(W)` — every value the stronger rule admits also
///    satisfies the weaker rule;
/// 2. when `W` canonicalises, every value in the range of
///    `S::refine` is already in the range of `W::refine` — the
///    stronger rule's canonical form is canonical-enough for the
///    weaker rule;
/// 3. the weaker rule has no observable behaviour that depends on
///    re-running its narrowing morphism on the upcast value.
///
/// Users MUST NOT add impls that violate the contract, and MUST NOT
/// declare a self-edge (`impl Implies<MyRule> for MyRule`); see the
/// module docs for how this irreflexivity reads against the
/// const-generic family impls.
///
/// # Examples
///
/// A user-declared edge between two custom rules:
///
/// ```
/// use whittle_core::{Implies, Refined, Rule};
///
/// /// Admits multiples of four.
/// enum MultipleOfFour {}
///
/// /// Admits even values.
/// enum Even {}
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct NotAdmitted;
///
/// impl Rule<i32> for MultipleOfFour {
///     type Error = NotAdmitted;
///     fn refine(raw: i32) -> Result<i32, Self::Error> {
///         if raw % 4 == 0 { Ok(raw) } else { Err(NotAdmitted) }
///     }
/// }
///
/// impl Rule<i32> for Even {
///     type Error = NotAdmitted;
///     fn refine(raw: i32) -> Result<i32, Self::Error> {
///         if raw % 2 == 0 { Ok(raw) } else { Err(NotAdmitted) }
///     }
/// }
///
/// // Contract discharge:
/// // 1. every multiple of four is even;
/// // 2. neither rule canonicalises (pure predicates);
/// // 3. `Even` has no behaviour beyond accept/reject at
/// //    construction.
/// impl Implies<Even> for MultipleOfFour {}
///
/// let strong: Refined<i32, MultipleOfFour> = Refined::try_new(8).unwrap();
/// let weak: Refined<i32, Even> = strong.weaken();
/// assert_eq!(*weak.as_inner(), 8);
/// ```
pub trait Implies<W>: Sized {
    /// Compile-time witness that the implication's side conditions
    /// hold for this monomorphisation.
    ///
    /// Defaults to `()` for unconditional (user-declared) edges.
    /// Const-generic family impls override it with an
    /// `assert!(...)`-carrying body, so a `weaken` call on an
    /// instantiation whose side condition fails is a compile error
    /// at monomorphisation rather than an unsound upcast — the same
    /// house pattern as `Within`'s `MIN <= MAX` gate.
    const VALID: () = ();
}

impl<T, S> Refined<T, S>
where
    T: 'static,
    S: Rule<T>,
{
    /// Upcast to the weaker rule `W` without re-running either
    /// rule's narrowing morphism.
    ///
    /// This is the explicit upcast IDEA §5.7 requires whenever
    /// `S: Implies<W>` holds (a blanket `From` impl would overlap
    /// with `core`'s reflexive `From<X> for X` and is rejected by
    /// coherence). The inner value is moved, not cloned, and not
    /// re-validated: the implication contract is the proof that the
    /// value is admissible under `W` in `W`'s canonical form.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::Within;
    ///
    /// let tight: Refined<i32, Within<10, 20>> = Refined::try_new(15).unwrap();
    ///
    /// // Widen: `[10, 20] ⊆ [0, 100]`. The value is moved as-is —
    /// // no `refine` runs, no proof-erasing round-trip through
    /// // `into_inner` → `try_new`.
    /// let wide: Refined<i32, Within<0, 100>> = tight.weaken();
    /// assert_eq!(*wide.as_inner(), 15);
    /// ```
    ///
    /// An instantiation whose side condition fails is a compile
    /// error at monomorphisation — `[0, 100]` is not contained in
    /// `[10, 50]`:
    ///
    /// ```compile_fail
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::Within;
    ///
    /// let wide: Refined<i32, Within<0, 100>> = Refined::try_new(42).unwrap();
    /// // error[E0080]: Within widening requires the target range to
    /// // contain the source range
    /// let narrow: Refined<i32, Within<10, 50>> = wide.weaken();
    /// ```
    #[inline]
    #[must_use]
    pub fn weaken<W>(self) -> Refined<T, W>
    where
        S: Implies<W>,
        W: Rule<T>,
    {
        const { <S as Implies<W>>::VALID };
        // SOUNDNESS (`from_inner` call-site obligation): existence
        // of `self` proves `S::refine` accepted the inner value at
        // construction. `S: Implies<W>` contract property 1 gives
        // `adm(S) ⊆ adm(W)`, so the value is admissible under `W`;
        // property 2 gives canonical-form compatibility, so it is
        // already in `W::refine`'s range; property 3 rules out any
        // observable dependence on re-running `W::refine`. The
        // family impls' side conditions are enforced by the
        // `const { VALID }` gate above.
        Refined::from_inner(self.into_inner())
    }
}

// ─── Numeric range narrowing (IDEA §5.7 documented common case). ──
//
// All five numeric rules are pure predicates over the lossless
// `i128` widening of the carrier, so contract properties 2 and 3
// are discharged module-wide (see the module docs); each impl
// documents property 1 — the interval containment its `VALID`
// condition encodes.

/// Property 1: `[A, B] ⊆ [C, D]` iff `C <= A && B <= D`.
impl<const A: i128, const B: i128, const C: i128, const D: i128> Implies<Within<C, D>>
    for Within<A, B>
{
    /// Containment of the closed source range in the closed target
    /// range.
    const VALID: () = assert!(
        C <= A && B <= D,
        "Within widening requires the target range to contain the source range",
    );
}

/// Property 1: `[A, B] ⊆ [C, +∞)` iff `C <= A`.
impl<const A: i128, const B: i128, const C: i128> Implies<AtLeast<C>> for Within<A, B> {
    /// The target lower bound sits at or below the source lower
    /// bound.
    const VALID: () = assert!(
        C <= A,
        "Within -> AtLeast widening requires the target lower bound \
         to be at most the source lower bound",
    );
}

/// Property 1: `[A, B] ⊆ (-∞, D]` iff `B <= D`.
impl<const A: i128, const B: i128, const D: i128> Implies<AtMost<D>> for Within<A, B> {
    /// The target upper bound sits at or above the source upper
    /// bound.
    const VALID: () = assert!(
        B <= D,
        "Within -> AtMost widening requires the target upper bound \
         to be at least the source upper bound",
    );
}

/// Property 1: `[A, +∞) ⊆ [C, +∞)` iff `C <= A`.
impl<const A: i128, const C: i128> Implies<AtLeast<C>> for AtLeast<A> {
    /// The target lower bound sits at or below the source lower
    /// bound.
    const VALID: () = assert!(
        C <= A,
        "AtLeast widening requires the target lower bound to be at \
         most the source lower bound",
    );
}

/// Property 1: `(-∞, B] ⊆ (-∞, D]` iff `B <= D`.
impl<const B: i128, const D: i128> Implies<AtMost<D>> for AtMost<B> {
    /// The target upper bound sits at or above the source upper
    /// bound.
    const VALID: () = assert!(
        B <= D,
        "AtMost widening requires the target upper bound to be at \
         least the source upper bound",
    );
}

/// Property 1: `(A, +∞) ⊆ (C, +∞)` iff `C <= A` (exact under both
/// dense and discrete order semantics).
impl<const A: i128, const C: i128> Implies<GreaterThan<C>> for GreaterThan<A> {
    /// The target open lower bound sits at or below the source open
    /// lower bound.
    const VALID: () = assert!(
        C <= A,
        "GreaterThan widening requires the target bound to be at \
         most the source bound",
    );
}

/// Property 1: `(-∞, B) ⊆ (-∞, D)` iff `B <= D` (exact under both
/// dense and discrete order semantics).
impl<const B: i128, const D: i128> Implies<LessThan<D>> for LessThan<B> {
    /// The target open upper bound sits at or above the source open
    /// upper bound.
    const VALID: () = assert!(
        B <= D,
        "LessThan widening requires the target bound to be at least \
         the source bound",
    );
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use crate::Refined;
    use crate::primitive::{AtLeast, AtMost, GreaterThan, LessThan, Within};

    // ─── Value preservation, one test per library edge. Each
    //      family impl is exercised under at least two distinct
    //      const instantiations (and, for the numeric edges, two
    //      carrier types) so every monomorphisation the suite
    //      relies on is compiled and run. ──────────────────────────

    #[test]
    fn weaken_within_to_containing_within_preserves_value() {
        let tight: Refined<i32, Within<10, 20>> = Refined::try_new(15).unwrap();
        let wide: Refined<i32, Within<0, 100>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 15);
    }

    #[test]
    fn weaken_within_to_within_second_instantiation() {
        // Distinct const arguments and a distinct carrier (`u8`).
        let tight: Refined<u8, Within<1, 5>> = Refined::try_new(5).unwrap();
        let wide: Refined<u8, Within<0, 10>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 5);
    }

    #[test]
    fn weaken_degenerate_family_edge_is_a_noop() {
        // The family impl's degenerate instantiation (source and
        // target parameters coincide) is trivially-valid
        // containment, not a declared self-edge; `weaken` is then
        // an identity conversion. Pins the irreflexivity reading in
        // the module docs.
        let r: Refined<i32, Within<10, 20>> = Refined::try_new(10).unwrap();
        let same: Refined<i32, Within<10, 20>> = r.weaken();
        assert_eq!(*same.as_inner(), 10);
    }

    #[test]
    fn weaken_within_to_at_least_projection() {
        // Boundary instantiation: target lower bound equals the
        // source lower bound (`C == A`).
        let tight: Refined<i32, Within<10, 20>> = Refined::try_new(10).unwrap();
        let lower: Refined<i32, AtLeast<10>> = tight.weaken();
        assert_eq!(*lower.as_inner(), 10);
    }

    #[test]
    fn weaken_within_to_at_least_second_instantiation() {
        let tight: Refined<u16, Within<50, 60>> = Refined::try_new(60).unwrap();
        let lower: Refined<u16, AtLeast<0>> = tight.weaken();
        assert_eq!(*lower.as_inner(), 60);
    }

    #[test]
    fn weaken_within_to_at_most_projection() {
        // Boundary instantiation: target upper bound equals the
        // source upper bound (`D == B`).
        let tight: Refined<i32, Within<10, 20>> = Refined::try_new(20).unwrap();
        let upper: Refined<i32, AtMost<20>> = tight.weaken();
        assert_eq!(*upper.as_inner(), 20);
    }

    #[test]
    fn weaken_within_to_at_most_second_instantiation() {
        let tight: Refined<i64, Within<-5, 5>> = Refined::try_new(-5).unwrap();
        let upper: Refined<i64, AtMost<100>> = tight.weaken();
        assert_eq!(*upper.as_inner(), -5);
    }

    #[test]
    fn weaken_at_least_to_lower_at_least_preserves_value() {
        let tight: Refined<i32, AtLeast<10>> = Refined::try_new(10).unwrap();
        let wide: Refined<i32, AtLeast<5>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 10);
    }

    #[test]
    fn weaken_at_least_to_at_least_second_instantiation() {
        let tight: Refined<i8, AtLeast<0>> = Refined::try_new(7).unwrap();
        let wide: Refined<i8, AtLeast<-3>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 7);
    }

    #[test]
    fn weaken_at_most_to_higher_at_most_preserves_value() {
        let tight: Refined<i32, AtMost<10>> = Refined::try_new(10).unwrap();
        let wide: Refined<i32, AtMost<20>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 10);
    }

    #[test]
    fn weaken_at_most_to_at_most_second_instantiation() {
        let tight: Refined<u32, AtMost<0>> = Refined::try_new(0).unwrap();
        let wide: Refined<u32, AtMost<5>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 0);
    }

    #[test]
    fn weaken_greater_than_to_lower_greater_than_preserves_value() {
        let tight: Refined<i32, GreaterThan<10>> = Refined::try_new(11).unwrap();
        let wide: Refined<i32, GreaterThan<5>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 11);
    }

    #[test]
    fn weaken_greater_than_to_greater_than_second_instantiation() {
        let tight: Refined<i64, GreaterThan<0>> = Refined::try_new(1).unwrap();
        let wide: Refined<i64, GreaterThan<-1>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 1);
    }

    #[test]
    fn weaken_less_than_to_higher_less_than_preserves_value() {
        let tight: Refined<i32, LessThan<10>> = Refined::try_new(9).unwrap();
        let wide: Refined<i32, LessThan<20>> = tight.weaken();
        assert_eq!(*wide.as_inner(), 9);
    }

    #[test]
    fn weaken_less_than_to_less_than_second_instantiation() {
        let tight: Refined<i16, LessThan<0>> = Refined::try_new(-1).unwrap();
        let wide: Refined<i16, LessThan<1>> = tight.weaken();
        assert_eq!(*wide.as_inner(), -1);
    }

    // ─── IDEA §5.14: "implication edges, where declared, preserve
    //      admissibility." For each library edge, generate through
    //      the STRONGER rule's `ArbitraryRule` strategy, weaken,
    //      and assert the WEAKER rule's `refine` accepts every
    //      sample (contract property 1, checked extensionally). ────

    #[cfg(feature = "proptest")]
    mod implication_preserves_admissibility {
        use crate::primitive::{AtLeast, AtMost, GreaterThan, LessThan, Within};
        use crate::{Refined, Rule};
        use proptest::arbitrary::any;

        proptest::proptest! {
            #[test]
            fn within_to_within(
                strong in any::<Refined<i32, Within<10, 20>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, Within<0, 100>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <Within<0, 100> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn within_to_at_least(
                strong in any::<Refined<i32, Within<10, 20>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, AtLeast<10>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <AtLeast<10> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn within_to_at_most(
                strong in any::<Refined<i32, Within<10, 20>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, AtMost<20>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <AtMost<20> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn at_least_to_at_least(
                strong in any::<Refined<i32, AtLeast<10>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, AtLeast<5>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <AtLeast<5> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn at_most_to_at_most(
                strong in any::<Refined<i32, AtMost<10>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, AtMost<20>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <AtMost<20> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn greater_than_to_greater_than(
                strong in any::<Refined<i32, GreaterThan<10>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, GreaterThan<5>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <GreaterThan<5> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

            #[test]
            fn less_than_to_less_than(
                strong in any::<Refined<i32, LessThan<10>>>()
            ) {
                let value = *strong.as_inner();
                let weak: Refined<i32, LessThan<20>> = strong.weaken();
                proptest::prop_assert_eq!(*weak.as_inner(), value);
                proptest::prop_assert!(
                    <LessThan<20> as Rule<i32>>::refine(weak.into_inner()).is_ok()
                );
            }

        }
    }
}
