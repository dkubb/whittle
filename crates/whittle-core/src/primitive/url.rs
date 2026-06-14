//! HTTP URL primitive rule.
//!
//! `HttpUrl` is a rule over the parsed [`url::Url`] carrier. It is not
//! a string validator: the boundary parses once, applies a pre-parse
//! length cap, then carries the parsed URL downstream. Egress uses
//! [`Url::as_str`](url::Url::as_str) or `Display`.
//!
//! Available behind the `url` Cargo feature. The dependency is compiled
//! with default features disabled so this primitive stays compatible
//! with the kernel's `#![no_std]` + `alloc` default.

use ::url::{ParseError, Url};

#[cfg(any(feature = "serde", feature = "proptest"))]
use alloc::string::String;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::{Refined, Rule};

/// Maximum accepted HTTP URL byte length before parsing.
///
/// This is a denial-of-service guard on ingress, not a URL grammar
/// rule. It runs before `Url::parse` so overlong inputs are rejected
/// without invoking the parser.
pub const HTTP_URL_MAX_LEN: usize = 8_192;

/// Parsed HTTP or HTTPS URL with no userinfo and no fragment.
///
/// The accepted carrier is [`url::Url`]. Use [`HttpUrl::parse`] at
/// string boundaries so the pre-parse length cap and parser both run;
/// use [`Refined::try_new`] when the input is already a parsed `Url`.
///
/// # Examples
///
/// ```
/// use url::Url;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{HttpUrl, HttpUrlError};
///
/// // Admit: parse from string, carry the parsed URL.
/// let ok: Refined<Url, HttpUrl> =
///     HttpUrl::parse("https://example.com/search?q=rust").unwrap();
/// assert_eq!(ok.as_inner().scheme(), "https");
/// assert_eq!(ok.as_inner().host_str(), Some("example.com"));
/// assert_eq!(ok.as_inner().as_str(), "https://example.com/search?q=rust");
///
/// // Reject: only HTTP and HTTPS schemes are admissible.
/// let err = HttpUrl::parse("ftp://example.com/file").unwrap_err();
/// assert_eq!(err, HttpUrlError::UnsupportedScheme);
/// ```
pub struct HttpUrl;

impl HttpUrl {
    /// Parse and refine an HTTP URL from a string boundary.
    ///
    /// The pipeline is:
    ///
    /// 1. reject empty input,
    /// 2. reject inputs longer than [`HTTP_URL_MAX_LEN`] bytes,
    /// 3. parse with [`Url::parse`],
    /// 4. refine the parsed carrier with [`Refined::try_new`].
    ///
    /// # Errors
    ///
    /// Returns [`HttpUrlError`] when the input is empty, over the
    /// pre-parse byte cap, rejected by the URL parser, or rejected by
    /// the HTTP URL policy.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::HttpUrl;
    ///
    /// let url = HttpUrl::parse("http://example.test/path").unwrap();
    /// assert_eq!(url.as_inner().host_str(), Some("example.test"));
    /// assert_eq!(url.as_inner().path(), "/path");
    /// ```
    #[inline]
    pub fn parse(raw: &str) -> Result<Refined<Url, Self>, HttpUrlError> {
        if raw.is_empty() {
            return Err(HttpUrlError::Empty);
        }
        if raw.len() > HTTP_URL_MAX_LEN {
            return Err(HttpUrlError::TooLong {
                actual: raw.len(),
                max: HTTP_URL_MAX_LEN,
            });
        }
        let parsed = Url::parse(raw).map_err(parse_error)?;
        Refined::try_new(parsed)
    }
}

/// Errors produced by [`HttpUrl`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpUrlError {
    /// Empty input at the string boundary.
    Empty,

    /// Input exceeded the pre-parse byte cap.
    TooLong {
        /// Observed byte length.
        actual: usize,
        /// Maximum accepted byte length.
        max: usize,
    },

    /// The URL parser rejected the input.
    Parse(ParseError),

    /// Scheme is not `http` or `https`.
    UnsupportedScheme,

    /// Parsed URL has no host.
    MissingHost,

    /// Parsed URL includes userinfo.
    HasUserinfo,

    /// Parsed URL includes a fragment.
    HasFragment,
}

impl core::fmt::Display for HttpUrlError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("url is empty"),
            Self::TooLong { actual, max } => {
                write!(f, "url byte length {actual} exceeds maximum {max}")
            }
            Self::Parse(error) => write!(f, "url parse error: {error}"),
            Self::UnsupportedScheme => f.write_str("url scheme must be http or https"),
            Self::MissingHost => f.write_str("url must include a host"),
            Self::HasUserinfo => f.write_str("url must not include userinfo"),
            Self::HasFragment => f.write_str("url must not include a fragment"),
        }
    }
}

impl core::error::Error for HttpUrlError {}

impl Rule<Url> for HttpUrl {
    type Error = HttpUrlError;

