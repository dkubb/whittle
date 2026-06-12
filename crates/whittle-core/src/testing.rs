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

use crate::closed_set::{self, ClosedSet};
use crate::rule::Rule;
use crate::schema::{Scalar, ScalarKind, Schema, SchemaRule};
use alloc::format;
use alloc::vec::Vec;
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

// ─── Schema cross-checks (IDEA §5.11). ─────────────────────────────
//
// `SchemaRule` introduces a second description of a rule's admitted
// set; these helpers are the mechanical oracle keeping the two
// determinants consistent until derived generation replaces the
// hand-written strategies. Violations are collected and reported
// together through one non-generic finisher, so a single
// deliberately-inconsistent fixture exercises every check.

/// Number of strategy samples each cross-check draws. Deterministic
/// runner, so the sample set is stable across runs.
const CROSS_CHECK_SAMPLES: u32 = 256;

/// Panic with the collected cross-check violations, deduplicated.
/// Non-generic on purpose: the panic is shared by every helper
/// instantiation, so pass and fail paths merge into one function.
fn finish_cross_check(subject: &str, mut violations: Vec<alloc::string::String>) {
    violations.sort_unstable();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "schema cross-check failed for {subject}:\n{}",
        violations.join("\n"),
    );
}

/// One row of a derived scalar boundary matrix: the schema's test
/// point plus the carrier-level outcome the generic collector
/// attached. Non-generic so every rule family shares one violation
/// renderer ([`scalar_matrix_violations`]).
struct ScalarMatrixRow {
    /// The schema-classified test point.
    boundary: crate::schema::ScalarBoundary,
    /// `None` when the carrier cannot embed the point losslessly:
    /// the candidate is skipped (see the float-precision note on
    /// [`Schema::scalar_boundaries`]).
    outcome: Option<ScalarMatrixOutcome>,
}

/// The carrier-level observation for one testable matrix row.
struct ScalarMatrixOutcome {
    /// Debug rendering of the carrier value, for violation reports.
    rendered: alloc::string::String,
    /// Whether `R::refine` admitted the value.
    refine_admits: bool,
    /// Whether `embed` maps the value back to the row's schema point
    /// — the lossless-embedding contract on `try_extract`.
    embeds_back: bool,
}

/// Derive the boundary matrix from `R::schema()` and attach each
/// testable row's `refine` verdict. Generic but branch-light: all
/// reporting decisions live in the non-generic
/// [`scalar_matrix_violations`].
fn collect_scalar_matrix<T, R>(
    embed: fn(&T) -> (ScalarKind, Scalar),
    try_extract: fn(ScalarKind, Scalar) -> Option<T>,
) -> Vec<ScalarMatrixRow>
where
    T: core::fmt::Debug + 'static,
    R: SchemaRule<T>,
{
    R::schema()
        .scalar_boundaries()
        .into_iter()
        .map(|boundary| {
            let outcome = try_extract(boundary.kind, boundary.value).map(|value| {
                let rendered = format!("{value:?}");
                let embeds_back = embed(&value) == (boundary.kind, boundary.value);
                ScalarMatrixOutcome {
                    rendered,
                    refine_admits: R::refine(value).is_ok(),
                    embeds_back,
                }
            });
            ScalarMatrixRow { boundary, outcome }
        })
        .collect()
}

/// Render a matrix's violations. Non-generic: one function serves
/// every rule family, so pass, skip, and both disagreement
/// directions merge into a single coverage surface.
fn scalar_matrix_violations(rows: &[ScalarMatrixRow]) -> Vec<alloc::string::String> {
    let mut violations: Vec<alloc::string::String> = Vec::new();
    let mut tested = 0_usize;
    for row in rows {
        let Some(outcome) = &row.outcome else {
            // Skipped: the carrier cannot represent the point.
            continue;
        };
        tested += 1;
        if !outcome.embeds_back {
            violations.push(format!(
                "boundary value {} does not embed back to its schema point \
                 ({:?}, {:?}): try_extract must return None for points the \
                 carrier cannot represent losslessly",
                outcome.rendered, row.boundary.kind, row.boundary.value,
            ));
        }
        match (row.boundary.admitted, outcome.refine_admits) {
            (true, false) => violations.push(format!(
                "schema admits {} at the boundary but refine rejects it: the \
                 schema overclaims the admitted set",
                outcome.rendered,
            )),
            (false, true) => violations.push(format!(
                "schema rejects {} at the boundary but refine admits it: the \
                 schema underclaims the admitted set",
                outcome.rendered,
            )),
            (true, true) | (false, false) => {}
        }
    }
    if tested == 0 {
        violations.push(alloc::string::String::from(
            "the boundary matrix is vacuous: no scalar boundary candidate was \
             testable (no finite interval endpoints, membership undecidable, \
             or every candidate outside the carrier)",
        ));
    }
    violations
}

