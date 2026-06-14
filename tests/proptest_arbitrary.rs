//! `Refined<T, R>: Arbitrary` for proptest.
//!
//! Whittle implements `Arbitrary` for every `Refined<T, R>` where
//! `R: ArbitraryRule<T>`. Each rule supplies a strategy that targets
//! the admissible region directly; the carrier's `Arbitrary` impl
//! maps that strategy through `Refined::try_new`. The blanket impl
//! does no rejection sampling; primitive rules over dense regions
//! (`NonZero`, `NotNan`) may apply a single `prop_filter` whose
//! reject rate is negligible, and `And<A, B>` may filter its right
//! operand. Sparse rules (`Within<0, 100>` over `i32` admits 101
//! values out of 2³²) are as cheap to sample as dense ones (`NonZero`
//! admits every i32 except 0) — no retry-budget exhaustion.
//!
//! Downstream property tests can write
//! `let r in any::<Refined<T, R>>()` for any library-supplied rule
//! and trust that every generated value satisfies the rule.

use proptest::proptest;
use whittle::primitive::{HexFixedAny, NonZero, NotNan, Within};
use whittle::transform::AsciiLowercase;
use whittle::{Refined, refinement};

refinement! {
    /// Percentage used by the integration test below.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    Percent: i32, Within<0, 100>;
}

#[test]
fn dense_rule_non_zero_arbitrary_admits_every_non_zero_value() {
    // ─── Dense rule: `Refined<T, R>: Arbitrary` directly.  ──────
    //
    // `NonZero` over `i32` admits every i32 except `0`. The rule's
    // `ArbitraryRule` strategy emits the full range filtered for
    // non-zero; every value passes `Refined::try_new` by
    // construction.

    proptest!(|(r in proptest::arbitrary::any::<Refined<i32, NonZero>>())| {
        assert!(*r.as_inner() != 0);
    });
}

#[test]
fn dense_rule_not_nan_arbitrary_admits_every_non_nan_f64() {
    // `NotNan` over `f64` is also dense: only NaN is excluded.
    // The rule's strategy emits any `f64` filtered for non-NaN.

    proptest!(|(r in proptest::arbitrary::any::<Refined<f64, NotNan>>())| {
        assert!(!r.as_inner().is_nan());
    });
}

#[test]
fn sparse_rule_within_arbitrary_stays_in_admissible_range() {
    // ─── Sparse rule: introspective generation, no workaround.
    //
    // `Within<0, 100>` over `i32` admits only 101 values out of 2³².
    // Before `ArbitraryRule`, calling `any::<Refined<i32, Within<0,
    // 100>>>()` forced proptest into rejection sampling against an
    // extremely sparse target and exhausted the retry budget. The
    // rule now supplies its own range-bounded strategy, so the
    // sparse case is just as cheap as the dense one.

    proptest!(|(r in proptest::arbitrary::any::<Refined<i32, Within<0, 100>>>())| {
        assert!((0..=100).contains(r.as_inner()));
    });
}

#[test]
fn refinement_newtype_arbitrary_forwards_rule_strategy() {
    // Generated domain newtypes inherit the same rule-derived
    // strategy as their inner `Refined` carrier, so downstream tests
    // can ask proptest for the domain type directly.

    proptest!(|(percent in proptest::arbitrary::any::<Percent>())| {
        assert!((0..=100).contains(percent.as_inner()));
    });
}

#[test]
fn transformer_rule_canonicalises_arbitrary_input_inside_try_new() {
    // ─── Transformer rule: the post-transform invariant.  ───────
    //
    // Every value emitted by the strategy must already equal its
    // own ASCII-lowercase form — that's the canonicalisation
    // promise of `AsciiLowercase<HexFixedAny<2>>`. The inner
    // strategy generates mixed-case input; the transformer runs
    // inside `try_new`, so the stored carrier is canonical.

    proptest!(|(r in proptest::arbitrary::any::<Refined<String, AsciiLowercase<HexFixedAny<2>>>>())| {
        assert_eq!(r.as_inner(), &r.as_inner().to_ascii_lowercase());
    });
}
