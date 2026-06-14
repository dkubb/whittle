//! Flat serde for generated `record!` tuple carriers.
//!
//! The generated codec owns both egress and ingress. Deserialization
//! accepts map-style struct input and compact sequence-form struct
//! input, and always routes through `try_new`.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use std::{fmt, io};

use serde::ser::{self, Impossible, SerializeStruct};
use serde_test::{
    Token as SerdeToken, assert_de_tokens, assert_de_tokens_error, assert_ser_tokens_error,
};
use whittle::record;

record! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Token {
        created_at: u64,
        expires_in: u64,
        refresh_token: Option<String>,
    }

    rule(created_at, expires_in, refresh_token) {
        let _: (&u64, &Option<String>) = (created_at, refresh_token);

        if *expires_in == 0 {
            Err(TokenError::ZeroLifetime)
        } else {
            Ok(())
        }
    }

    error TokenError {
        ZeroLifetime => "expires_in must be greater than zero",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldSerializeError;

impl fmt::Display for FieldSerializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("field serializer failed")
    }
}

impl core::error::Error for FieldSerializeError {}

impl ser::Error for FieldSerializeError {
    fn custom<T>(_message: T) -> Self
    where
        T: fmt::Display,
    {
        Self
    }
}

struct FailingRecordSerializer<const FAIL_FIELD: bool>;

struct FailingRecordSerializeStruct<const FAIL_FIELD: bool>;

impl<const FAIL_FIELD: bool> SerializeStruct for FailingRecordSerializeStruct<FAIL_FIELD> {
    type Error = FieldSerializeError;
    type Ok = ();

    fn serialize_field<T>(&mut self, _key: &'static str, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        if FAIL_FIELD {
            Err(FieldSerializeError)
        } else {
            Ok(())
        }
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if FAIL_FIELD {
            Ok(())
        } else {
            Err(FieldSerializeError)
        }
    }
}

impl<const FAIL_FIELD: bool> ser::Serializer for FailingRecordSerializer<FAIL_FIELD> {
    type Error = FieldSerializeError;
    type Ok = ();
    type SerializeMap = Impossible<(), FieldSerializeError>;
    type SerializeSeq = Impossible<(), FieldSerializeError>;
    type SerializeStruct = FailingRecordSerializeStruct<FAIL_FIELD>;
    type SerializeStructVariant = Impossible<(), FieldSerializeError>;
    type SerializeTuple = Impossible<(), FieldSerializeError>;
    type SerializeTupleStruct = Impossible<(), FieldSerializeError>;
    type SerializeTupleVariant = Impossible<(), FieldSerializeError>;

    fn serialize_bool(self, _value: bool) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_i8(self, _value: i8) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_i16(self, _value: i16) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_i32(self, _value: i32) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_i64(self, _value: i64) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_u8(self, _value: u8) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_u16(self, _value: u16) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_u32(self, _value: u32) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_u64(self, _value: u64) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_char(self, _value: char) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_str(self, _value: &str) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_some<T>(self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        Err(FieldSerializeError)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        Err(FieldSerializeError)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        Err(FieldSerializeError)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(FieldSerializeError)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(FailingRecordSerializeStruct)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(FieldSerializeError)
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
fn record_accessors_and_into_parts_are_named_tuple_projections() {
    let token = Token::try_new(1_700_000_000, 3_600, Some("rtok".to_string())).unwrap();

    assert_eq!(token.created_at(), &1_700_000_000);
    assert_eq!(token.expires_in(), &3_600);
    assert_eq!(token.refresh_token().as_deref(), Some("rtok"));
    assert_eq!(
        token.into_parts(),
        (1_700_000_000, 3_600, Some("rtok".to_string())),
    );
}

#[test]
fn record_try_new_rejects_cross_field_failure_with_domain_error() {
    let error = Token::try_new(1_700_000_000, 0, None).unwrap_err();

    assert_eq!(error, TokenError::ZeroLifetime);
    assert_eq!(error.to_string(), "expires_in must be greater than zero");
    let _: &dyn core::error::Error = &error;
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
fn record_serialize_propagates_later_json_field_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();
    let mut writer = JsonBytes::fail_after(r#"{"created_at":1700000000"#.len());

    let err = serde_json::to_writer(&mut writer, &token).unwrap_err();

    assert!(err.is_io());
}

#[test]
fn record_serialize_propagates_token_field_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();

    assert_ser_tokens_error(
        &token,
        &[
            SerdeToken::Struct {
                name: "Token",
                len: 3,
            },
            SerdeToken::Str("created_at"),
            SerdeToken::U64(1_700_000_000),
            SerdeToken::Str("wrong"),
        ],
        r#"expected Token::Str("wrong") but serialized as Str("expires_in")"#,
    );
}

#[test]
fn record_serialize_propagates_direct_serialize_field_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();

    let err = serde::Serialize::serialize(&token, FailingRecordSerializer::<true>).unwrap_err();

    assert_eq!(err, FieldSerializeError);
}

#[test]
fn record_serialize_propagates_direct_serialize_end_error() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();

