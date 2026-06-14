//! Parsed HTTP URL boundary.
//!
//! `HttpUrl` carries `url::Url`, not a raw string. Use
//! `HttpUrl::parse` at string boundaries so the pre-parse byte cap,
//! URL parser, and HTTP policy all run once before downstream code
//! receives the parsed carrier.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use url::Url;
use whittle::Refined;
use whittle::primitive::{HTTP_URL_MAX_LEN, HttpUrl, HttpUrlError};

#[test]
fn http_url_parse_carries_the_parsed_url() {
    let url: Refined<Url, HttpUrl> = HttpUrl::parse("https://example.com/search?q=rust").unwrap();

    assert_eq!(url.as_inner().scheme(), "https");
    assert_eq!(url.as_inner().host_str(), Some("example.com"));
    assert_eq!(url.as_inner().path(), "/search");
    assert_eq!(url.as_inner().query(), Some("q=rust"));
    assert_eq!(url.as_inner().as_str(), "https://example.com/search?q=rust");
}

#[test]
fn http_url_rejects_empty_and_overlong_inputs_before_parse() {
    let empty = HttpUrl::parse("").unwrap_err();
    assert_eq!(empty, HttpUrlError::Empty);

    let overlong = format!("https://example.com/{}", "a".repeat(HTTP_URL_MAX_LEN));
    let error = HttpUrl::parse(&overlong).unwrap_err();
    assert_eq!(
        error,
        HttpUrlError::TooLong {
            actual: overlong.len(),
            max: HTTP_URL_MAX_LEN,
        },
    );
}

#[test]
fn http_url_rejects_relative_and_malformed_inputs_at_parse() {
    let relative = HttpUrl::parse("/relative/path").unwrap_err();
    assert!(matches!(relative, HttpUrlError::Parse(_)));

    let malformed = HttpUrl::parse("http://exa mple.com").unwrap_err();
    assert!(matches!(malformed, HttpUrlError::Parse(_)));
}

#[test]
fn http_url_rejects_non_http_missing_host_userinfo_and_fragments() {
    let ftp = HttpUrl::parse("ftp://example.com/file").unwrap_err();
    assert_eq!(ftp, HttpUrlError::UnsupportedScheme);

    let missing_host = HttpUrl::parse("https:").unwrap_err();
    assert_eq!(missing_host, HttpUrlError::MissingHost);

    let missing_host = Url::parse("file:///tmp/socket").unwrap();
    let missing_host = Refined::<Url, HttpUrl>::try_new(missing_host).unwrap_err();
    assert_eq!(missing_host, HttpUrlError::MissingHost);

    let userinfo = HttpUrl::parse("https://user:pass@example.com/").unwrap_err();
    assert_eq!(userinfo, HttpUrlError::HasUserinfo);

    let fragment = HttpUrl::parse("https://example.com/path#section").unwrap_err();
    assert_eq!(fragment, HttpUrlError::HasFragment);
}
