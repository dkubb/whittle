//! `FromStr` / `TryFrom` boundary parsing for a domain newtype.
//!
//! Three trait impls give the newtype the canonical Rust
//! boundary-parsing API: `clap` uses `FromStr`, `serde` (and
//! `argh`) can use `TryFrom<String>` or `TryFrom<&str>`, and
//! anywhere downstream that writes `s.parse::<FlightCode>()`
//! plugs straight in.
//!
//! The error returned by the trait impls is the SAME flat domain
//! error that `try_new` returns — no wrapping, no positional
//! indirection. Use this whenever you want a refined newtype to
//! flow through the ecosystem's stringly-typed boundaries
//! (config files, CLI args, environment variables) without
//! leaking the underlying `StringError` shape.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use core::str::FromStr;

use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};
use whittle::{And, Refined};

/// IATA-ish flight code: 3..=8 ASCII alphanumeric characters.
/// The inner `Refined<...>` is private so callers cannot bypass
/// `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlightCode(Refined<String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>>);

/// Flat domain error. One variant per externally distinguishable
/// failure mode — the same enum that `FromStr` and `TryFrom`
/// return, so callers see one error type regardless of which
/// boundary they crossed.
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
    pub fn try_new(raw: String) -> Result<Self, FlightCodeError> {
        Refined::try_new(raw)
            .map(Self)
            .map_err(|err: StringError| match err {
                StringError::CharCountOutOfRange { actual } => FlightCodeError::Length { actual },
                StringError::BadChar { offset } => FlightCodeError::BadChar { offset },
                StringError::ByteLenOutOfRange { .. }
                | StringError::Empty
                | StringError::BadFirstChar
                | StringError::BadHexLength { .. } => {
                    unreachable!("composition emits only CharCountOutOfRange and BadChar")
                }
            })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

// `FromStr` delegates to `try_new`. `clap` and `s.parse::<...>()`
// both go through this impl; the `Err` type is the flat domain
// error, not the underlying `StringError`.
impl FromStr for FlightCode {
    type Err = FlightCodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s.to_string())
    }
}

// `TryFrom<&str>` delegates to `FromStr`. Symmetric so callers
// can pick whichever shape their framework prefers.
impl TryFrom<&str> for FlightCode {
    type Error = FlightCodeError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

// `TryFrom<String>` consumes the input directly, so the byte
// buffer can move into the `Refined` carrier without an extra
// copy.
impl TryFrom<String> for FlightCode {
    type Error = FlightCodeError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_new(s)
    }
}

#[test]
fn from_str_admits_valid_input_via_parse_turbofish() {
    let code: FlightCode = "BA117".parse().unwrap();
    assert_eq!(code.as_str(), "BA117");
}

#[test]
fn from_str_rejects_invalid_input_with_flat_domain_error() {
    let err = "BA".parse::<FlightCode>().unwrap_err();
    assert_eq!(err, FlightCodeError::Length { actual: 2 });

    let bad_char = "BA 17".parse::<FlightCode>().unwrap_err();
    assert_eq!(bad_char, FlightCodeError::BadChar { offset: 2 });
}

#[test]
fn try_from_str_slice_admits_valid_input() {
    let code = FlightCode::try_from("BA117").unwrap();
    assert_eq!(code.as_str(), "BA117");
}

#[test]
fn try_from_owned_string_admits_valid_input() {
    let code = FlightCode::try_from(String::from("BA117")).unwrap();
    assert_eq!(code.as_str(), "BA117");
}

#[test]
fn all_three_boundary_impls_return_the_same_flat_domain_error() {
    // The point: `FromStr`, `TryFrom<&str>`, and `TryFrom<String>`
    // all surface the same `FlightCodeError` — callers don't have
    // to map between three error types depending on which boundary
    // they crossed.
    let from_str: FlightCodeError = "ab".parse::<FlightCode>().unwrap_err();
    let from_slice: FlightCodeError = FlightCode::try_from("ab").unwrap_err();
    let from_owned: FlightCodeError = FlightCode::try_from(String::from("ab")).unwrap_err();

    assert_eq!(from_str, FlightCodeError::Length { actual: 2 });
    assert_eq!(from_slice, FlightCodeError::Length { actual: 2 });
    assert_eq!(from_owned, FlightCodeError::Length { actual: 2 });
}
