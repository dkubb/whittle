//! Property harness for the `f: A → B` methodology.
//!
//! `docs/DOGFOODING.md` §2.5 states the testing north star as a
//! triangle of three obligations around a function `f: A → B` whose
//! domain is a whittle smart-ctor type:
//!
//! 1. **Constructor faithfulness (input boundary).** `A`'s smart
//!    constructor accepts *exactly* its admitted set `S(A)` —
//!    boundary-matrix accept/reject, tested on the constructor, not
//!    on `f`.
//! 2. **Totality + image-validity (the PBT on `f`).** Generate valid
//!    `A` via `Arbitrary`; assert `f` never panics (totality) and
//!    `f(a) ∈ S(B)` (image-validity). *This module discharges
//!    obligation 2 as a one-liner per function:* [`prop_total`]
//!    checks totality; [`prop_image_refines`] additionally checks
//!    the image against a stated output rule.
//! 3. **Generator completeness.** `Arbitrary<A>` must be surjective
//!    onto `S(A)` (boundary-complete at minimum) or obligation 2
//!    passes vacuously on a sub-domain. This is a property of
//!    whittle's generators, separate from `f`.
//!
//! **Delete the test the type proves.** When `f` already returns a
//! refined `B`, image-validity is discharged by the return type —
//! the value cannot exist without `R_B::refine` having accepted it.
//! In that case use [`prop_total`] *only*; reaching for
//! [`prop_image_refines`] would re-test what the type system
//! already proves (least power applied to tests). The best
//! image-validity test is the one you delete because `f` returns a
//! refined `B`.
//!
//! # Design: closure-taking functions, not strategy combinators
//!
//! Each harness entry point is a plain function that takes `f` and
//! runs a [`proptest::test_runner::TestRunner`] internally, rather
//! than a strategy-returning combinator for use inside `proptest!`.
//! Two reasons:
//!
//! - The existing corpus (`tests/proptest-arbitrary.rs`) writes one
//!   named `#[test]` per property. A closure-taking function keeps
//!   that shape — `#[test] fn f_is_total() { prop_total(f); }` —
//!   with the obligation named at the call site.
//! - The whole point of obligation 2 is that `Arbitrary<A>` already
//!   determines the input set (one determinant). A combinator used
//!   inside `proptest!` would force the caller to restate the input
//!   strategy at the test site, re-introducing the second
//!   determinant the harness exists to remove.
//!
//! Failure persistence is disabled: the harness cannot know its
//! caller's source file, so writing `proptest-regressions/` files
//! into an unpredictable location would be worse than none. The
//! input set is fully determined by `Arbitrary<A>`, and proptest's
//! shrinker re-derives a minimal counterexample on each run.
//!
//! Available behind the `proptest` feature.

use crate::rule::Rule;
use alloc::format;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

