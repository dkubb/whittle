//! Flat `Serialize` projection for refined tuple carriers.
//!
//! The macro only owns egress. Ingress remains the domain type's
//! existing `Deserialize` path, which routes through `try_new`.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use core::fmt;
use std::io;

use serde::Deserialize;
use whittle::{Refined, Rule};

#[derive(Debug, PartialEq, Eq)]
enum TokenError {
    ZeroLifetime,
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLifetime => f.write_str("expires_in must be greater than zero"),
        }
    }
}

enum ValidToken {}

impl Rule<(u64, u64, Option<String>)> for ValidToken {
    type Error = TokenError;

    fn refine(raw: (u64, u64, Option<String>)) -> Result<(u64, u64, Option<String>), Self::Error> {
        if raw.1 == 0 {
            return Err(TokenError::ZeroLifetime);
        }

        Ok(raw)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenWire {
    created_at: u64,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Debug)]
struct Token {
    fields: Refined<(u64, u64, Option<String>), ValidToken>,
}

impl Token {
    fn try_new(
        created_at: u64,
        expires_in: u64,
        refresh_token: Option<String>,
    ) -> Result<Self, TokenError> {
        Refined::try_new((created_at, expires_in, refresh_token)).map(|fields| Self { fields })
    }
}

impl<'de> Deserialize<'de> for Token {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = TokenWire::deserialize(deserializer)?;
        Self::try_new(wire.created_at, wire.expires_in, wire.refresh_token)
            .map_err(serde::de::Error::custom)
    }
}

whittle::serialize_flat! {
    impl Serialize for Token as |token| {
        "created_at" => token.fields.as_inner().0,
        "expires_in" => token.fields.as_inner().1,
        "refresh_token" => token.fields.as_inner().2.as_ref(),
    }
}

#[derive(Debug, Default)]
struct JsonBytes {
    bytes: Vec<u8>,
    fail_after: Option<usize>,
}

impl JsonBytes {
    const fn fail_after(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            fail_after: Some(limit),
        }
    }

    fn into_string(self) -> String {
        String::from_utf8(self.bytes).unwrap()
    }
}

impl io::Write for JsonBytes {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let Some(limit) = self.fail_after else {
            self.bytes.extend_from_slice(buf);
            return Ok(buf.len());
        };

        let remaining = limit.saturating_sub(self.bytes.len());
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "configured write failure",
            ));
        }

        let written = remaining.min(buf.len());
        self.bytes.extend_from_slice(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_to_json(token: &Token) -> String {
    let mut writer = JsonBytes::default();
    serde_json::to_writer(&mut writer, token).unwrap();
    writer.into_string()
}

#[test]
fn serialize_flat_writes_exact_json_for_refined_tuple_carrier() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();

    let json = serialize_to_json(&token);

    assert_eq!(
        json,
        r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null}"#,
    );
}

#[test]
fn serialize_flat_preserves_option_some_projection() {
    let token = Token::try_new(1_700_000_000, 3_600, Some("rtok".to_string())).unwrap();

    let json = serialize_to_json(&token);

    assert_eq!(
        json,
        r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":"rtok"}"#,
    );
}

#[test]
fn serialize_flat_propagates_json_struct_start_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();
    let mut writer = JsonBytes::fail_after(0);

    let err = serde_json::to_writer(&mut writer, &token).unwrap_err();

    assert!(err.is_io());
}

#[test]
fn serialize_flat_propagates_json_field_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();
    let mut writer = JsonBytes::fail_after(1);

    let err = serde_json::to_writer(&mut writer, &token).unwrap_err();

    assert!(err.is_io());
}

#[test]
fn deserialize_still_routes_through_try_new() {
    let token: Token =
        serde_json::from_str(r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null}"#)
            .unwrap();

    assert_eq!(token.fields.as_inner().0, 1_700_000_000);
    assert_eq!(token.fields.as_inner().1, 3_600);
    assert!(token.fields.as_inner().2.is_none());

    let err = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":0,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "expires_in must be greater than zero");
}