/// Assert the schema-derived R-T1 boundary matrix against `refine`:
/// accept-at-boundary and reject-just-outside, with both the test
/// points and the expected verdicts read off `R::schema()`.
///
/// For every finite interval endpoint of the schema, the matrix
/// tests the endpoint and its adjacent representable neighbours
/// (`MIN−1`/`MIN`/`MIN+1`, `MAX−1`/`MAX`/`MAX+1`; floats step by one
/// `f64` ULP — see [`Schema::scalar_boundaries`]), asserting that
/// `R::refine` agrees with the schema's own membership verdict on
/// each point.
///
/// This is the helper whose absence deferred R-T1: without schema
/// reflection a matrix generator had to restate MIN/MAX at the test
/// site — a second determinant for the same bound. The schema now
/// carries the bounds, so the matrix is a fold and the test site
/// states nothing. The helper asserts accept/reject *placement*
/// only; pinning the exact error variant of a reject stays the
/// caller's line (one hand-written exact-variant reject test per
/// family keeps the error contract visible).
///
/// `embed` is the carrier's embedding into the scalar universe;
/// `try_extract` is its partial inverse and MUST return `None` for
/// any point the carrier cannot represent *losslessly* (an `f32`
/// carrier offered an `f64`-ULP neighbour, a `u8` carrier offered
/// `-1`); such candidates are skipped. The matrix must not be
/// vacuous: at least one candidate has to be testable.
///
/// # Panics
///
/// Panics when `refine` disagrees with the schema's verdict on a
/// boundary point (in either direction), when an extracted value
/// does not embed back to its schema point, or when no candidate
/// was testable.
///
/// # Examples
///
/// ```
/// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};
/// use whittle_core::testing::assert_schema_boundary_matrix;
/// use whittle_core::Rule;
///
/// /// Admits `0..=100`.
/// enum Percent {}
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct OutOfRange;
///
/// impl Rule<i32> for Percent {
///     type Error = OutOfRange;
///     fn refine(raw: i32) -> Result<i32, Self::Error> {
///         if (0..=100).contains(&raw) { Ok(raw) } else { Err(OutOfRange) }
///     }
/// }
///
/// impl SchemaRule<i32> for Percent {
///     fn schema() -> Schema {
///         Schema::interval(
///             ScalarKind::Integer,
///             Bound::Inclusive(Scalar::Int(0)),
///             Bound::Inclusive(Scalar::Int(100)),
///         )
///     }
/// }
///
/// // -1, 0, 1, 99, 100, 101 — derived, classified, and checked
/// // against refine, with nothing restated at the test site.
/// assert_schema_boundary_matrix::<i32, Percent>(
///     |value| (ScalarKind::Integer, Scalar::Int(i128::from(*value))),
///     |_kind, scalar| i32::try_from(scalar.as_int().expect("integer schema")).ok(),
/// );
/// ```
pub fn assert_schema_boundary_matrix<T, R>(
    embed: fn(&T) -> (ScalarKind, Scalar),
    try_extract: fn(ScalarKind, Scalar) -> Option<T>,
) where
    T: core::fmt::Debug + 'static,
    R: SchemaRule<T>,
{
    let rows = collect_scalar_matrix::<T, R>(embed, try_extract);
    finish_cross_check(core::any::type_name::<R>(), scalar_matrix_violations(&rows));
}

