// Examples are interactive demonstrations: they use `println!` to
// confirm what was demonstrated and `unwrap()` to keep the focus on
// the API, not error plumbing. The workspace lints would otherwise
// deny both.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    clippy::items_after_statements
)]

//! Hex hashes: strict, permissive, normalized.
//!
//! Three rules over the same shape (`40` hex chars — the SHA-1
//! length) demonstrate the spectrum of acceptance:
//!
//! - `HexFixedLower<40>`: strict. Only lowercase admitted.
//! - `HexFixedAny<40>`: permissive. Either case admitted; input
//!   is stored verbatim.
//! - `HexFixedNormalized<40>`: canonical. Either case admitted;
//!   input is lowercased before storage.
//!
//! This is the canonical "what is a transformer" example.
//! Transformers (`AsciiLowercase<R>`, `AsciiUppercase<R>`,
//! `Trim<R>`) rewrite the input *before* the inner rule runs, so
//! the stored carrier is the canonical form, not the input.
//!
//! Picking between the three: pick the strict rule for inputs you
//! want to reject if they aren't already canonical, the
//! permissive rule for round-trip-faithful storage of
//! user-provided values, and the normalized rule when you want
//! one canonical form regardless of input case.

use whittle::primitive::{HexFixedAny, HexFixedLower, HexFixedNormalized};
use whittle::Refined;

fn main() {
    // 40-char mixed-case hash. The SHA-1 of an empty string,
    // uppercased — a real-shape sample.
    let mixed = "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709";

    // 1. Strict: lowercase only. Mixed-case is rejected.
    let strict_err = Refined::<String, HexFixedLower<40>>::try_new(mixed.to_string());
    assert!(strict_err.is_err(), "uppercase rejected by strict rule");

    // The strict rule admits an all-lowercase input verbatim.
    let lower_in = mixed.to_ascii_lowercase();
    let strict_ok: Refined<String, HexFixedLower<40>> =
        Refined::try_new(lower_in.clone()).unwrap();
    assert_eq!(strict_ok.as_inner(), &lower_in);

    // 2. Permissive: mixed-case is admitted; storage is verbatim.
    let any_ok: Refined<String, HexFixedAny<40>> = Refined::try_new(mixed.to_string()).unwrap();
    assert_eq!(any_ok.as_inner(), mixed); // stored as given

    // 3. Normalized: mixed-case is admitted *and* canonicalized
    // to lowercase. The stored carrier is the canonical form.
    let normalized: Refined<String, HexFixedNormalized<40>> =
        Refined::try_new(mixed.to_string()).unwrap();
    assert_eq!(normalized.as_inner(), &lower_in);
    assert_ne!(normalized.as_inner(), mixed);

    // Idempotent: feeding the canonical form back through produces
    // the same value.
    let again: Refined<String, HexFixedNormalized<40>> =
        Refined::try_new(lower_in.clone()).unwrap();
    assert_eq!(again.as_inner(), normalized.as_inner());

    println!("strict in:  {lower_in}");
    println!("any in:     {mixed}");
    println!("normalized: {}", normalized.as_inner());
    println!("OK: hex rules — strict rejects, any preserves, normalized canonicalizes");
}