    let err = serde::Serialize::serialize(&token, FailingRecordSerializer::<false>).unwrap_err();

    assert_eq!(err, FieldSerializeError);
}

#[test]
fn deserialize_still_routes_through_try_new() {
    let token: Token =
        serde_json::from_str(r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null}"#)
            .unwrap();

    assert_eq!(token.created_at(), &1_700_000_000);
    assert_eq!(token.expires_in(), &3_600);
    assert!(token.refresh_token().is_none());

    let err = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":0,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("expires_in must be greater than zero")
    );
}

#[test]
fn record_deserialize_accepts_sequence_form_structs() {
    let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();

    assert_de_tokens(
        &token,
        &[
            SerdeToken::Seq { len: Some(3) },
            SerdeToken::U64(1_700_000_000),
            SerdeToken::U64(3_600),
            SerdeToken::None,
            SerdeToken::SeqEnd,
        ],
    );
}

#[test]
fn record_deserialize_rejects_short_sequence_form_structs() {
    assert_de_tokens_error::<Token>(
        &[
            SerdeToken::Seq { len: Some(2) },
            SerdeToken::U64(1_700_000_000),
            SerdeToken::U64(3_600),
            SerdeToken::SeqEnd,
        ],
        "invalid length 2, expected struct Token",
    );
}

#[test]
fn record_deserialize_propagates_sequence_field_decode_error() {
    assert_de_tokens_error::<Token>(
        &[SerdeToken::Seq { len: Some(3) }, SerdeToken::Str("bad")],
        "invalid type: string \"bad\", expected u64",
    );

    assert_de_tokens_error::<Token>(
        &[
            SerdeToken::Seq { len: Some(3) },
            SerdeToken::U64(1_700_000_000),
            SerdeToken::Str("bad"),
        ],
        "invalid type: string \"bad\", expected u64",
    );

    assert_de_tokens_error::<Token>(
        &[
            SerdeToken::Seq { len: Some(3) },
            SerdeToken::U64(1_700_000_000),
            SerdeToken::U64(3_600),
            SerdeToken::I32(7),
        ],
        "invalid type: integer `7`, expected option",
    );
}

#[test]
fn record_deserialize_rejects_unknown_duplicate_and_missing_fields() {
    let unknown = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null,"extra":1}"#,
    )
    .unwrap_err();
    assert!(unknown.to_string().contains("unknown field"));

    let duplicate = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"created_at":1700000001,"expires_in":3600,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("duplicate field"));

    let missing =
        serde_json::from_str::<Token>(r#"{"created_at":1700000000,"refresh_token":null}"#)
            .unwrap_err();
    assert!(missing.to_string().contains("missing field"));
}

#[test]
fn record_deserialize_rejects_each_duplicate_field() {
    let duplicate_created_at = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"created_at":1700000001,"expires_in":3600,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(duplicate_created_at.to_string().contains("created_at"));

    let duplicate_expires_in = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":3600,"expires_in":7200,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(duplicate_expires_in.to_string().contains("expires_in"));

    let duplicate_refresh_token = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null,"refresh_token":"rtok"}"#,
    )
    .unwrap_err();
    assert!(
        duplicate_refresh_token
            .to_string()
            .contains("refresh_token")
    );
}

#[test]
fn record_deserialize_rejects_each_missing_field() {
    let missing_created_at =
        serde_json::from_str::<Token>(r#"{"expires_in":3600,"refresh_token":null}"#).unwrap_err();
    assert!(missing_created_at.to_string().contains("created_at"));

    let missing_refresh_token =
        serde_json::from_str::<Token>(r#"{"created_at":1700000000,"expires_in":3600}"#)
            .unwrap_err();
    assert!(missing_refresh_token.to_string().contains("refresh_token"));
}

#[test]
fn record_deserialize_propagates_each_field_decode_error() {
    let bad_created_at = serde_json::from_str::<Token>(
        r#"{"created_at":"bad","expires_in":3600,"refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(bad_created_at.to_string().contains("invalid type"));

    let bad_expires_in = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":"bad","refresh_token":null}"#,
    )
    .unwrap_err();
    assert!(bad_expires_in.to_string().contains("invalid type"));

    let bad_refresh_token = serde_json::from_str::<Token>(
        r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":1}"#,
    )
    .unwrap_err();
    assert!(bad_refresh_token.to_string().contains("invalid type"));
}

#[test]
fn record_deserialize_names_expected_field_and_record_shapes() {
    assert_de_tokens_error::<Token>(
        &[SerdeToken::Map { len: Some(3) }, SerdeToken::I32(1)],
        "invalid type: integer `1`, expected record field",
    );

    let err = serde_json::from_str::<Token>("null").unwrap_err();
    assert!(err.to_string().contains("expected struct Token"));
}
