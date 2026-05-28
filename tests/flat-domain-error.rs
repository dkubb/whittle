//! Flat domain error: the central whittle pattern.
//!
//! A `FlightCode` is "3 to 8 ASCII alphanumeric characters". The
//! invariant is naturally a composition (`And<LenChars<3, 8>,
//! EachChar<AsciiAlphanumeric>>`). Whittle's composition operators
//! require both rules to share an `Error` type, so the composition
//! surfaces `StringError` directly — but exposing the raw
//! `Refined<String, And<...>>` (or its `StringError`) to callers
//! still leaks implementation choices the newtype should encapsulate.
//!
//! The fix — and the pattern to copy whenever a domain type wraps
//! a composed rule — is:
//!
//! 1. Define a nominal newtype with a private field.
//! 2. Define a flat domain error enum with one variant per
//!    distinguishable failure mode.
//! 3. Hand-write `try_new` that pattern-matches the rules' shared
//!    flat enum into the domain variants.
//!
//! Callers then see `FlightCodeError::Length` or
//! `FlightCodeError::BadChar`, never the underlying `StringError`
//! variants the implementation happens to use.
//!
//! This example uses **`thiserror`** to derive `Display` + `Error`
//! on the flat domain error — it is the most ergonomic option when
//! it is already in your stack. Whittle does not require any
//! specific derive macro; the `Rule` trait only needs
//! `Debug + Display + core::error::Error` on `Rule::Error`, so
//! hand-rolled `impl Display + impl Error` works just as well.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use core::error::Error;

use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};
use whittle::{And, Refined};

/// The nominal newtype.
///
/// The inner `Refined<...>` is private so callers cannot bypass
/// `try_new`. The composition (IATA-ish flight code shape: 3..=8
/// ASCII alphanumeric chars) is anonymous and lives inside the
/// `Refined<T, ...>` field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlightCode(Refined<String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>>);

/// Flat domain error. One variant per externally distinguishable
/// failure mode. Callers match these; the rule composition and the
/// underlying `StringError` enum are implementation details.
///
/// `thiserror` is one option for the `Display` + `Error` impls;
/// whittle does not require any specific derive macro — hand-rolled
/// `impl Display + impl Error` works too.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum FlightCodeError {
    /// String length (in characters) is outside `3..=8`.
    #[error("flight code length {actual} not in 3..=8")]
    Length { actual: usize },
    /// Character at the given UTF-8 byte offset is not ASCII
    /// alphanumeric.
    #[error("flight code character at byte offset {offset} is not ASCII alphanumeric")]
    BadChar { offset: usize },
}

impl FlightCode {
    /// Validate `raw` and wrap. Both inner rules share
    /// `StringError`, so the match is a flat 1:1 mapping into the
    /// domain enum — no positional wrapping.
    pub fn try_new(raw: String) -> Result<Self, FlightCodeError> {
        Refined::try_new(raw)
            .map(Self)
            .map_err(|err: StringError| match err {
                StringError::CharCountOutOfRange { actual } => FlightCodeError::Length { actual },
                StringError::BadChar { offset } => FlightCodeError::BadChar { offset },
                // `StringError` is `#[non_exhaustive]`, so the catch-all
                // is required. The `LenChars` + `EachChar` composition
                // can only emit the two variants above, so this arm is
                // dead in practice — but the compiler requires it.
                other => unreachable!("unexpected inner StringError: {other:?}"),
            })
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

#[test]
fn flight_code_admits_valid_alphanumeric_input() {
    // Admit: 6 alphanumerics.
    let code = FlightCode::try_new("BA2490".to_string()).unwrap();
    assert_eq!(code.as_str(), "BA2490");
}

#[test]
fn flight_code_rejects_short_input_with_flat_length_error() {
    // Reject — too short. The flat error names the failure mode.
    let too_short = FlightCode::try_new("AB".to_string()).unwrap_err();
    assert_eq!(too_short, FlightCodeError::Length { actual: 2 });
}

#[test]
fn flight_code_rejects_bad_character_with_flat_offset_error() {
    // Reject — forbidden character. The flat error pinpoints the
    // offset in the original input.
    let bad_char = FlightCode::try_new("BA 490".to_string()).unwrap_err();
    assert_eq!(bad_char, FlightCodeError::BadChar { offset: 2 });
}

#[test]
fn flight_code_error_implements_display_and_error_traits() {
    // The flat error implements `Display` and `Error`, so it works
    // with `?`, `anyhow`, and stdlib error machinery. The derive
    // macro is your choice — `thiserror` here, but hand-rolled
    // `impl Display + impl Error` would satisfy whittle's `Rule`
    // trait too.
    let bad_char = FlightCode::try_new("BA 490".to_string()).unwrap_err();
    let _: &dyn Error = &bad_char;
    assert_eq!(
        FlightCodeError::Length { actual: 2 }.to_string(),
        "flight code length 2 not in 3..=8",
    );
}
