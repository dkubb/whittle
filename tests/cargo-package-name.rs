//! `CargoPackageName`: bounded length, leading alnum + alnum/underscore/dash body.
//!
//! Cargo-style identifier rules: the name is 1..=64 characters
//! long, the first character must be `[A-Za-z0-9]`, and subsequent
//! characters may also include `_` and `-`. The underlying
//! composition is the flat n-ary chain
//! `All<(LenChars<1, 64>, FirstChar<AsciiAlphanumeric>, EachChar<IdentDashChar>)>`,
//! wrapped in a nominal newtype with a flat error — all generated
//! from one `refinement!` error-block declaration.
//!
//! The length bound matters: without it,
//! `FirstChar<...>` and `EachChar<...>` admit the empty string,
//! because both are vacuous on empty input. Putting `LenChars<1, 64>`
//! first closes that gap and also caps the upper bound at 64
//! characters (Cargo's actual limit).
//!
//! The error block maps the rules' shared `StringError` into the
//! domain enum once; `try_new` contains no match. `Display` and
//! `core::error::Error` on the enum are generated from the per-arm
//! literals — whittle needs no error-derive macro, and adding
//! `thiserror::Error` through the attribute passthrough would
//! conflict with the emitted impls.
//!
//! Use this whenever you need to validate Cargo crate names,
//! DNS-label-style identifiers, or any "URL-slug" shape where the
//! leading character is restricted but the body admits the dash.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use core::error::Error;

use whittle::primitive::{
    AsciiAlphanumeric, EachChar, FirstChar, IdentDashChar, LenChars, StringError,
};
use whittle::{All, refinement};

refinement! {
    /// Nominal Cargo-package-name newtype.
    ///
    /// The inner `Refined<...>` is private so callers cannot bypass
    /// `try_new`. The inner rule composition is anonymous: `LenChars`
    /// runs first so empty input is rejected before the head/body
    /// predicates would vacuously accept it.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub CargoPackageName: String,
        All<(LenChars<1, 64>, FirstChar<AsciiAlphanumeric>, EachChar<IdentDashChar>)>;

    /// Flat domain error. One variant per externally distinguishable
    /// failure mode; the underlying composition and `StringError` enum
    /// are implementation details.
    error StringError => pub CargoPackageNameError {
        /// Length (in characters) is outside `1..=64`. Carries the
        /// actual character count so callers can produce precise
        /// diagnostics.
        StringError::CharCountOutOfRange { actual } => Length {
            /// Observed character count of the offending string.
            actual: usize,
        }: "cargo package name length {actual} not in 1..=64",
        /// First character is not `[A-Za-z0-9]` (e.g. leading `-` or
        /// `_`).
        StringError::BadFirstChar => BadFirstChar:
            "cargo package name must start with an ASCII alphanumeric character",
        /// A non-head character is not `[A-Za-z0-9_-]`. Carries the
        /// UTF-8 byte offset of the offending character.
        StringError::BadChar { offset } => BadChar {
            /// UTF-8 byte offset of the offending character.
            offset: usize,
        }: "cargo package name contains a character outside [A-Za-z0-9_-] at byte offset {offset}",
        // The composition emits only the three variants above.
        unreachable StringError::ByteLenOutOfRange { .. }
            | StringError::Empty
            | StringError::BadHexLength { .. },
    }
}

#[test]
fn cargo_package_name_admits_typical_crate_names() {
    // Admit: a typical Cargo crate name.
    let ok = CargoPackageName::try_new("my-crate_42".to_string()).unwrap();
    assert_eq!(ok.as_inner(), "my-crate_42");

    // Admit: leading digit is fine — `AsciiAlphanumeric` covers it.
    let digit_head = CargoPackageName::try_new("2fa-helper".to_string()).unwrap();
    assert_eq!(digit_head.as_inner(), "2fa-helper");
}

#[test]
fn cargo_package_name_rejects_empty_and_too_long_with_length_variant() {
    // Reject: empty string. The length bound fires first; without
    // it, `FirstChar` and `EachChar` would both vacuously accept
    // and the empty string would slip through.
    let empty = CargoPackageName::try_new(String::new()).unwrap_err();
    assert_eq!(empty, CargoPackageNameError::Length { actual: 0 });

    // Reject: 65 characters — one over the upper bound.
    let too_long_input = "a".repeat(65);
    let too_long = CargoPackageName::try_new(too_long_input).unwrap_err();
    assert_eq!(too_long, CargoPackageNameError::Length { actual: 65 });
}

#[test]
fn cargo_package_name_rejects_leading_dash_and_underscore_with_bad_first_char() {
    // Reject: leading `-` is not `AsciiAlphanumeric`.
    let bad_head = CargoPackageName::try_new("-leading-dash".to_string()).unwrap_err();
    assert_eq!(bad_head, CargoPackageNameError::BadFirstChar);

    // Reject: leading `_` is also not `AsciiAlphanumeric` (the head
    // predicate is tighter than the body predicate on purpose).
    let bad_head_under = CargoPackageName::try_new("_under".to_string()).unwrap_err();
    assert_eq!(bad_head_under, CargoPackageNameError::BadFirstChar);
}

#[test]
fn cargo_package_name_rejects_dot_in_body_with_bad_char_offset() {
    // Reject: `.` is not in the body alphabet.
    let bad_body = CargoPackageName::try_new("my.crate".to_string()).unwrap_err();
    assert_eq!(bad_body, CargoPackageNameError::BadChar { offset: 2 });
}

#[test]
fn cargo_package_name_error_implements_display_and_error() {
    // The flat error implements `Display` and `Error`, so it works
    // with `?`, `anyhow`, and the stdlib error machinery — both
    // impls generated from the declaration's per-arm literals, no
    // derive macro involved.
    let _: &dyn Error = &CargoPackageNameError::BadFirstChar;
    let rendered = CargoPackageNameError::Length { actual: 0 }.to_string();
    assert_eq!(rendered, "cargo package name length 0 not in 1..=64");
}
