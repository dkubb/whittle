// Examples are interactive demonstrations: they use `println!` to
// confirm what was demonstrated and `unwrap()` to keep the focus on
// the API, not error plumbing. The workspace lints would otherwise
// deny both.
#![expect(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    reason = "interactive demonstration: println!, unwrap, and items_after_statements keep the focus on the API"
)]

//! Serde round-trip with refined domain newtypes.
//!
//! A user struct embeds two domain newtypes (`UserName` and
//! `Age`), each a private wrapper around a `Refined<T, R>` field.
//! `#[serde(transparent)]` on the newtypes routes Serialize and
//! Deserialize through the inner `Refined`, which:
//!
//! - **Serialize** writes the bare inner value (no rule-marker
//!   noise on the wire).
//! - **Deserialize** runs the raw value through `Refined::try_new`,
//!   so invalid JSON is rejected with the rule's typed error.
//!
//! Headline patterns demonstrated:
//!
//! 1. **The newtype is the domain.** Public struct fields are
//!    `UserName` and `Age`, not raw `Refined<String, LenChars<...>>`.
//!    The `Refined` carrier is an implementation detail.
//! 2. **Flat domain errors.** Each newtype owns a flat enum
//!    (`UserNameError`, `AgeError`) with named variants, plus
//!    hand-rolled `Display` + `Error` impls — no `thiserror`
//!    dependency in this example. Use whichever derive macro you
//!    prefer (`thiserror`, `snafu`, `miette`) or none.
//! 3. **`deny_unknown_fields` is the outer struct's
//!    responsibility.** `Refined<T, R>` has no visibility into the
//!    outer field map, so the attribute lives on `UserInput`.

use core::error::Error;
use core::fmt;

use serde::{Deserialize, Serialize};
use whittle::primitive::{LenChars, NumericError, StringError, Within};
use whittle::Refined;

// ─── UserName: 3..=32 character display name. ────────────────

/// Nominal display-name newtype. The inner `Refined<...>` field is
/// private so callers cannot bypass `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserName(Refined<String, LenChars<3, 32>>);

/// Flat domain error for `UserName`.
#[derive(Debug, PartialEq, Eq)]
pub enum UserNameError {
    /// Length (in characters) outside `3..=32`.
    Length { actual: usize },
}

impl fmt::Display for UserNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Length { actual } => {
                write!(f, "user name length {actual} not in 3..=32")
            }
        }
    }
}

// Hand-rolled `Error` impl — no `thiserror` dependency.
//
// Alternative: derive with `thiserror::Error` if it's already in
// your stack:
//
// ```ignore
// #[derive(Debug, thiserror::Error, PartialEq, Eq)]
// pub enum UserNameError {
//     #[error("user name length {actual} not in 3..=32")]
//     Length { actual: usize },
// }
// ```
impl Error for UserNameError {}

impl UserName {
    /// Validate `raw` and wrap. Flattens the rule's error into the
    /// domain enum.
    pub fn try_new(raw: String) -> Result<Self, UserNameError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            StringError::CharCountOutOfRange { actual } => UserNameError::Length { actual },
            // `LenChars` only emits `CharCountOutOfRange`; the
            // catch-all is dead in practice but required because
            // `StringError` is `#[non_exhaustive]`.
            other => unreachable!("unexpected inner StringError variant: {other:?}"),
        })
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_inner()
    }
}

// ─── Age: 0..=150 years. ─────────────────────────────────────

/// Nominal age newtype. The inner `Refined<...>` is private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Age(Refined<u8, Within<0, 150>>);

/// Flat domain error for `Age`.
#[derive(Debug, PartialEq, Eq)]
pub enum AgeError {
    /// Value outside `0..=150`.
    OutOfRange { value: i128 },
}

impl fmt::Display for AgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::OutOfRange { value } => {
                write!(f, "age {value} not in 0..=150")
            }
        }
    }
}

impl Error for AgeError {}

impl Age {
    /// Validate `raw` and wrap.
    pub fn try_new(raw: u8) -> Result<Self, AgeError> {
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            NumericError::OutOfRange { value } => AgeError::OutOfRange { value },
            // `Within` only emits `OutOfRange`; the catch-all is
            // dead in practice but required because `NumericError`
            // is `#[non_exhaustive]`.
            other => unreachable!("unexpected inner NumericError variant: {other:?}"),
        })
    }

    /// Return the inner age.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0.into_inner()
    }
}

// ─── Outer payload. `deny_unknown_fields` is the struct's job. ─

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct UserInput {
    name: UserName,
    age: Age,
}

fn main() {
    // Round-trip an admissible value.
    let original = UserInput {
        name: UserName::try_new("Alice".to_string()).unwrap(),
        age: Age::try_new(30).unwrap(),
    };
    let json = serde_json::to_string(&original).unwrap();
    // The newtype's `#[serde(transparent)]` makes it disappear on
    // the wire — the JSON contains only the inner values.
    assert_eq!(json, r#"{"name":"Alice","age":30}"#);
    let parsed: UserInput = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);

    // Reject at deserialize time: name is too short (fails
    // `LenChars<3, 32>`). `Refined::deserialize` runs `try_new`,
    // which surfaces the rule's error as a serde custom error.
    let bad_name: Result<UserInput, _> = serde_json::from_str(r#"{"name":"AB","age":30}"#);
    assert!(bad_name.is_err());

    // Reject at deserialize time: age out of range. Same
    // mechanism — `Refined<u8, Within<0, 150>>::deserialize` runs
    // `Within::refine` on the parsed `u8` and rejects 200.
    let bad_age: Result<UserInput, _> = serde_json::from_str(r#"{"name":"Alice","age":200}"#);
    assert!(bad_age.is_err());

    // Reject at deserialize time: unknown field.
    // `deny_unknown_fields` lives on `UserInput` because the outer
    // type controls its own field map — `Refined` has no hook for
    // this policy.
    let unknown_field: Result<UserInput, _> =
        serde_json::from_str(r#"{"name":"Alice","age":30,"email":"x"}"#);
    assert!(unknown_field.is_err());

    // The flat domain errors implement `Display` and `Error`, so
    // they work with `?`, `anyhow`, and stdlib error machinery
    // without depending on `thiserror`.
    let _: &dyn Error = &UserNameError::Length { actual: 1 };
    let _: &dyn Error = &AgeError::OutOfRange { value: 200 };

    println!("wire: {json}");
    println!("OK: domain newtypes serialize transparently; deserialize runs try_new");
}
