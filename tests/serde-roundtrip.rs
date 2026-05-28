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
//!    (`UserNameError`, `AgeError`) with named variants, derived
//!    via `thiserror` for brevity. Whittle is agnostic about
//!    error-derive macros — hand-rolled `impl Display + impl Error`
//!    works just as well.
//! 3. **`deny_unknown_fields` is the outer struct's
//!    responsibility.** `Refined<T, R>` has no visibility into the
//!    outer field map, so the attribute lives on `UserInput`.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use core::error::Error;

use serde::{Deserialize, Serialize};
use whittle::Refined;
use whittle::primitive::{LenChars, NumericError, StringError, Within};

// ─── UserName: 3..=32 character display name. ────────────────

/// Nominal display-name newtype. The inner `Refined<...>` field is
/// private so callers cannot bypass `try_new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserName(Refined<String, LenChars<3, 32>>);

/// Flat domain error for `UserName`.
///
/// `thiserror` is one option for the `Display` + `Error` impls;
/// whittle does not require any specific derive macro — hand-rolled
/// `impl Display + impl Error` works too.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum UserNameError {
    /// Length (in characters) outside `3..=32`.
    #[error("user name length {actual} not in 3..=32")]
    Length { actual: usize },
}

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
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgeError {
    /// Value outside `0..=150`.
    #[error("age {value} not in 0..=150")]
    OutOfRange { value: i128 },
}

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

#[test]
fn user_input_round_trips_through_json_transparently() {
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
    assert_eq!(parsed.name.as_str(), "Alice");
    assert_eq!(parsed.age.get(), 30);
}

#[test]
fn deserialize_runs_try_new_and_rejects_invalid_name() {
    // Reject at deserialize time: name is too short (fails
    // `LenChars<3, 32>`). `Refined::deserialize` runs `try_new`,
    // which surfaces the rule's error as a serde custom error.
    let bad_name: Result<UserInput, _> = serde_json::from_str(r#"{"name":"AB","age":30}"#);
    bad_name.unwrap_err();
}

#[test]
fn deserialize_runs_try_new_and_rejects_out_of_range_age() {
    // Reject at deserialize time: age out of range. Same
    // mechanism — `Refined<u8, Within<0, 150>>::deserialize` runs
    // `Within::refine` on the parsed `u8` and rejects 200.
    let bad_age: Result<UserInput, _> = serde_json::from_str(r#"{"name":"Alice","age":200}"#);
    bad_age.unwrap_err();
}

#[test]
fn deny_unknown_fields_rejects_extra_keys_on_outer_struct() {
    // Reject at deserialize time: unknown field.
    // `deny_unknown_fields` lives on `UserInput` because the outer
    // type controls its own field map — `Refined` has no hook for
    // this policy.
    let unknown_field: Result<UserInput, _> =
        serde_json::from_str(r#"{"name":"Alice","age":30,"email":"x"}"#);
    unknown_field.unwrap_err();
}

#[test]
fn flat_domain_errors_implement_display_and_error_traits() {
    // The flat domain errors implement `Display` and `Error`, so
    // they work with `?`, `anyhow`, and stdlib error machinery.
    // The derive macro is your choice — `thiserror` here, but
    // hand-rolled `impl Display + impl Error` would satisfy
    // whittle's `Rule` trait too.
    let _: &dyn Error = &UserNameError::Length { actual: 1 };
    let _: &dyn Error = &AgeError::OutOfRange { value: 200 };
    assert_eq!(
        UserNameError::Length { actual: 1 }.to_string(),
        "user name length 1 not in 3..=32",
    );
}
