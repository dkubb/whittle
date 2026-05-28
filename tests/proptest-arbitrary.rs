//! `Refined<T, R>: Arbitrary` for proptest.
//!
//! Whittle implements `Arbitrary` for every `Refined<T, R>` where
//! `T: Arbitrary` and `R: Rule<T>`. The strategy drives the inner
//! `T::arbitrary` distribution and runs the result through
//! `R::refine`, keeping only admissible values. Downstream
//! property tests never need `prop_assume!` filtering — the
//! admissibility invariant is generated, not asserted.
//!
//! The default strategy uses rejection sampling, which is fine for
//! rules whose admissible region is *dense* in `T` (such as
//! `NonZero` over `i32`: every i32 except `0` is admitted, so the
//! sampler practically never rejects). For *sparse* rules
//! (`Within<0, 100>` over the whole `i32` range admits 101 values
//! out of 2³² ≈ 4 billion), the default sampler can exhaust its
//! retry budget; route a narrower inner strategy through
//! `Refined::try_new` instead.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use proptest::proptest;
use whittle::Refined;
use whittle::primitive::{HexFixedAny, NonZero, NotNan, Within};
use whittle::transform::AsciiLowercase;

#[test]
fn dense_rule_non_zero_arbitrary_uses_rejection_sampling_without_retries() {
    // ─── Dense rule: `Refined<T, R>: Arbitrary` directly.  ──────
    //
    // `NonZero` over `i32` admits every i32 except `0` — a single
    // excluded value in a ~4-billion-value domain. The default
    // `Arbitrary` sampler can take the rejection-sampling path
    // without ever exhausting its retry budget. No workaround
    // needed.

    proptest!(|(r in proptest::arbitrary::any::<Refined<i32, NonZero>>())| {
        assert!(*r.as_inner() != 0);
    });
}

#[test]
fn dense_rule_not_nan_arbitrary_admits_every_non_nan_f64() {
    // `NotNan` over `f64` is also dense: only NaN is excluded.
    // Every other f64 (including the two infinities) is admitted,
    // so the sampler accepts nearly every generated value.

    proptest!(|(r in proptest::arbitrary::any::<Refined<f64, NotNan>>())| {
        assert!(!r.as_inner().is_nan());
    });
}

#[test]
fn sparse_rule_within_drives_narrower_strategy_through_try_new() {
    // ─── Sparse rule: drive a narrower strategy through `try_new`.
    //
    // `Within<0, 100>` over `i32` admits only 101 values out of 2³².
    // Calling `any::<Refined<i32, Within<0, 100>>>()` would force
    // proptest into rejection sampling against an extremely sparse
    // target and likely exhaust the retry budget. The workaround is
    // to drive a narrower input strategy (`0..=100`) and route each
    // candidate through `Refined::try_new`. Every generated value
    // satisfies the rule by construction, with no rejection
    // sampling involved.

    proptest!(|(x in 0_i32..=100_i32)| {
        let r: Refined<i32, Within<0, 100>> = Refined::try_new(x).unwrap();
        assert!((0..=100).contains(r.as_inner()));
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

    proptest!(|(raw in "[0-9a-fA-F]{2}")| {
        let r: Refined<String, AsciiLowercase<HexFixedAny<2>>> =
            Refined::try_new(raw).unwrap();
        assert_eq!(r.as_inner(), &r.as_inner().to_ascii_lowercase());
    });
}
