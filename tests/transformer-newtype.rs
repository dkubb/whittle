//! Canonicalised domain newtype with a flat error.
//!
//! Transformers (`AsciiLowercase<R>`, `AsciiUppercase<R>`,
//! `Trim<R>`) rewrite the input *before* the inner rule runs, so
//! the stored carrier is always the canonical form. Two inputs
//! that differ only in case land in the same equivalence class:
//! `Sha1HashHex::try_new("ABC...")` and `Sha1HashHex::try_new(
//! "abc...")` produce equal newtypes.
//!
//! This is the standard pattern for case-insensitive identifiers:
//! hex hashes, hostnames, IANA tokens, base16 IDs. The newtype
//! exposes only the canonical form; the flat error names each
//! externally distinguishable failure mode (`Length`, `NonHex`)
//! without leaking the underlying `StringError` shape.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use whittle::Refined;
use whittle::primitive::{HexFixedAny, StringError};
use whittle::transform::AsciiLowercase;

/// 40-char lowercase hex string (SHA-1 shape). Input may be in
/// any case; storage is canonical lowercase. The inner
/// `Refined<...>` is private so callers cannot bypass `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sha1HashHex(Refined<String, AsciiLowercase<HexFixedAny<40>>>);

/// Flat domain error.
///
/// `HexFixedAny<40>` distinguishes the fixed-length failure
/// (`BadHexLength`) from the bad-character failure (`BadChar`);
/// the flat enum mirrors that distinction with named variants the
/// public API can match on.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Sha1HashHexError {
    /// Input did not have exactly 40 characters.
    #[error("sha1 hash length {actual} is not 40")]
    Length { actual: usize },
    /// Character at the given UTF-8 byte offset is not a hex digit.
    #[error("sha1 hash has a non-hex character at byte offset {offset}")]
    NonHex { offset: usize },
}

impl Sha1HashHex {
    pub fn try_new(raw: String) -> Result<Self, Sha1HashHexError> {
        Refined::try_new(raw)
            .map(Self)
            .map_err(|err: StringError| match err {
                StringError::BadHexLength { actual } => Sha1HashHexError::Length { actual },
                StringError::BadChar { offset } => Sha1HashHexError::NonHex { offset },
                other => unreachable!("unexpected inner StringError variant: {other:?}"),
            })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

const MIXED: &str = "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8";
const LOWER: &str = "5baa61e4c9b93f3f0682250b6cf8331b7ee68fd8";

#[test]
fn sha1_hash_hex_canonicalises_mixed_case_input_to_lowercase() {
    let hash = Sha1HashHex::try_new(MIXED.to_string()).unwrap();
    assert_eq!(hash.as_str(), LOWER);
}

#[test]
fn sha1_hash_hex_stores_already_lowercase_input_unchanged() {
    let hash = Sha1HashHex::try_new(LOWER.to_string()).unwrap();
    assert_eq!(hash.as_str(), LOWER);
}

#[test]
fn sha1_hash_hex_canonicalises_fully_uppercase_input_to_lowercase() {
    let upper = MIXED.to_ascii_uppercase();
    let hash = Sha1HashHex::try_new(upper).unwrap();
    assert_eq!(hash.as_str(), LOWER);
}

#[test]
fn sha1_hash_hex_inputs_differing_only_in_case_are_equal() {
    // The load-bearing property of a canonicalised newtype: case
    // is folded away before storage, so equivalent inputs produce
    // equal values. Downstream code can use `==` and `Hash`
    // without re-canonicalising.
    let upper = Sha1HashHex::try_new(MIXED.to_ascii_uppercase()).unwrap();
    let lower = Sha1HashHex::try_new(MIXED.to_ascii_lowercase()).unwrap();
    assert_eq!(upper, lower);
}

#[test]
fn sha1_hash_hex_rejects_short_input_with_length_variant() {
    let err = Sha1HashHex::try_new("abc123".to_string()).unwrap_err();
    assert_eq!(err, Sha1HashHexError::Length { actual: 6 });
}

#[test]
fn sha1_hash_hex_rejects_non_hex_character_with_offset() {
    // Replace the 3rd char with `g` — not a hex digit. Length is
    // still 40, so the per-char check is the rejection path.
    let mut bad = String::from(LOWER);
    bad.replace_range(2..3, "g");
    let err = Sha1HashHex::try_new(bad).unwrap_err();
    assert_eq!(err, Sha1HashHexError::NonHex { offset: 2 });
}
