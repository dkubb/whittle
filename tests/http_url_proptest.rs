//! Constructive proptest strategy for parsed HTTP URLs.
//!
//! The `HttpUrl` strategy builds admissible URLs directly: `http` or
//! `https`, host present, no userinfo, no fragment, and under the
//! pre-parse cap. It does not depend on downstream rejection sampling.

use proptest::proptest;
use url::Url;
use whittle::primitive::{HTTP_URL_MAX_LEN, HttpUrl};
use whittle::{ArbitraryRule, Refined};

const fn assert_rule<T: 'static, R>()
where
    R: ArbitraryRule<T>,
{
}

#[test]
fn http_url_has_an_arbitrary_rule_impl() {
    assert_rule::<Url, HttpUrl>();
}

#[test]
fn arbitrary_http_url_values_are_admissible_by_construction() {
    proptest!(|(url in proptest::arbitrary::any::<Refined<Url, HttpUrl>>())| {
        let inner = url.as_inner();
        assert!(matches!(inner.scheme(), "http" | "https"));
        assert!(inner.host().is_some());
        assert!(inner.username().is_empty());
        assert!(inner.password().is_none());
        assert!(inner.fragment().is_none());
        assert!(inner.as_str().len() <= HTTP_URL_MAX_LEN);
    });
}
