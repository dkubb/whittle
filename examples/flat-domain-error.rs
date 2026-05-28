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

//! Flat domain error: the central whittle pattern.
//!
//! A `FlightCode` is "3 to 8 ASCII alphanumeric characters". The
//! invariant is naturally a composition (`And<LenChars<3, 8>,
//! EachChar<AsciiAlphanumeric>>`), but exposing
//! `AndError<StringError, StringError>` to callers leaks the
//! composition machinery into every match site.
//!
//! The fix — and the pattern to copy whenever a domain type wraps
//! a composed rule — is:
//!
//! 1. Define a nominal newtype with a private field.
//! 2. Define a flat domain error enum with one variant per
//!    distinguishable failure mode.
//! 3. Hand-write `try_new` that pattern-matches the inner
//!    `AndError` into the flat domain variants.
//!
//! Callers then see `FlightCodeError::Length` or
//! `FlightCodeError::BadChar`, never `AndError::Left | Right` and
//! never the underlying `StringError` variants the implementation
//! happens to use.
//!
//! This example uses **hand-rolled `Display` + `Error`** impls so
//! the error works with `?`, `anyhow`, and the stdlib error
//! machinery without depending on `thiserror`. Whittle's `Rule`
//! trait only needs `Debug + Display + core::error::Error` on
//! `Rule::Error`; the derive macro is your choice. See the
//! trailing comment for the `thiserror`-derived equivalent.

use core::error::Error;
use core::fmt;

use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};
use whittle::{And, AndError, Refined};

/// IATA-ish flight code shape: 3..=8 ASCII alphanumeric chars.
type FlightCodeRule = And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>;

/// The nominal newtype. The inner `Refined<...>` is private so
/// callers cannot bypass `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlightCode(Refined<String, FlightCodeRule>);

/// Flat domain error. One variant per externally distinguishable
/// failure mode. Callers match these; the `AndError` shape and the
/// underlying `StringError` enum are implementation details.
#[derive(Debug, PartialEq, Eq)]
pub enum FlightCodeError {
    /// String length (in characters) is outside `3..=8`.
    Length { actual: usize },
    /// Character at the given UTF-8 byte offset is not ASCII
    /// alphanumeric.
    BadChar { offset: usize },
}

// Hand-rolled `Display` impl. A `match` over the variants produces
// a readable, machine-readable rendering. Whittle's `Rule` trait
// requires `Display + Error` on `Rule::Error`; this is the
// no-dependency way to satisfy it.
impl fmt::Display for FlightCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Length { actual } => {
                write!(f, "flight code length {actual} not in 3..=8")
            }
            Self::BadChar { offset } => write!(
                f,
                "flight code character at byte offset {offset} is not ASCII alphanumeric",
            ),
        }
    }
}

// Hand-rolled `Error` impl. With no source / cause chain to
// forward, the empty impl block is enough — `Error` provides
// default methods for everything else.
impl Error for FlightCodeError {}

impl FlightCode {
    /// Validate `raw` and wrap. The match flattens the
    /// composition error into the domain enum.
    pub fn try_new(raw: String) -> Result<Self, FlightCodeError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            AndError::Left(StringError::CharCountOutOfRange { actual }) => {
                FlightCodeError::Length { actual }
            }
            AndError::Right(StringError::BadChar { offset }) => {
                FlightCodeError::BadChar { offset }
            }
            // `StringError` is `#[non_exhaustive]`, so the match
            // must include a catch-all. The `LenChars` + `EachChar`
            // composition can only emit the two variants above, so
            // the catch-all is dead in practice — but the compiler
            // requires it.
            AndError::Left(other) | AndError::Right(other) => {
                unreachable!("unexpected inner StringError: {other:?}")
            }
        })
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

fn main() {
    // Admit: 6 alphanumerics.
    let code = FlightCode::try_new("BA2490".to_string()).unwrap();
    assert_eq!(code.as_str(), "BA2490");

    // Reject — too short. The flat error names the failure mode.
    let too_short = FlightCode::try_new("AB".to_string()).unwrap_err();
    assert_eq!(too_short, FlightCodeError::Length { actual: 2 });

    // Reject — forbidden character. The flat error pinpoints the
    // offset in the original input.
    let bad_char = FlightCode::try_new("BA 490".to_string()).unwrap_err();
    assert_eq!(bad_char, FlightCodeError::BadChar { offset: 2 });

    // The flat error implements `Display` and `Error`, so it works
    // with `?`, `anyhow`, and stdlib error machinery — no
    // `thiserror` dependency required.
    let _: &dyn Error = &bad_char;
    assert_eq!(
        FlightCodeError::Length { actual: 2 }.to_string(),
        "flight code length 2 not in 3..=8",
    );

    println!("flight code: {}", code.as_str());
    println!("OK: FlightCode wraps And<...> with a flat FlightCodeError");
}

// ─── Alternative: `thiserror`-derived equivalent. ────────────
//
// If `thiserror` is already in your stack, the same error type
// is one derive away. Whittle does not care which you pick — the
// `Rule` trait only requires `Debug + Display + core::error::Error`.
//
// ```ignore
// #[derive(Debug, thiserror::Error, PartialEq, Eq)]
// pub enum FlightCodeError {
//     #[error("flight code length {actual} not in 3..=8")]
//     Length { actual: usize },
//     #[error("flight code character at byte offset {offset} is not ASCII alphanumeric")]
//     BadChar { offset: usize },
// }
// ```

