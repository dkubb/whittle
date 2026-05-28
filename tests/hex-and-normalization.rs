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

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::Refined;
use whittle::primitive::{HexFixedAny, HexFixedLower, HexFixedNormalized};

// 40-char mixed-case hash. The SHA-1 of an empty string,
// uppercased — a real-shape sample.
const MIXED: &str = "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709";

#[test]
fn hex_fixed_lower_rejects_uppercase_and_admits_lowercase_verbatim() {
    // 1. Strict: lowercase only. Mixed-case is rejected.
    let strict_err = Refined::<String, HexFixedLower<40>>::try_new(MIXED.to_string());
    assert!(strict_err.is_err(), "uppercase rejected by strict rule");

    // The strict rule admits an all-lowercase input verbatim.
    let lower_in = MIXED.to_ascii_lowercase();
    let strict_ok: Refined<String, HexFixedLower<40>> =
        Refined::try_new(lower_in.clone()).unwrap();
    assert_eq!(strict_ok.as_inner(), &lower_in);
}

#[test]
fn hex_fixed_any_admits_mixed_case_and_stores_input_verbatim() {
    // 2. Permissive: mixed-case is admitted; storage is verbatim.
    let any_ok: Refined<String, HexFixedAny<40>> = Refined::try_new(MIXED.to_string()).unwrap();
    assert_eq!(any_ok.as_inner(), MIXED); // stored as given
}

#[test]
fn hex_fixed_normalized_admits_mixed_case_and_canonicalises_to_lowercase() {
    // 3. Normalized: mixed-case is admitted *and* canonicalized
    // to lowercase. The stored carrier is the canonical form.
    let lower_in = MIXED.to_ascii_lowercase();
    let normalized: Refined<String, HexFixedNormalized<40>> =
        Refined::try_new(MIXED.to_string()).unwrap();
    assert_eq!(normalized.as_inner(), &lower_in);
    assert_ne!(normalized.as_inner(), MIXED);

    // Idempotent: feeding the canonical form back through produces
    // the same value.
    let again: Refined<String, HexFixedNormalized<40>> =
        Refined::try_new(lower_in).unwrap();
    assert_eq!(again.as_inner(), normalized.as_inner());
}
