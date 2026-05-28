//! Airline booking domain: three refined types and an itinerary.
//!
//! - `IataAirportCode`: 3 uppercase ASCII letters (e.g. "LHR").
//! - `BookingReference` (PNR): 6 alphanumerics, stored uppercase.
//! - `FlightCode`: 3..=7 chars, head uppercase, body uppercase
//!   alphanumeric (no spaces, no lowercase).
//!
//! Each is a nominal newtype with a private inner `Refined<...>`
//! and a flat domain error — the `flat-domain-error.rs` pattern
//! applied three times. The parent `Itinerary` struct composes
//! the three; the type signature alone tells a reader (or another
//! LLM) which invariants hold.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "integration test: unwrap keeps the focus on the API; helper impls are pedagogical"
)]

use whittle::primitive::{
    AsciiAlphanumeric, CharPredicate, EachChar, FirstChar, LenChars, StringError,
};
use whittle::transform::AsciiUppercase;
use whittle::{And, AndError, Refined};

/// Predicate: ASCII uppercase letter `A`-`Z`.
struct UppercaseAscii;
impl CharPredicate for UppercaseAscii {
    fn test(ch: char) -> bool {
        ch.is_ascii_uppercase()
    }
}

/// Predicate: ASCII uppercase letter or digit (`A`-`Z`, `0`-`9`).
struct UppercaseAlphanumeric;
impl CharPredicate for UppercaseAlphanumeric {
    fn test(ch: char) -> bool {
        ch.is_ascii_uppercase() || ch.is_ascii_digit()
    }
}

type IataRule = And<LenChars<3, 3>, EachChar<UppercaseAscii>>;
type PnrRule = AsciiUppercase<And<LenChars<6, 6>, EachChar<AsciiAlphanumeric>>>;

// IATA/ICAO flight designators are 3..=7 characters: 2..=3 letters
// (the carrier code) followed by 1..=4 digits (the flight number).
// Whittle has no positional-split primitive, so we capture the
// structural shape: bounded length, uppercase head, uppercase
// alphanumeric body. The `LenChars` bound runs first so the empty /
// over-long inputs reject before `FirstChar` / `EachChar` could
// vacuously accept.
type FlightRule =
    And<LenChars<3, 7>, And<FirstChar<UppercaseAscii>, EachChar<UppercaseAlphanumeric>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IataAirportCode(Refined<String, IataRule>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookingReference(Refined<String, PnrRule>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlightCode(Refined<String, FlightRule>);

#[derive(Debug, PartialEq, Eq)]
pub enum IataError {
    Length { actual: usize },
    NotUppercase { offset: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PnrError {
    Length { actual: usize },
    BadChar { offset: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlightCodeError {
    Length { actual: usize },
    BadFirstChar,
    BadChar { offset: usize },
}

impl IataAirportCode {
    pub fn try_new(raw: String) -> Result<Self, IataError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            AndError::Left(StringError::CharCountOutOfRange { actual }) => {
                IataError::Length { actual }
            }
            AndError::Right(StringError::BadChar { offset }) => IataError::NotUppercase { offset },
            AndError::Left(o) | AndError::Right(o) => unreachable!("unexpected: {o:?}"),
        })
    }
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

impl BookingReference {
    pub fn try_new(raw: String) -> Result<Self, PnrError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            AndError::Left(StringError::CharCountOutOfRange { actual }) => {
                PnrError::Length { actual }
            }
            AndError::Right(StringError::BadChar { offset }) => PnrError::BadChar { offset },
            AndError::Left(o) | AndError::Right(o) => unreachable!("unexpected: {o:?}"),
        })
    }
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

impl FlightCode {
    pub fn try_new(raw: String) -> Result<Self, FlightCodeError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            // Outer `Left` is the `LenChars<3, 7>` arm.
            AndError::Left(StringError::CharCountOutOfRange { actual }) => {
                FlightCodeError::Length { actual }
            }
            // Outer `Right` is the inner `And<FirstChar, EachChar>`.
            // Its `Left` is the head predicate; its `Right` is the
            // body predicate.
            AndError::Right(AndError::Left(StringError::BadChar { offset: 0 })) => {
                FlightCodeError::BadFirstChar
            }
            AndError::Right(AndError::Right(StringError::BadChar { offset })) => {
                FlightCodeError::BadChar { offset }
            }
            // `StringError` is `#[non_exhaustive]`, so the match must
            // include a catch-all. The composition above can only
            // emit the three variants we just named, so the catch-all
            // is dead in practice — but the compiler requires it.
            AndError::Left(other)
            | AndError::Right(AndError::Left(other) | AndError::Right(other)) => {
                unreachable!("unexpected inner StringError variant: {other:?}")
            }
        })
    }
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Itinerary {
    pub origin: IataAirportCode,
    pub destination: IataAirportCode,
    pub flight: FlightCode,
    pub pnr: BookingReference,
}

