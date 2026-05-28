//! `CargoPackageName`: bounded length, leading alnum + alnum/underscore/dash body.
//!
//! Cargo-style identifier rules: the name is 1..=64 characters
//! long, the first character must be `[A-Za-z0-9]`, and subsequent
//! characters may also include `_` and `-`. The underlying
//! composition is
//! `And<LenChars<1, 64>,
//!      And<FirstChar<AsciiAlphanumeric>, EachChar<IdentDashChar>>>`,
//! wrapped in a nominal newtype with a flat error.
//!
//! The length bound matters: without it,
//! `And<FirstChar<...>, EachChar<...>>` admits the empty string,
//! because `FirstChar` is vacuous on empty input and `EachChar` is
//! vacuous on empty input. Composing with `LenChars<1, 64>` first
//! closes that gap and also caps the upper bound at 64 characters
//! (Cargo's actual limit).
//!
//! This example uses `thiserror` to derive `Display` + `Error` on
//! the flat domain error — convenient when it is already in your
//! stack. Whittle is agnostic about error-derive macros: hand-rolled
//! `impl Display + impl Error` works just as well.
//!
//! Use this whenever you need to validate Cargo crate names,
//! DNS-label-style identifiers, or any "URL-slug" shape where the
//! leading character is restricted but the body admits the dash.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use core::error::Error;

use whittle::primitive::{
    AsciiAlphanumeric, EachChar, FirstChar, IdentDashChar, LenChars, StringError,
};
use whittle::{And, AndError, Refined};

/// Inner composition rule. `LenChars` runs first so empty input is
/// rejected before the head/body predicates would vacuously accept
/// it.
type CargoPackageRule = And<
    LenChars<1, 64>,
    And<FirstChar<AsciiAlphanumeric>, EachChar<IdentDashChar>>,
>;

/// Nominal Cargo-package-name newtype. The inner `Refined<...>` is
/// private so callers cannot bypass `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CargoPackageName(Refined<String, CargoPackageRule>);

/// Flat domain error. One variant per externally distinguishable
/// failure mode; the underlying `AndError` tree and `StringError`
/// enum are implementation details.
///
/// `thiserror` is one option for the `Display` + `Error` impls;
/// whittle does not require any specific derive macro — hand-rolled
/// `impl Display + impl Error` works too.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CargoPackageNameError {
    /// Length (in characters) is outside `1..=64`. Carries the
    /// actual character count so callers can produce precise
    /// diagnostics.
    #[error("cargo package name length {actual} not in 1..=64")]
    Length { actual: usize },
    /// First character is not `[A-Za-z0-9]` (e.g. leading `-` or
    /// `_`).
    #[error("cargo package name must start with an ASCII alphanumeric character")]
    BadFirstChar,
    /// A non-head character is not `[A-Za-z0-9_-]`. Carries the
    /// UTF-8 byte offset of the offending character.
    #[error(
        "cargo package name contains a character outside [A-Za-z0-9_-] at byte offset {offset}"
    )]
    BadChar { offset: usize },
}

impl CargoPackageName {
    /// Validate `raw` and wrap. The match flattens the nested
    /// `AndError` tree into the flat domain enum.
    pub fn try_new(raw: String) -> Result<Self, CargoPackageNameError> {
        use AndError::{Left, Right};
        use CargoPackageNameError as E;
        use StringError::{BadChar, BadFirstChar, CharCountOutOfRange};
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            // Outer `Left` is the `LenChars<1, 64>` arm. Outer `Right`
            // is the inner `And<FirstChar, EachChar>`: its `Left` is
            // the head predicate, its `Right` is the body predicate.
            Left(CharCountOutOfRange { actual }) => E::Length { actual },
            Right(Left(BadFirstChar)) => E::BadFirstChar,
            Right(Right(BadChar { offset })) => E::BadChar { offset },
            // `StringError` is `#[non_exhaustive]`, so the match
            // must include a catch-all. The composition above can
            // only emit the three variants we just named, so the
            // catch-all is dead in practice — but the compiler
            // requires it.
            Left(other) | Right(Left(other) | Right(other)) => {
                unreachable!("unexpected inner StringError variant: {other:?}")
            }
        })
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

#[test]
fn cargo_package_name_admits_typical_crate_names() {
    // Admit: a typical Cargo crate name.
    let ok = CargoPackageName::try_new("my-crate_42".to_string()).unwrap();
    assert_eq!(ok.as_str(), "my-crate_42");

    // Admit: leading digit is fine — `AsciiAlphanumeric` covers it.
    let digit_head = CargoPackageName::try_new("2fa-helper".to_string()).unwrap();
    assert_eq!(digit_head.as_str(), "2fa-helper");
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
    // with `?`, `anyhow`, and the stdlib error machinery. The
    // derive macro is your choice — `thiserror` here, hand-rolled
    // elsewhere; whittle accepts either.
    let _: &dyn Error = &CargoPackageNameError::BadFirstChar;
    let rendered = CargoPackageNameError::Length { actual: 0 }.to_string();
    assert!(rendered.contains("1..=64"));
}