/// Cross-check a rule's [`SchemaRule`] schema against its `refine`
/// and its hand-written [`ArbitraryRule`](crate::ArbitraryRule)
/// strategy:
///
/// 1. **The boundary matrix agrees with `refine`.** Every testable
///    point of the schema-derived boundary matrix (endpoints and
///    their neighbours, accept AND reject side) must get the same
///    verdict from `R::refine` — exactly
///    [`assert_schema_boundary_matrix`]'s obligation.
/// 2. **Strategy samples are schema members.** Every value the
///    hand-written strategy emits — embedded into the scalar
///    universe through `embed` — must be a member of `⟦schema⟧`
///    under [`Schema::scalar_membership`].
///
/// `embed` is the carrier's embedding into the schema's scalar
/// universe (`i32` → `(Integer, Int(widened))`, `f64` →
/// `(Float, Float(value))`, `NaiveDate` →
/// `(Date, Int(days from CE))`, …); `try_extract` is its partial
/// inverse, `None` for points the carrier cannot represent
/// losslessly (those candidates are skipped).
///
/// Violations are collected and reported together, so one run
/// surfaces every disagreement between the two determinants.
///
/// # Panics
///
/// Panics when `refine` disagrees with the schema on a boundary
/// point, when an extracted boundary value does not embed back to
/// its schema point, when the matrix is vacuous, or when a strategy
/// sample falls outside `⟦schema⟧` — each a violation of the
/// [`SchemaRule`] soundness obligation, named in the message.
///
/// # Examples
///
/// ```
/// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};
/// use whittle_core::testing::prop_schema_cross_check;
/// use whittle_core::{ArbitraryRule, Rule};
///
/// /// Admits `0..=100`.
/// enum Percent {}
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct OutOfRange;
///
/// impl Rule<i32> for Percent {
///     type Error = OutOfRange;
///     fn refine(raw: i32) -> Result<i32, Self::Error> {
///         if (0..=100).contains(&raw) { Ok(raw) } else { Err(OutOfRange) }
///     }
/// }
///
/// impl SchemaRule<i32> for Percent {
///     fn schema() -> Schema {
///         Schema::interval(
///             ScalarKind::Integer,
///             Bound::Inclusive(Scalar::Int(0)),
///             Bound::Inclusive(Scalar::Int(100)),
///         )
///     }
/// }
///
/// impl ArbitraryRule<i32> for Percent {
///     type Strategy = core::ops::RangeInclusive<i32>;
///     fn arbitrary_strategy() -> Self::Strategy {
///         0..=100
///     }
/// }
///
/// prop_schema_cross_check::<i32, Percent>(
///     |value| (ScalarKind::Integer, Scalar::Int(i128::from(*value))),
///     |_kind, scalar| i32::try_from(scalar.as_int().expect("integer schema")).ok(),
/// );
/// ```
pub fn prop_schema_cross_check<T, R>(
    embed: fn(&T) -> (ScalarKind, Scalar),
    try_extract: fn(ScalarKind, Scalar) -> Option<T>,
) where
    T: core::fmt::Debug + 'static,
    R: SchemaRule<T> + crate::ArbitraryRule<T>,
{
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let schema = R::schema();

    // (1) The schema-derived boundary matrix agrees with refine.
    let rows = collect_scalar_matrix::<T, R>(embed, try_extract);
    let mut violations = scalar_matrix_violations(&rows);

    // (2) Strategy samples are ⟦schema⟧ members.
    let strategy = R::arbitrary_strategy();
    let mut runner = TestRunner::deterministic();
    for _ in 0..CROSS_CHECK_SAMPLES {
        let sample = strategy
            .new_tree(&mut runner)
            .expect("strategy must produce a value tree")
            .current();
        let (kind, scalar) = embed(&sample);
        if schema.scalar_membership(kind, &scalar) != Some(true) {
            violations.push(format!(
                "ArbitraryRule sample {sample:?} is not a ⟦schema⟧ member: the \
                 hand-written strategy and the schema disagree",
            ));
        }
    }

    finish_cross_check(core::any::type_name::<R>(), violations);
}