    #[inline]
    fn refine(raw: Url) -> Result<Url, Self::Error> {
        if raw.host().is_none() {
            return Err(HttpUrlError::MissingHost);
        }
        if !matches!(raw.scheme(), "http" | "https") {
            return Err(HttpUrlError::UnsupportedScheme);
        }
        if has_userinfo(&raw) {
            return Err(HttpUrlError::HasUserinfo);
        }
        if raw.fragment().is_some() {
            return Err(HttpUrlError::HasFragment);
        }
        Ok(raw)
    }
}

// SOUNDNESS: `refine` inspects the parsed URL and returns the input
// `Url` itself on acceptance — no canonicalisation happens inside the
// rule.
impl crate::rule::PureFilter for HttpUrl {}

#[cfg(feature = "serde")]
impl<'de> crate::DeserializeRule<'de, Url> for HttpUrl {
    #[inline]
    fn deserialize_refined<D>(deserializer: D) -> Result<Refined<Url, Self>, D::Error>
    where
        D: crate::serde::Deserializer<'de>,
    {
        use crate::serde::Deserialize as _;

        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(crate::serde::de::Error::custom)
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryRule<Url> for HttpUrl {
    type Strategy = proptest::strategy::BoxedStrategy<Url>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use alloc::format;
        use proptest::strategy::Strategy as _;

        (
            proptest::prop_oneof![
                proptest::strategy::Just("http"),
                proptest::strategy::Just("https"),
            ],
            host_strategy(),
            path_strategy(),
        )
            .prop_map(|(scheme, host, path)| {
                let raw = format!("{scheme}://{host}{path}");
                Url::parse(&raw).expect("constructed HTTP URL must parse")
            })
            .boxed()
    }
}

fn has_userinfo(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

const fn parse_error(parsed_error: ParseError) -> HttpUrlError {
    match parsed_error {
        ParseError::EmptyHost => HttpUrlError::MissingHost,
        ParseError::IdnaError
        | ParseError::InvalidPort
        | ParseError::InvalidIpv4Address
        | ParseError::InvalidIpv6Address
        | ParseError::InvalidDomainCharacter
        | ParseError::RelativeUrlWithoutBase
        | ParseError::RelativeUrlWithCannotBeABaseBase
        | ParseError::SetHostOnCannotBeABaseUrl
        | ParseError::Overflow
        | _ => HttpUrlError::Parse(parsed_error),
    }
}

#[cfg(feature = "proptest")]
fn host_strategy() -> proptest::strategy::BoxedStrategy<String> {
    use proptest::strategy::Strategy as _;

    (label_strategy(), label_strategy())
        .prop_map(|(left, right)| {
            let mut out = String::with_capacity(left.len() + right.len() + ".test.".len());
            out.push_str(&left);
            out.push('.');
            out.push_str(&right);
            out.push_str(".test");
            out
        })
        .boxed()
}

#[cfg(feature = "proptest")]
fn label_strategy() -> proptest::strategy::BoxedStrategy<String> {
    use proptest::strategy::Strategy as _;

    (
        lower_alpha_strategy(),
        proptest::collection::vec(label_tail_char_strategy(), 0_usize..=8_usize),
    )
        .prop_map(|(first, tail)| {
            let mut out = String::with_capacity(1 + tail.len());
            out.push(first);
            for ch in tail {
                out.push(ch);
            }
            out
        })
        .boxed()
}

#[cfg(feature = "proptest")]
fn path_strategy() -> proptest::strategy::BoxedStrategy<String> {
    use proptest::strategy::Strategy as _;

    proptest::collection::vec(path_segment_strategy(), 0_usize..=3_usize)
        .prop_map(|segments| {
            let mut path = String::new();
            for segment in segments {
                path.push('/');
                path.push_str(&segment);
            }
            path
        })
        .boxed()
}

#[cfg(feature = "proptest")]
fn path_segment_strategy() -> proptest::strategy::BoxedStrategy<String> {
    use proptest::strategy::Strategy as _;

    proptest::collection::vec(path_char_strategy(), 0_usize..=8_usize)
        .prop_map(|chars| chars.into_iter().collect())
        .boxed()
}

#[cfg(feature = "proptest")]
fn lower_alpha_strategy() -> proptest::strategy::BoxedStrategy<char> {
    use proptest::strategy::Strategy as _;

    proptest::char::ranges(alloc::borrow::Cow::Owned(alloc::vec!['a'..='z'])).boxed()
}

#[cfg(feature = "proptest")]
fn label_tail_char_strategy() -> proptest::strategy::BoxedStrategy<char> {
    use proptest::strategy::Strategy as _;

    proptest::char::ranges(alloc::borrow::Cow::Owned(
        alloc::vec!['a'..='z', '0'..='9',],
    ))
    .boxed()
}

#[cfg(feature = "proptest")]
fn path_char_strategy() -> proptest::strategy::BoxedStrategy<char> {
    use proptest::strategy::Strategy as _;

    proptest::char::ranges(alloc::borrow::Cow::Owned(alloc::vec![
        'a'..='z',
        'A'..='Z',
        '0'..='9',
        '-'..='-',
        '_'..='_',
        '.'..='.',
    ]))
    .boxed()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use super::{HTTP_URL_MAX_LEN, HttpUrl, HttpUrlError, parse_error};
    use ::url::{ParseError, Url};
    use alloc::string::ToString as _;

    use crate::rule::{Refined, Rule};

    #[test]
    fn parse_admits_http_and_https_urls() {
        let http = HttpUrl::parse("http://example.com/path").unwrap();
        assert_eq!(http.as_inner().scheme(), "http");
        assert_eq!(http.as_inner().host_str(), Some("example.com"));

        let https = HttpUrl::parse("https://example.com/path?x=1").unwrap();
        assert_eq!(https.as_inner().scheme(), "https");
        assert_eq!(https.as_inner().query(), Some("x=1"));
    }

    #[test]
    fn parse_rejects_empty_before_parser() {
        let error = HttpUrl::parse("").unwrap_err();
        assert_eq!(error, HttpUrlError::Empty);
    }

    #[test]
    fn parse_rejects_overlong_before_parser() {
        let raw = alloc::format!("https://example.com/{}", "a".repeat(HTTP_URL_MAX_LEN));
        let error = HttpUrl::parse(&raw).unwrap_err();
        assert_eq!(
            error,
            HttpUrlError::TooLong {
                actual: raw.len(),
                max: HTTP_URL_MAX_LEN,
            },
        );
    }

    #[test]
    fn parse_rejects_parser_errors() {
        let error = HttpUrl::parse("://not-a-url").unwrap_err();
        assert_eq!(
            error,
            HttpUrlError::Parse(ParseError::RelativeUrlWithoutBase)
        );
    }

    #[test]
    fn parse_error_maps_empty_host_and_preserves_parser_variants() {
        assert_eq!(
            parse_error(ParseError::EmptyHost),
            HttpUrlError::MissingHost
        );

        for error in [
            ParseError::IdnaError,
            ParseError::InvalidPort,
            ParseError::InvalidIpv4Address,
            ParseError::InvalidIpv6Address,
            ParseError::InvalidDomainCharacter,
            ParseError::RelativeUrlWithoutBase,
            ParseError::RelativeUrlWithCannotBeABaseBase,
            ParseError::SetHostOnCannotBeABaseUrl,
            ParseError::Overflow,
        ] {
            assert_eq!(parse_error(error), HttpUrlError::Parse(error));
        }
    }

    #[test]
    fn refine_rejects_unsupported_scheme() {
        let parsed = Url::parse("ftp://example.com/file").unwrap();
        let error = Refined::<Url, HttpUrl>::try_new(parsed).unwrap_err();
        assert_eq!(error, HttpUrlError::UnsupportedScheme);
    }

    #[test]
    fn refine_rejects_missing_host() {
        let parsed = Url::parse("file:///tmp/socket").unwrap();
        let error = Refined::<Url, HttpUrl>::try_new(parsed).unwrap_err();
        assert_eq!(error, HttpUrlError::MissingHost);
    }

    #[test]
    fn refine_rejects_userinfo() {
        let parsed = Url::parse("https://user:pass@example.com/").unwrap();
        let error = Refined::<Url, HttpUrl>::try_new(parsed).unwrap_err();
        assert_eq!(error, HttpUrlError::HasUserinfo);
    }

    #[test]
    fn refine_rejects_fragment() {
        let parsed = Url::parse("https://example.com/path#section").unwrap();
        let error = Refined::<Url, HttpUrl>::try_new(parsed).unwrap_err();
        assert_eq!(error, HttpUrlError::HasFragment);
    }

    #[test]
    fn accepts_is_true_only_for_policy_admissible_urls() {
        let ok = Url::parse("https://example.com/path").unwrap();
        let bad = Url::parse("https://example.com/path#fragment").unwrap();

        assert!(HttpUrl::accepts(ok));
        assert!(!HttpUrl::accepts(bad));
    }

    #[test]
    fn display_messages_are_stable() {
        assert_eq!(HttpUrlError::Empty.to_string(), "url is empty");
        assert_eq!(
            HttpUrlError::TooLong { actual: 10, max: 8 }.to_string(),
            "url byte length 10 exceeds maximum 8",
        );
        assert_eq!(
            HttpUrlError::Parse(ParseError::RelativeUrlWithoutBase).to_string(),
            "url parse error: relative URL without a base",
        );
        assert_eq!(
            HttpUrlError::UnsupportedScheme.to_string(),
            "url scheme must be http or https",
        );
        assert_eq!(
            HttpUrlError::MissingHost.to_string(),
            "url must include a host",
        );
        assert_eq!(
            HttpUrlError::HasUserinfo.to_string(),
            "url must not include userinfo",
        );
        assert_eq!(
            HttpUrlError::HasFragment.to_string(),
            "url must not include a fragment",
        );
    }
}
