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
use whittle::primitive::collection::ArbitraryPredicate;
use whittle::primitive::{
    AllItems, AnyOf, ArbitraryChar, AsciiAlphabetic, AsciiAlphanumeric, AsciiDigit, AsciiGraphic,
    AsciiLowercase as AsciiLowercaseChar, AsciiUppercase, AtLeast, AtMost, CharEither, CharExcept,
    CharLiteral, CharPredicate, Distinct, EachChar, EqualTo, Finite, FirstChar, GreaterThan,
    HexChar, HexFixedAny, HexFixedLower, HexFixedNormalized, IdentChar, IdentDashChar, IdentStart,
    InClosedRange, LenBytes, LenChars, LenItems, LessThan, Negative, NonControl, NonEmpty, NonZero,
    NoneOf, NotEqualTo, NotInfinite, NotNan, Positive, Predicate, RelativePath, Sorted,
    UniqueByKey, Within,
};
use whittle::transform::{
    AsciiLowercase as LowercaseTransform, AsciiUppercase as UppercaseTransform, Trim,
};
use whittle::{
    All, And, Any, ArbitraryRule, MapErr, Not, Or, Refined, SizeProfile, Xor, profiled_refined,
    refinement,
};

#[cfg(feature = "chrono")]
use chrono::{DateTime, NaiveDate, Utc};
#[cfg(feature = "decimal")]
use rust_decimal::Decimal;
#[cfg(feature = "regex")]
use whittle::primitive::Pattern;
#[cfg(feature = "unicode")]
use whittle::primitive::{
    BoundedLine, BoundedText, PrintableChar, PrintableLine, PrintableMultiline,
};

refinement! {
    /// Percentage used by the integration test below.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    Percent: i32, Within<0, 100>;
}

struct IsZero;

impl Predicate<i32> for IsZero {
    fn test(value: &i32) -> bool {
        *value == 0
    }
}

impl ArbitraryPredicate<i32> for IsZero {
    type Strategy = proptest::strategy::BoxedStrategy<i32>;

    fn arbitrary_matching() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::strategy::Just(0_i32).boxed()
    }
}

struct IdentityMap;

impl<E> whittle::ErrorMapper<E> for IdentityMap {
    type Error = E;

    fn map_error(error: E) -> Self::Error {
        error
    }
}

const fn assert_rule<T: 'static, R>()
where
    R: ArbitraryRule<T>,
{
}

const fn assert_char<P>()
where
    P: ArbitraryChar,
{
}

#[cfg(feature = "chrono")]
const fn assert_chrono_rule_families() {
    use whittle::primitive::{
        DateAtLeast, DateAtMost, DateInRange, DateTimeAtLeast, DateTimeAtMost, DateTimeInRange,
    };

    assert_rule::<NaiveDate, DateAtLeast<730_120>>();
    assert_rule::<NaiveDate, DateAtMost<767_009>>();
    assert_rule::<NaiveDate, DateInRange<730_120, 767_009>>();
    assert_rule::<DateTime<Utc>, DateTimeAtLeast<1_704_067_200>>();
    assert_rule::<DateTime<Utc>, DateTimeAtMost<1_893_456_000>>();
    assert_rule::<DateTime<Utc>, DateTimeInRange<1_704_067_200, 1_893_456_000>>();
}

#[cfg(not(feature = "chrono"))]
const fn assert_chrono_rule_families() {}

#[cfg(feature = "decimal")]
const fn assert_decimal_rule_families() {
    use whittle::primitive::{DecimalInRange, DecimalPositive, DecimalPrecision, DecimalScale};

    assert_rule::<Decimal, DecimalPositive>();
    assert_rule::<Decimal, DecimalScale<2>>();
    assert_rule::<Decimal, DecimalPrecision<6>>();
    assert_rule::<Decimal, DecimalInRange<0, 10_000, 2>>();
}