/// Run `test` against every generated `A`, panicking with the
/// minimal failing input on property failure.
#[expect(
    clippy::panic,
    reason = "a failed property must fail the enclosing #[test]; panicking with \
              proptest's own failure rendering (minimal input + seed) is the \
              same surface the proptest! macro provides"
)]
fn run_cases<A, F>(test: F)
where
    A: proptest::arbitrary::Arbitrary,
    F: Fn(A) -> Result<(), TestCaseError>,
{
    let config = Config {
        // No source file is known here (the harness is a library
        // function), so regression files would land in an
        // unpredictable location. See the module docs.
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner = TestRunner::new(config);
    if let Err(err) = runner.run(&proptest::arbitrary::any::<A>(), test) {
        panic!("{err}\n{runner}");
    }
}

/// Property: `f` is total over `S(A)` — no generated input panics.
///
/// DOGFOODING §2.5 obligation 2, totality half. Inputs are drawn
/// from `A`'s [`Arbitrary`](proptest::arbitrary::Arbitrary) impl;
/// for `A = Refined<T, R>` that is the rule-derived strategy, so
/// every sample is admissible by construction. Any panic inside `f`
/// fails the property (proptest catches the unwind, shrinks, and
/// this harness re-panics with the minimal failing input).
///
/// When `f` returns a refined `B`, this is the *only* harness call
/// needed: image-validity is discharged by the return type. Reach
/// for [`prop_image_refines`] only when `f` returns a raw type whose
/// admissible subset the signature does not carry.
///
/// # Panics
///
/// Panics when `f` panics for some generated input; the message
/// carries proptest's rendering of the minimal failing input.
///
/// # Examples
///
/// A total function over its refined domain passes:
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::Within;
/// use whittle_core::testing::prop_total;
///
/// /// `f: Refined<u8, Within<0, 50>> → u8`. Total: doubling any
/// /// admissible input stays within `u8`.
/// fn double(half_percent: Refined<u8, Within<0, 50>>) -> u8 {
///     half_percent.as_inner() * 2
/// }
///
/// prop_total(double);
/// ```
///
/// A function that panics on an admissible input fails:
///
/// ```should_panic
/// use whittle_core::Refined;
/// use whittle_core::primitive::Within;
/// use whittle_core::testing::prop_total;
///
/// /// Deliberately partial: the domain admits 1, the body does not.
/// fn buggy(bit: Refined<u8, Within<0, 1>>) -> u8 {
///     assert!(*bit.as_inner() < 1, "off-by-one: rejects the MAX bound");
///     *bit.as_inner()
/// }
///
/// prop_total(buggy); // panics with the minimal failing input (1)
/// ```
pub fn prop_total<A, B, F>(f: F)
where
    A: proptest::arbitrary::Arbitrary,
    F: Fn(A) -> B,
{
    run_cases::<A, _>(move |input| {
        drop(f(input));
        Ok(())
    });
}

/// Property: `f` is total over `S(A)` *and* every output lies in
/// `S(B)` as declared by rule `RB` — membership checked via
/// `RB::refine(output).is_ok()`.
///
/// DOGFOODING §2.5 obligation 2, both halves, for functions whose
/// return type does not already carry the output invariant.
/// Membership-as-bool is the semantics here (R-D8): the refined
/// output is discarded, only admission is asserted. On failure the
/// panic message carries the exact rejection error, and proptest's
/// rendering carries the minimal offending input.
///
/// If you can tighten `f` to return `Refined<B, RB>` instead, do
/// that and delete this call — use [`prop_total`] only ("delete the
/// test the type proves").
///
/// # Panics
///
/// Panics when `f` panics for some generated input, or when some
/// output is rejected by `RB`; the message names `RB`, shows the
/// rejection error, and carries proptest's rendering of the minimal
/// failing input.
///
/// # Examples
///
/// Every output of `halve` satisfies `Within<0, 50>`:
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::Within;
/// use whittle_core::testing::prop_image_refines;
///
/// /// `f: Refined<u8, Within<0, 100>> → u8`, image `0..=50`.
/// fn halve(percent: Refined<u8, Within<0, 100>>) -> u8 {
///     percent.as_inner() / 2
/// }
///
/// prop_image_refines::<Within<0, 50>, _, _, _>(halve);
/// ```
///
/// An output escaping the stated rule fails, reporting the input
/// and the exact rejection:
///
/// ```should_panic
/// use whittle_core::Refined;
/// use whittle_core::primitive::Within;
/// use whittle_core::testing::prop_image_refines;
///
/// /// Claims image `0..=10` but maps the admissible input 1 to 11.
/// fn leaky(bit: Refined<u8, Within<0, 1>>) -> u8 {
///     bit.as_inner() + 10
/// }
///
/// prop_image_refines::<Within<0, 10>, _, _, _>(leaky);
/// ```
pub fn prop_image_refines<RB, A, B, F>(f: F)
where
    RB: Rule<B>,
    RB::Error: core::fmt::Debug,
    A: proptest::arbitrary::Arbitrary,
    B: 'static,
    F: Fn(A) -> B,
{
    run_cases::<A, _>(move |input| match RB::refine(f(input)) {
        Ok(_admitted) => Ok(()),
        Err(rejection) => Err(TestCaseError::fail(format!(
            "image violates {}: {rejection:?}",
            core::any::type_name::<RB>(),
        ))),
    });
}

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "explicit in test code: the deliberately-partial fixture panics"
)]
mod tests {
    use super::{prop_image_refines, prop_total};
    use crate::primitive::Within;
    use crate::rule::Refined;

    // The pass/fail test pairs below hand the harness `fn` POINTERS
    // (`f as fn(A) -> B`) rather than closures. Each closure
    // expression has a unique type, so a closure-fed pass test and
    // fail test would monomorphise `run_cases` twice — and each copy
    // would cover only one side of its pass/fail branch, which the
    // per-instantiation-group coverage summary reports as a missed
    // region. A shared `fn`-pointer type funnels both tests through
    // ONE instantiation that covers both sides.

    /// `Within<0, 1>` carrier: two admissible values, both sampled
    /// with overwhelming probability in a default 256-case run.
    type Bit = Refined<u8, Within<0, 1>>;

    fn total_over_bits(input: Bit) -> u8 {
        *input.as_inner()
    }

    fn panics_on_any_bit(_input: Bit) -> u8 {
        panic!("deliberately partial")
    }

    /// Totality holds: the unwrapping identity is total.
    #[test]
    fn prop_total_passes_for_total_function() {
        prop_total(total_over_bits as fn(Bit) -> u8);
    }

    /// Totality fails: the unconditional panic fires on the first
    /// case — deterministic, no reliance on sampling luck.
    #[test]
    #[should_panic(expected = "deliberately partial")]
    fn prop_total_panics_when_function_panics() {
        prop_total(panics_on_any_bit as fn(Bit) -> u8);
    }

    /// `Within<0, 0>` carrier: exactly one admissible value, so the
    /// constant-output fixtures below are exercised on every case.
    type Zero = Refined<u8, Within<0, 0>>;

    fn admissible_constant(_input: Zero) -> u8 {
        10
    }

    fn inadmissible_constant(_input: Zero) -> u8 {
        11
    }

    /// Image-validity holds: the constant 10 satisfies the stated
    /// output rule `Within<0, 10>` (at its MAX bound).
    #[test]
    fn prop_image_refines_passes_when_image_is_admissible() {
        prop_image_refines::<Within<0, 10>, _, _, _>(admissible_constant as fn(Zero) -> u8);
    }

    /// Image-validity fails: the constant 11 escapes `Within<0,
    /// 10>` on the first case, and the panic message names the rule
    /// and carries the exact rejection error.
    #[test]
    #[should_panic(expected = "image violates")]
    fn prop_image_refines_panics_on_inadmissible_output() {
        prop_image_refines::<Within<0, 10>, _, _, _>(inadmissible_constant as fn(Zero) -> u8);
    }
}