/// Cross-check a closed set's `Enumerated` schema against its
/// [`ClosedSet::MEMBERS`] table:
///
/// 1. **Labels match the table.** The schema's labels must be
///    exactly the table's wire strings, in declaration order.
/// 2. **The string boundary matrix agrees with `parse`.** Every
///    point of the schema-derived matrix
///    ([`Schema::string_boundaries`]: the labels plus their derived
///    near-misses — case-flips, truncations, extensions, the empty
///    string) must get the same verdict from [`closed_set::parse`]
///    that the schema gives it. This is R-T1 for closed sets: the
///    accept AND reject points read off one determinant.
/// 3. **Strategy samples are schema members.** Every value
///    [`closed_set::admissible`] emits must render
///    ([`closed_set::as_str`]) to one of the schema's labels.
///
/// `schema` is the value under test — pass the macro-emitted
/// `Enum::schema()` (or a hand-written equivalent). Violations are
/// collected and reported together.
///
/// # Panics
///
/// Panics when the schema is not an `Enumerated` node, when its
/// labels differ from the `MEMBERS` wire strings, when `parse`
/// disagrees with the schema's verdict on a boundary point (in
/// either direction), or when an admissible sample renders outside
/// the label set.
///
/// # Examples
///
/// ```
/// use whittle_core::closed_set;
/// use whittle_core::testing::assert_closed_set_schema;
///
/// closed_set! {
///     /// Feature toggle wire form.
///     pub enum Toggle {
///         /// Enabled.
///         On = "on",
///         /// Disabled.
///         Off = "off",
///     }
/// }
///
/// assert_closed_set_schema::<Toggle>(&Toggle::schema());
/// ```
pub fn assert_closed_set_schema<E>(schema: &Schema)
where
    E: ClosedSet + core::fmt::Debug,
{
    use proptest::strategy::{Strategy as _, ValueTree as _};

    let mut violations: Vec<alloc::string::String> = Vec::new();

    if let Some(labels) = schema.as_enumerated() {
        // (1) Labels are exactly the MEMBERS wire strings, in order.
        let wires: Vec<&'static str> = E::MEMBERS.iter().map(|member| member.0).collect();
        if labels != wires {
            violations.push(format!(
                "Enumerated labels {labels:?} must be the MEMBERS wire strings \
                 {wires:?} in declaration order",
            ));
        }

        // (2) The derived string boundary matrix agrees with parse:
        // labels parse, near-misses are rejected.
        for boundary in schema.string_boundaries() {
            let parses = closed_set::parse::<E>(&boundary.value).is_ok();
            match (boundary.admitted, parses) {
                (true, false) => violations.push(format!(
                    "schema label {:?} does not parse: not a member of the \
                     closed set",
                    boundary.value,
                )),
                (false, true) => violations.push(format!(
                    "near-miss {:?} parses but the schema rejects it: the \
                     label set under-covers the closed set",
                    boundary.value,
                )),
                (true, true) | (false, false) => {}
            }
        }

        // (3) Admissible samples render to schema labels.
        let strategy = closed_set::admissible::<E>();
        let mut runner = TestRunner::deterministic();
        for _ in 0..CROSS_CHECK_SAMPLES {
            let sample = strategy
                .new_tree(&mut runner)
                .expect("strategy must produce a value tree")
                .current();
            let wire = closed_set::as_str(sample);
            if !labels.contains(&wire) {
                violations.push(format!(
                    "admissible sample {sample:?} renders to {wire:?}, which is \
                     not a schema label",
                ));
            }
        }
    } else {
        violations.push(format!(
            "closed-set schema must be an Enumerated node, got:\n{schema}",
        ));
    }

    finish_cross_check(core::any::type_name::<E>(), violations);
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

    // ─── Schema cross-checks. ──────────────────────────────────────

    use super::{assert_closed_set_schema, assert_schema_boundary_matrix, prop_schema_cross_check};
    use crate::rule::{ArbitraryRule, Rule};
    use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

    /// Consistent fixture over `i32`: refine, schema, and strategy
    /// all describe `0..=100`.
    enum PercentI32 {}

    #[derive(Debug, PartialEq, Eq)]
    struct OutOfRange;

    impl Rule<i32> for PercentI32 {
        type Error = OutOfRange;
        fn refine(raw: i32) -> Result<i32, Self::Error> {
            if (0..=100).contains(&raw) {
                Ok(raw)
            } else {
                Err(OutOfRange)
            }
        }
    }

    impl SchemaRule<i32> for PercentI32 {
        fn schema() -> Schema {
            Schema::interval(
                ScalarKind::Integer,
                Bound::Inclusive(Scalar::Int(0)),
                Bound::Inclusive(Scalar::Int(100)),
            )
        }
    }

    impl ArbitraryRule<i32> for PercentI32 {
        type Strategy = core::ops::RangeInclusive<i32>;
        fn arbitrary_strategy() -> Self::Strategy {
            0..=100
        }
    }

    /// Consistent fixture over `u8` — the second carrier
    /// monomorphisation of the cross-check helper.
    enum HalfU8 {}

    impl Rule<u8> for HalfU8 {
        type Error = OutOfRange;
        fn refine(raw: u8) -> Result<u8, Self::Error> {
            if raw <= 50 { Ok(raw) } else { Err(OutOfRange) }
        }
    }

    impl SchemaRule<u8> for HalfU8 {
        fn schema() -> Schema {
            Schema::interval(
                ScalarKind::Integer,
                Bound::Inclusive(Scalar::Int(0)),
                Bound::Inclusive(Scalar::Int(50)),
            )
        }
    }

    impl ArbitraryRule<u8> for HalfU8 {
        type Strategy = core::ops::RangeInclusive<u8>;
        fn arbitrary_strategy() -> Self::Strategy {
            0..=50
        }
    }

    /// Fixture violating every cross-check obligation in one run —
    /// the collected-violations design lets a single instantiation
    /// exercise every branch of the collector and renderer:
    ///
    /// - `refine` admits `1..=11`, but the schema claims `[0, 10]`:
    ///   the boundary 0 is admitted-but-rejected (overclaim) and 11
    ///   is rejected-but-admitted (underclaim);
    /// - the schema's second member `[10^12, 10^12]` has boundary
    ///   points no `i32` can hold, so `try_extract` skips them;
    /// - [`embed_wild`] deliberately mis-embeds the value 9, driving
    ///   the embeds-back violation;
    /// - the strategy emits `10` (a member) and `11` (not a member —
    ///   sample-membership violation).
    enum WildlyInconsistent {}

    impl Rule<i32> for WildlyInconsistent {
        type Error = OutOfRange;
        fn refine(raw: i32) -> Result<i32, Self::Error> {
            if (1..=11).contains(&raw) {
                Ok(raw)
            } else {
                Err(OutOfRange)
            }
        }
    }

    impl SchemaRule<i32> for WildlyInconsistent {
        fn schema() -> Schema {
            Schema::union(
                [
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Inclusive(Scalar::Int(0)),
                        Bound::Inclusive(Scalar::Int(10)),
                    ),
                    Schema::interval(
                        ScalarKind::Integer,
                        Bound::Inclusive(Scalar::Int(1_000_000_000_000)),
                        Bound::Inclusive(Scalar::Int(1_000_000_000_000)),
                    ),
                ]
                .into(),
            )
        }
    }

    impl ArbitraryRule<i32> for WildlyInconsistent {
        type Strategy = core::ops::RangeInclusive<i32>;
        fn arbitrary_strategy() -> Self::Strategy {
            // Mixes a schema member (10) with a non-member (11), so
            // one run exercises both sides of the sample check.
            10..=11
        }
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
    )]
    fn embed_i32(value: &i32) -> (ScalarKind, Scalar) {
        (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
    }

    /// Deliberately wrong embedding: 9 maps to 900, every other
    /// value embeds faithfully. Drives the embeds-back violation in
    /// the same run as every other branch.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
    )]
    fn embed_wild(value: &i32) -> (ScalarKind, Scalar) {
        if *value == 9 {
            (ScalarKind::Integer, Scalar::Int(900))
        } else {
            (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
        }
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

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
    )]
    fn embed_u8(value: &u8) -> (ScalarKind, Scalar) {
        (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
    }

    #[expect(
        clippy::return_and_then,
        reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                  would add a None arm no boundary candidate reaches"
    )]
    fn extract_u8(_kind: ScalarKind, scalar: Scalar) -> Option<u8> {
        scalar
            .as_int()
            .and_then(|widened| u8::try_from(widened).ok())
    }

    /// Schema with no scalar vocabulary: the boundary matrix has
    /// nothing to test and must say so rather than pass vacuously.
    enum RegexOnly {}

    impl Rule<i32> for RegexOnly {
        type Error = OutOfRange;
        fn refine(raw: i32) -> Result<i32, Self::Error> {
            Ok(raw)
        }
    }

    impl SchemaRule<i32> for RegexOnly {
        fn schema() -> Schema {
            Schema::regex("^x$")
        }
    }

    /// Both obligations hold for consistent fixtures, across two
    /// carrier monomorphisations. `HalfU8`'s `-1` candidate also
    /// exercises the skip path (`u8` cannot represent it).
    #[test]
    fn prop_schema_cross_check_passes_for_consistent_rules() {
        prop_schema_cross_check::<i32, PercentI32>(embed_i32, extract_i32);
        prop_schema_cross_check::<u8, HalfU8>(embed_u8, extract_u8);
        // The matrix pins reject PLACEMENT; pin the exact reject
        // variant directly (the caller's line, per R-T1).
        assert_eq!(PercentI32::refine(101), Err(OutOfRange));
        assert_eq!(HalfU8::refine(51), Err(OutOfRange));
    }

    /// The standalone matrix helper passes for consistent fixtures.
    #[test]
    fn assert_schema_boundary_matrix_passes_for_consistent_rules() {
        assert_schema_boundary_matrix::<i32, PercentI32>(embed_i32, extract_i32);
        assert_schema_boundary_matrix::<u8, HalfU8>(embed_u8, extract_u8);
    }

    /// Matrix obligation, overclaim direction: the schema admits 0
    /// at the boundary, refine rejects it.
    #[test]
    #[should_panic(expected = "refine rejects it")]
    fn assert_schema_boundary_matrix_panics_when_schema_overclaims() {
        assert_schema_boundary_matrix::<i32, WildlyInconsistent>(embed_wild, extract_i32);
    }

    /// Matrix obligation, underclaim direction: the schema rejects
    /// 11 at the boundary, refine admits it.
    #[test]
    #[should_panic(expected = "refine admits it")]
    fn assert_schema_boundary_matrix_panics_when_schema_underclaims() {
        assert_schema_boundary_matrix::<i32, WildlyInconsistent>(embed_wild, extract_i32);
    }

    /// The lossless-embedding contract: an extracted boundary value
    /// must embed back to its schema point.
    #[test]
    #[should_panic(expected = "does not embed back")]
    fn assert_schema_boundary_matrix_panics_when_embedding_disagrees() {
        assert_schema_boundary_matrix::<i32, WildlyInconsistent>(embed_wild, extract_i32);
    }

    /// A matrix with nothing testable is reported, not silently
    /// passed.
    #[test]
    #[should_panic(expected = "vacuous")]
    fn assert_schema_boundary_matrix_panics_when_vacuous() {
        assert_schema_boundary_matrix::<i32, RegexOnly>(embed_i32, extract_i32);
    }

    /// The cross-check consumes the same matrix: the overclaimed
    /// boundary fires through it too.
    #[test]
    #[should_panic(expected = "refine rejects it")]
    fn prop_schema_cross_check_panics_when_schema_overclaims() {
        prop_schema_cross_check::<i32, WildlyInconsistent>(embed_wild, extract_i32);
    }

    /// Obligation (2) fires: a strategy sample outside ⟦schema⟧ means
    /// the two determinants disagree. Same fixture, same panic — the
    /// collected report carries every violation at once.
    #[test]
    #[should_panic(expected = "is not a ⟦schema⟧ member")]
    fn prop_schema_cross_check_panics_when_strategy_leaks() {
        prop_schema_cross_check::<i32, WildlyInconsistent>(embed_wild, extract_i32);
    }

    crate::closed_set! {
        /// Macro-generated fixture: the tracer bullet through the
        /// closed-set schema emission.
        pub enum TestToggle {
            /// Enabled.
            On = "on",
            /// Disabled.
            Off = "off",
        }
    }

    /// The macro-emitted schema satisfies both closed-set
    /// obligations.
    #[test]
    fn assert_closed_set_schema_passes_for_macro_emitted_schema() {
        assert_closed_set_schema::<TestToggle>(&TestToggle::schema());
    }

    /// A non-Enumerated schema is rejected up front.
    #[test]
    #[should_panic(expected = "must be an Enumerated node")]
    fn assert_closed_set_schema_panics_for_non_enumerated_schema() {
        assert_closed_set_schema::<TestToggle>(&Schema::regex("^x$"));
    }

    /// Reordered labels are a drift between the schema and the
    /// MEMBERS table.
    #[test]
    #[should_panic(expected = "in declaration order")]
    fn assert_closed_set_schema_panics_for_reordered_labels() {
        assert_closed_set_schema::<TestToggle>(&Schema::enumerated(&["off", "on"]));
    }

    /// A label outside the closed set fails the parse obligation.
    #[test]
    #[should_panic(expected = "does not parse")]
    fn assert_closed_set_schema_panics_for_unparseable_label() {
        assert_closed_set_schema::<TestToggle>(&Schema::enumerated(&["on", "bogus"]));
    }

    /// The reject side of the matrix: a derived near-miss the schema
    /// rejects must not parse. Truncating the bogus label "onx"
    /// yields "on", which the closed set accepts — drift between the
    /// schema's label set and the table.
    #[test]
    #[should_panic(expected = "parses but the schema rejects it")]
    fn assert_closed_set_schema_panics_when_a_rejected_near_miss_parses() {
        assert_closed_set_schema::<TestToggle>(&Schema::enumerated(&["onx"]));
    }

    /// A schema missing a member's wire string is caught by the
    /// admissible-sample check.
    #[test]
    #[should_panic(expected = "not a schema label")]
    fn assert_closed_set_schema_panics_for_missing_label() {
        assert_closed_set_schema::<TestToggle>(&Schema::enumerated(&["on"]));
    }
}