#[cfg(not(feature = "decimal"))]
const fn assert_decimal_rule_families() {}

#[cfg(feature = "regex")]
const fn assert_regex_rule_families() {
    assert_rule::<String, Pattern<"^[A-Z][A-Z0-9]*$">>();
}

#[cfg(not(feature = "regex"))]
const fn assert_regex_rule_families() {}

#[cfg(feature = "unicode")]
const fn assert_unicode_rule_families() {
    assert_char::<PrintableLine>();
    assert_char::<PrintableMultiline>();
    assert_char::<PrintableChar>();
    assert_rule::<String, BoundedLine<32>>();
    assert_rule::<String, BoundedText<32>>();
}

#[cfg(not(feature = "unicode"))]
const fn assert_unicode_rule_families() {}

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
fn profiled_len_chars_small_valid_never_exceeds_the_clamp() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, LenChars<0, 10_000_000>>(SizeProfile::small_valid(4));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled LenChars strategy must produce a value tree")
            .current();
        assert!(value.as_inner().chars().count() <= 4);
    }
}

#[test]
fn profiled_len_chars_small_valid_above_rule_max_keeps_rule_max() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, LenChars<0, 4>>(SizeProfile::small_valid(10));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled LenChars strategy must produce a value tree")
            .current();
        assert!(value.as_inner().chars().count() <= 4);
    }
}

#[test]
fn profiled_len_bytes_small_valid_never_exceeds_the_byte_clamp() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, LenBytes<0, 1024>>(SizeProfile::small_valid(4));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled LenBytes strategy must produce a value tree")
            .current();
        assert!(value.as_inner().len() <= 4);
    }
}

#[test]
fn profiled_len_chars_full_boundary_still_reaches_original_edges() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, LenChars<2, 8>>(SizeProfile::FULL_BOUNDARY);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut saw_min = false;
    let mut saw_max = false;

    for _ in 0_u32..256 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled full LenChars strategy must produce a value tree")
            .current();
        let len = value.as_inner().chars().count();
        saw_min |= len == 2;
        saw_max |= len == 8;
    }

    assert!(saw_min, "FULL_BOUNDARY must still reach MIN");
    assert!(saw_max, "FULL_BOUNDARY must still reach MAX");
}

#[test]
fn profiled_non_empty_small_valid_below_min_uses_the_minimum() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, NonEmpty>(SizeProfile::small_valid(0));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..16 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled NonEmpty strategy must produce a value tree")
            .current();
        assert_eq!(value.as_inner().chars().count(), 1);
    }
}

#[test]
fn profiled_each_char_small_valid_clamps_and_preserves_predicate() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy =
        profiled_refined::<String, EachChar<AsciiAlphanumeric>>(SizeProfile::small_valid(3));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled EachChar strategy must produce a value tree")
            .current();
        assert!(value.as_inner().chars().count() <= 3);
        assert!(
            value
                .as_inner()
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric())
        );
    }
}

#[test]
fn profiled_first_char_zero_cap_emits_empty_string() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, FirstChar<IdentStart>>(SizeProfile::small_valid(0));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..16 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled FirstChar strategy must produce a value tree")
            .current();
        assert!(value.as_inner().is_empty());
    }
}

#[test]
fn profiled_first_char_one_cap_emits_empty_or_valid_head() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = profiled_refined::<String, FirstChar<IdentStart>>(SizeProfile::small_valid(1));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("profiled FirstChar strategy must produce a value tree")
            .current();
        assert!(value.as_inner().chars().count() <= 1);
        if let Some(head) = value.as_inner().chars().next() {
            assert!(IdentStart::test(head));
        }
    }
}

#[test]
fn profiled_numeric_default_strategy_stays_admissible() {
    proptest!(|(r in profiled_refined::<i32, Within<0, 100>>(SizeProfile::small_valid(4)))| {
        assert!((0..=100).contains(r.as_inner()));
    });
}

