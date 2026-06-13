//! `pattern!` macro: the facade-resolved, compile-time-validated path
//! to a `Pattern<RE>` rule.
//!
//! These tests run from the `whittle` facade crate so the macro's
//! `proc-macro-crate` path resolution picks the facade name
//! (`::whittle::primitive::Pattern`). They require the `regex` feature.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the macro API"
)]
#![expect(
    clippy::needless_raw_strings,
    reason = "regex patterns stay raw strings so a later edit adding a backslash escape \
              does not silently change their meaning"
)]

use whittle::Refined;
use whittle::primitive::PatternError;

/// `pattern!` expands to a usable `Pattern<RE>` rule type.
type Name = whittle::pattern!(r"^(?:[A-Z])(?:-?[A-Za-z]+)*$");

#[test]
fn pattern_macro_admits_matching_string() {
    let ok: Refined<String, Name> = Refined::try_new("A-Bc-De".to_string()).unwrap();
    assert_eq!(ok.as_inner(), "A-Bc-De");
}

#[test]
fn pattern_macro_rejects_non_matching_string() {
    let err = Refined::<String, Name>::try_new("abc".to_string()).unwrap_err();
    assert_eq!(err, PatternError::NoMatch);
}

#[test]
fn pattern_macro_can_be_used_inline() {
    // The macro is usable directly in a type position, not only via a
    // `type` alias.
    let ok: Refined<String, whittle::pattern!(r"[0-9]+")> =
        Refined::try_new("12345".to_string()).unwrap();
    assert_eq!(ok.as_inner(), "12345");
}