#[test]
fn itinerary_composes_three_refined_newtypes() {
    let it = Itinerary {
        origin: IataAirportCode::try_new("LHR".to_string()).unwrap(),
        destination: IataAirportCode::try_new("JFK".to_string()).unwrap(),
        flight: FlightCode::try_new("BA117".to_string()).unwrap(),
        // Mixed-case input is stored canonicalised by `AsciiUppercase`.
        pnr: BookingReference::try_new("ab12CD".to_string()).unwrap(),
    };
    assert_eq!(it.pnr.as_str(), "AB12CD");
    assert_eq!(it.origin.as_str(), "LHR");
    assert_eq!(it.destination.as_str(), "JFK");
    assert_eq!(it.flight.as_str(), "BA117");
}

#[test]
fn iata_airport_code_rejects_short_input_with_length_error() {
    let bad_iata = IataAirportCode::try_new("LH".to_string()).unwrap_err();
    assert_eq!(bad_iata, IataError::Length { actual: 2 });
}

#[test]
fn booking_reference_distinguishes_length_and_bad_char_with_exact_variants() {
    // ─── PNR rejections — exact variant match, not `matches!`.  ─
    // `AB-12CD` has 7 chars, so the length bound rejects before the
    // body predicate sees the `-`. Asserting the exact variant
    // freezes the demonstrated behaviour into the corpus.
    let bad_pnr_len = BookingReference::try_new("AB-12CD".to_string()).unwrap_err();
    assert_eq!(bad_pnr_len, PnrError::Length { actual: 7 });
    let bad_pnr_char = BookingReference::try_new("AB-2CD".to_string()).unwrap_err();
    assert_eq!(bad_pnr_char, PnrError::BadChar { offset: 2 });
}

#[test]
fn flight_code_admits_each_valid_shape() {
    // Length bounds: 3..=7. Head must be uppercase; body must be
    // uppercase alphanumeric (no lowercase, no separator).

    // Admit at min length (3 chars).
    let short = FlightCode::try_new("AA1".to_string()).unwrap();
    assert_eq!(short.as_str(), "AA1");

    // Admit common shapes.
    let common = FlightCode::try_new("BA117".to_string()).unwrap();
    assert_eq!(common.as_str(), "BA117");

    let four_digit = FlightCode::try_new("BA1234".to_string()).unwrap();
    assert_eq!(four_digit.as_str(), "BA1234");

    // Admit at max length (7 chars).
    let max_len = FlightCode::try_new("BA12345".to_string()).unwrap();
    assert_eq!(max_len.as_str(), "BA12345");
}

#[test]
fn flight_code_rejects_each_invalid_shape_with_a_named_variant() {
    // Reject: 1 char, below min length.
    let too_short = FlightCode::try_new("B".to_string()).unwrap_err();
    assert_eq!(too_short, FlightCodeError::Length { actual: 1 });

    // Reject: 8 chars, above max length.
    let too_long = FlightCode::try_new("BA123456".to_string()).unwrap_err();
    assert_eq!(too_long, FlightCodeError::Length { actual: 8 });

    // Reject: lowercase head.
    let bad_head = FlightCode::try_new("ba117".to_string()).unwrap_err();
    assert_eq!(bad_head, FlightCodeError::BadFirstChar);

    // Reject: separator in body.
    let bad_body = FlightCode::try_new("BA-117".to_string()).unwrap_err();
    assert_eq!(bad_body, FlightCodeError::BadChar { offset: 2 });
}