#[test]
fn profiled_composition_strategies_preserve_admissibility() {
    proptest!(|(
        all in profiled_refined::<i32, All<(Within<0, 100>, AtLeast<10>, AtMost<90>)>>(
            SizeProfile::FULL_BOUNDARY,
        ),
        any in profiled_refined::<i32, Any<(LessThan<0>, EqualTo<0>, GreaterThan<100>)>>(
            SizeProfile::small_valid(4),
        ),
        or_value in profiled_refined::<i32, Or<LessThan<0>, GreaterThan<100>>>(
            SizeProfile::small_valid(4),
        ),
        xor_value in profiled_refined::<i32, Xor<Within<0, 10>, Within<5, 15>>>(
            SizeProfile::small_valid(4),
        ),
        mapped in profiled_refined::<i32, MapErr<Within<0, 100>, IdentityMap>>(
            SizeProfile::small_valid(4),
        ),
    )| {
        assert!((10..=90).contains(all.as_inner()));
        assert!([*any.as_inner() < 0, *any.as_inner() == 0, *any.as_inner() > 100]
            .into_iter()
            .any(core::convert::identity));
        assert!(*or_value.as_inner() < 0 || *or_value.as_inner() > 100);
        assert!((0..=10).contains(xor_value.as_inner()) ^ (5..=15).contains(xor_value.as_inner()));
        assert!((0..=100).contains(mapped.as_inner()));
    });
}

#[cfg(feature = "unicode")]
#[test]
fn profiled_bounded_printable_aliases_preserve_predicates() {
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let line_strategy = profiled_refined::<String, BoundedLine<32>>(SizeProfile::small_valid(4));
    let text_strategy = profiled_refined::<String, BoundedText<32>>(SizeProfile::small_valid(4));
    let mut runner = proptest::test_runner::TestRunner::deterministic();

    for _ in 0_u32..64 {
        let line = line_strategy
            .new_tree(&mut runner)
            .expect("profiled BoundedLine strategy must produce a value tree")
            .current();
        let text = text_strategy
            .new_tree(&mut runner)
            .expect("profiled BoundedText strategy must produce a value tree")
            .current();

        assert!((1..=4).contains(&line.as_inner().chars().count()));
        assert!(line.as_inner().chars().all(PrintableLine::test));
        assert!((1..=4).contains(&text.as_inner().chars().count()));
        assert!(text.as_inner().chars().all(PrintableMultiline::test));
    }
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
fn refinement_newtype_arbitrary_reaches_inner_rule_boundaries() {
    // `Percent` is a transparent `prop_map(Self)` over the inner
    // `Refined<i32, Within<0, 100>>` strategy. `prop_map(Self)` is
    // bijective over the carrier, so the newtype reaches every value
    // the inner strategy reaches; this deterministic sample pins
    // both endpoints directly.

    use proptest::strategy::{Strategy as _, ValueTree as _};

    let strategy = proptest::arbitrary::any::<Percent>();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut saw_min = false;
    let mut saw_max = false;

    for _ in 0_u32..256 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("Percent strategy must produce a value tree")
            .current();
        saw_min |= *value.as_inner() == 0;
        saw_max |= *value.as_inner() == 100;
    }

    assert!(saw_min, "newtype Arbitrary must reach the lower bound");
    assert!(saw_max, "newtype Arbitrary must reach the upper bound");
}

