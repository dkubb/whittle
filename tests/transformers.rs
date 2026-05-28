//! Transformers: rewrite input before the inner rule runs.
//!
//! `AsciiLowercase<R>`, `AsciiUppercase<R>`, and `Trim<R>` are
//! adapters that normalize the input first and then delegate to
//! `R`. The stored carrier is the canonical form — `try_new(" Hi
//! ")` and `try_new("hi")` produce equal `Refined` values when
//! wrapped in `Trim<AsciiLowercase<NonEmpty>>`.
//!
//! Use these when canonical form is part of the contract (hex
//! hashes, hostnames, IANA tokens). For invariants where the
//! input should be preserved verbatim, stick with validation-only
//! rules.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::Refined;
use whittle::primitive::NonEmpty;
use whittle::transform::{AsciiLowercase, AsciiUppercase, Trim};

#[test]
fn ascii_lowercase_canonicalises_input_before_inner_rule_runs() {
    // `AsciiLowercase<R>` lowercases first, then validates with `R`.
    let lower: Refined<String, AsciiLowercase<NonEmpty>> =
        Refined::try_new("HELLO".to_string()).unwrap();
    assert_eq!(lower.as_inner(), "hello");
}

#[test]
fn ascii_uppercase_canonicalises_input_before_inner_rule_runs() {
    // `AsciiUppercase<R>` is the symmetric counterpart.
    let upper: Refined<String, AsciiUppercase<NonEmpty>> =
        Refined::try_new("hello".to_string()).unwrap();
    assert_eq!(upper.as_inner(), "HELLO");
}

#[test]
fn trim_strips_whitespace_and_rejects_whitespace_only_input() {
    // `Trim<R>` strips leading + trailing whitespace, then validates.
    let trimmed: Refined<String, Trim<NonEmpty>> = Refined::try_new("  hi  ".to_string()).unwrap();
    assert_eq!(trimmed.as_inner(), "hi");

    // Whitespace-only input is empty after trimming, so the inner
    // `NonEmpty` rule rejects.
    let blank = Refined::<String, Trim<NonEmpty>>::try_new("   ".to_string());
    blank.unwrap_err();
}

#[test]
fn transformers_compose_and_produce_equal_refined_values_for_equivalent_inputs() {
    // Transformers compose. Outer runs first: `Trim` strips, then
    // `AsciiLowercase` lowercases, then `NonEmpty` validates.
    let canon: Refined<String, Trim<AsciiLowercase<NonEmpty>>> =
        Refined::try_new(" Hello ".to_string()).unwrap();
    assert_eq!(canon.as_inner(), "hello");

    // Two inputs that differ only in case + surrounding whitespace
    // produce equal `Refined` values under the same composition.
    let other: Refined<String, Trim<AsciiLowercase<NonEmpty>>> =
        Refined::try_new("hello".to_string()).unwrap();
    assert_eq!(canon.as_inner(), other.as_inner());
}
