//! The `f: A → B` property harness (`whittle::testing`).
//!
//! DOGFOODING §2.5 obligation 2 as a one-liner per function:
//! generate valid inputs via `Arbitrary<Refined<T, R>>`, assert `f`
//! is total ([`prop_total`]) and — only when the return type does
//! not already carry the invariant — that the image satisfies a
//! stated output rule ([`prop_image_refines`]).

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::primitive::{LenChars, NumericError, Within};
use whittle::testing::{prop_image_refines, prop_total};
use whittle::{Refined, Rule};

/// A booked-seat count for a 100-seat cabin.
type BookedSeats = Refined<u8, Within<0, 100>>;

/// `f: BookedSeats → u8` with a raw output type. The signature does
/// not carry the output invariant (`0..=100`), so obligation 2 has
/// both halves: totality AND image-validity against a stated rule.
fn seats_remaining(booked: BookedSeats) -> u8 {
    100 - booked.as_inner()
}

#[test]
fn seats_remaining_is_total() {
    // ─── Totality: no admissible booking count panics. ──────────
    //
    // `100 - booked` cannot underflow because the domain rule
    // bounds `booked` at 100 — the property documents exactly that
    // reliance on the input rule.
    prop_total(seats_remaining);
}

#[test]
fn seats_remaining_image_refines_seat_range() {
    // ─── Image-validity: every output lies in `0..=100`. ────────
    //
    // The output is a raw `u8`, so the type proves only `0..=255`;
    // the residual `0..=100` claim is the harness's job. On failure
    // the panic names the rule, shows the exact rejection error,
    // and proptest reports the minimal offending input.
    prop_image_refines::<Within<0, 100>, _, _, _>(seats_remaining);
}

/// A passenger display name: 1..=8 chars.
type DisplayName = Refined<String, LenChars<1, 8>>;

/// `f: DisplayName → DisplayName` returns a *refined* `B`: the
/// output invariant is carried by the type, so image-validity is
/// discharged by construction ("delete the test the type proves").
fn initial(name: DisplayName) -> DisplayName {
    let head: String = name.into_inner().chars().take(1).collect();
    Refined::try_new(head).expect("one char of a non-empty name is within 1..=8 chars")
}

#[test]
fn initial_is_total() {
    // ─── Refined return type: totality is the ONLY residual. ────
    //
    // No `prop_image_refines` companion: `DisplayName` cannot exist
    // without `LenChars<1, 8>` having accepted it, so an image test
    // would re-prove what the type already carries. The interesting
    // residual is the `expect` inside `initial` — totality covers
    // it across the whole admitted input set.
    prop_total(initial);
}

#[test]
fn rule_membership_check_matches_harness_semantics() {
    // ─── R-D8: membership via `refine` — the harness's image check
    //     is exactly this predicate. Pin the semantics with one
    //     accept and one reject witness (exact variants).
    assert_eq!(<Within<0, 100> as Rule<u8>>::refine(100_u8).unwrap(), 100);
    assert_eq!(
        <Within<0, 100> as Rule<u8>>::refine(101_u8).unwrap_err(),
        NumericError::OutOfRange { value: 101 },
    );
}