#[test]
fn public_rule_families_have_arbitrary_rule_impls() {
    // Compile-time coverage audit for the public facade: every
    // library rule family intended for rule-derived generation must
    // satisfy `ArbitraryRule` under its feature gate. Aliases are
    // checked by their public spelling so inherited impls stay pinned.

    // Numeric primitives and aliases.
    assert_rule::<i32, Within<0, 100>>();
    assert_rule::<i32, AtLeast<0>>();
    assert_rule::<i32, AtMost<100>>();
    assert_rule::<i32, GreaterThan<0>>();
    assert_rule::<i32, LessThan<100>>();
    assert_rule::<i32, EqualTo<42>>();
    assert_rule::<i32, NotEqualTo<0>>();
    assert_rule::<i32, NonZero>();
    assert_rule::<i32, Positive>();
    assert_rule::<i32, Negative>();

    // Float primitives.
    assert_rule::<f64, NotNan>();
    assert_rule::<f64, NotInfinite>();
    assert_rule::<f64, Finite>();
    assert_rule::<f64, InClosedRange<0, 1, 10, 1>>();

    // String-length, character, and hex primitives.
    assert_rule::<String, LenChars<1, 8>>();
    assert_rule::<String, LenBytes<1, 8>>();
    assert_rule::<String, NonEmpty>();
    assert_rule::<String, EachChar<AsciiAlphanumeric>>();
    assert_rule::<String, FirstChar<IdentStart>>();
    assert_rule::<String, HexFixedLower<4>>();
    assert_rule::<String, HexFixedAny<4>>();
    assert_rule::<String, HexFixedNormalized<4>>();
    assert_rule::<String, RelativePath>();

    // Character predicates used by `EachChar` and `FirstChar`.
    assert_char::<CharLiteral<'x'>>();
    assert_char::<CharEither<CharLiteral<'x'>, CharLiteral<'-'>>>();
    assert_char::<CharExcept<AsciiGraphic, CharLiteral<'/'>>>();
    assert_char::<AsciiGraphic>();
    assert_char::<AsciiAlphanumeric>();
    assert_char::<AsciiAlphabetic>();
    assert_char::<AsciiUppercase>();
    assert_char::<AsciiLowercaseChar>();
    assert_char::<AsciiDigit>();
    assert_char::<IdentChar>();
    assert_char::<IdentStart>();
    assert_char::<NonControl>();
    assert_char::<HexChar>();
    assert_char::<IdentDashChar>();

    // Collection primitives and aliases.
    assert_rule::<Vec<i32>, LenItems<1, 5>>();
    assert_rule::<Vec<i32>, AllItems<Within<0, 100>>>();
    assert_rule::<Vec<i32>, UniqueByKey<i32, whittle::primitive::IdentityKey<i32>>>();
    assert_rule::<Vec<i32>, Distinct<i32>>();
    assert_rule::<Vec<i32>, Sorted<i32, whittle::primitive::IdentityKey<i32>>>();
    assert_rule::<Vec<i32>, NoneOf<IsZero>>();
    assert_rule::<Vec<i32>, AnyOf<IsZero>>();

    // Composition rules.
    assert_rule::<i32, And<AtLeast<0>, AtMost<100>>>();
    assert_rule::<i32, Or<LessThan<0>, GreaterThan<100>>>();
    assert_rule::<i32, Not<EqualTo<0>>>();
    assert_rule::<i32, Xor<Within<0, 10>, Within<5, 15>>>();
    assert_rule::<i32, MapErr<Within<0, 100>, IdentityMap>>();
    assert_rule::<i32, All<(Within<0, 100>, AtLeast<10>, AtMost<90>)>>();
    assert_rule::<i32, Any<(LessThan<0>, EqualTo<0>, GreaterThan<100>)>>();

    // Transformers.
    assert_rule::<String, LowercaseTransform<HexFixedAny<4>>>();
    assert_rule::<String, UppercaseTransform<EachChar<AsciiAlphanumeric>>>();
    assert_rule::<String, Trim<EachChar<AsciiAlphanumeric>>>();
    assert_chrono_rule_families();
    assert_decimal_rule_families();
    assert_regex_rule_families();
    assert_unicode_rule_families();
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

    proptest!(|(r in proptest::arbitrary::any::<Refined<String, LowercaseTransform<HexFixedAny<2>>>>())| {
        assert_eq!(r.as_inner(), &r.as_inner().to_ascii_lowercase());
    });
}
