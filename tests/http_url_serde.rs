//! Serde ingress and egress for parsed HTTP URLs.
//!
//! Deserialization routes a JSON string through `HttpUrl::parse`, so
//! the stored carrier is `url::Url`; serialization emits the URL's
//! string form.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use serde::{Deserialize, Serialize};
use url::Url;
use whittle::Refined;
use whittle::primitive::{HTTP_URL_MAX_LEN, HttpUrl};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Callback {
    endpoint: Refined<Url, HttpUrl>,
}

#[test]
fn http_url_round_trips_through_json_as_a_string() {
    let json = r#"{"endpoint":"https://example.com/callback?token=abc"}"#;
    let parsed: Callback = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.endpoint.as_inner().scheme(), "https");
    assert_eq!(parsed.endpoint.as_inner().host_str(), Some("example.com"));

    let encoded = serde_json::to_string(&parsed).unwrap();
    assert_eq!(encoded, json);
}

#[test]
fn http_url_deserialize_uses_the_pre_parse_cap() {
    let overlong = format!(
        r#"{{"endpoint":"https://example.com/{}"}}"#,
        "a".repeat(HTTP_URL_MAX_LEN),
    );

    let error = serde_json::from_str::<Callback>(&overlong).unwrap_err();
    assert!(error.to_string().contains("url byte length"));
}

#[test]
fn http_url_deserialize_requires_a_json_string() {
    let error = serde_json::from_str::<Callback>(r#"{"endpoint":42}"#).unwrap_err();

    assert!(error.to_string().contains("invalid type"));
}
