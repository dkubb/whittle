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
//! a composed rule — is the `refinement!` **error-block form**. One
//! declaration generates:
//!
//! 1. The nominal newtype with a private field (`try_new`,
//!    `as_inner`, `into_inner`, `AsRef`, and — via the opt-in
//!    `impl Display;` token — a carrier-forwarding `Display`).
//! 2. The flat domain error enum with one variant per
//!    distinguishable failure mode, with hand-rolled `Display` and
//!    `core::error::Error` impls — no error-derive macro involved.
//!    (Do not add `thiserror::Error` through the attribute
//!    passthrough; the impls are already emitted.)
//! 3. An `ErrorMapper` impl on the enum itself — the single place
//!    the `StringError`-to-domain mapping lives. The newtype wraps
//!    `Refined<String, MapErr<..., FlightCodeError>>`, so `try_new`
//!    (and, with the `serde` feature, deserialisation) inherits the
//!    mapping with no hand-written match anywhere.
//!
//! Callers then see `FlightCodeError::Length` or
//! `FlightCodeError::BadChar`, never the underlying `StringError`
//! variants the implementation happens to use.
//!
//! The `unreachable` arm lists the residual `StringError` variants
//! the composition cannot produce — explicitly, with no `_`
//! catch-all. Whittle's error enums are closed sums, so when
//! `StringError` gains a variant this declaration stops compiling:
//! the new failure mode must be mapped or declared residual, never
//! silently absorbed.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use core::error::Error;

use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};
use whittle::{And, refinement};

refinement! {
    /// The nominal newtype.
    ///
    /// The inner `Refined<...>` is private so callers cannot bypass
    /// `try_new`. The composition (IATA-ish flight code shape: 3..=8
    /// ASCII alphanumeric chars) is anonymous and lives inside the
    /// `Refined<T, ...>` field.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub FlightCode: String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>;
    impl Display;

    /// Flat domain error. One variant per externally distinguishable
    /// failure mode. Callers match these; the rule composition and the
    /// underlying `StringError` enum are implementation details.
    error StringError => pub FlightCodeError {
        /// String length (in characters) is outside `3..=8`.
        StringError::CharCountOutOfRange { actual } => Length {
            /// Observed character count of the offending string.
            actual: usize,
        }: "flight code length {actual} not in 3..=8",
        /// Character at the given UTF-8 byte offset is not ASCII
        /// alphanumeric.
        StringError::BadChar { offset } => BadChar {
            /// UTF-8 byte offset of the rejected character.
            offset: usize,
        }: "flight code character at byte offset {offset} is not ASCII alphanumeric",
        // `LenChars` + `EachChar` emits only the two variants above;
        // the remaining ones are residual.
        unreachable StringError::ByteLenOutOfRange { .. }
            | StringError::Empty
            | StringError::BadFirstChar
            | StringError::BadHexLength { .. },
    }
}

#[test]
fn flight_code_admits_valid_alphanumeric_input() {
    // Admit: 6 alphanumerics.
    let code = FlightCode::try_new("BA2490".to_string()).unwrap();
    assert_eq!(code.as_inner(), "BA2490");

    // The generated `AsRef<String>` and opt-in `Display` borrow and
    // render the same inner value.
    let inner: &String = code.as_ref();
    assert_eq!(inner, "BA2490");
    assert_eq!(code.to_string(), "BA2490");
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
    // with `?`, `anyhow`, and stdlib error machinery. Both impls are
    // generated from the declaration: the per-arm string literal is
    // the `Display` text, no error-derive macro required.
    let bad_char = FlightCode::try_new("BA 490".to_string()).unwrap_err();
    let _: &dyn Error = &bad_char;
    assert_eq!(
        FlightCodeError::Length { actual: 2 }.to_string(),
        "flight code length 2 not in 3..=8",
    );
}
