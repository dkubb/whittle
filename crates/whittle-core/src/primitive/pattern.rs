//! Regular-expression pattern primitive rule.
//!
//! `Pattern<RE>` carries its regular expression in the type as a
//! `&'static str` const generic. It is the escape hatch for positional
//! grammars that the composable character-class rules (`EachChar`,
//! `FirstChar`, `CharEither`, ...) cannot express ergonomically — for
//! example "an uppercase initial followed by dash-separated alphabetic
//! runs".
//!
//! `Pattern` is **always a whole-string match**: a candidate is
//! admissible only when the regex matches the entire input, even if the
//! pattern was written without `^`/`$` anchors (the rule enforces the
//! full span itself). Anchored patterns are accepted too; the explicit
//! span check is belt-and-suspenders and makes unanchored patterns
//! behave identically.
//!
//! This rule needs `std` (for the regex crate and the keyed compile
//! cache) and the nightly `adt_const_params` / `unsized_const_params`
//! features that let a `&'static str` be a const generic. It is gated
//! behind the `regex` cargo feature; the default kernel stays
//! `#![no_std]` and pulls in neither `std` nor the regex dependency.
//!
//! ## Malformed patterns
//!
//! A bare `Pattern<RE>` with a malformed `RE` **panics on first
//! construction** (when its regex is first compiled). The
//! [`pattern!`](crate::pattern) macro turns that runtime panic into a
//! compile error: prefer `pattern!(r"...")` over writing
//! `Pattern<"...">` by hand.

use std::boxed::Box;
use std::collections::HashMap;
use std::string::String;
use std::sync::{Mutex, OnceLock};

use regex::Regex;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// A whole-string regular-expression rule whose pattern is carried in
/// the type as a `&'static str` const generic.
///
/// `Pattern<RE>` admits a `String` only when the regex `RE` matches the
/// **entire** input. See the [module docs](self) for the whole-string
/// semantics and the malformed-pattern panic behaviour.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{Pattern, PatternError};
///
/// // An uppercase initial followed by dash-separated alphabetic runs.
/// type Name = Pattern<r"^(?:[A-Z])(?:-?[A-Za-z]+)*$">;
///
/// // Admit: matches the whole string.
/// let ok: Refined<String, Name> = Refined::try_new("A-Bc-De".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "A-Bc-De");
///
/// // Reject: does not match.
/// let err = Refined::<String, Name>::try_new("abc".to_string()).unwrap_err();
/// assert_eq!(err, PatternError::NoMatch);
/// ```
pub struct Pattern<const RE: &'static str>;

/// Error produced when a `String` does not match its `Pattern<RE>`.
///
/// The error is deliberately opaque: it carries neither the pattern nor
/// the offending input, because the pattern is already encoded in the
/// rule type and reproducing arbitrary user input in an error payload
/// is a footgun. This is one cost of reaching for `Pattern` over the
/// structured character-class rules, whose errors pinpoint the failing
/// character.
#[derive(Debug, PartialEq, Eq)]
pub enum PatternError {
    /// The input did not match the pattern over its whole span.
    NoMatch,
}

impl core::fmt::Display for PatternError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::NoMatch => f.write_str("input does not match the required pattern"),
        }
    }
}

impl core::error::Error for PatternError {}

/// Per-process cache of compiled regexes, keyed by the pattern string.
///
/// Each distinct `RE` const generic is its own monomorphization of
/// `Pattern<RE>`, but Rust offers no per-monomorphization `static`, so a
/// single keyed global serves every instantiation. The key is the
/// `&'static str` pattern itself; the value is a `&'static Regex` leaked
/// from a `Box` so the borrow outlives the lock guard.
type Cache = OnceLock<Mutex<HashMap<&'static str, &'static Regex>>>;
static CACHE: Cache = OnceLock::new();

/// Return the compiled `Regex` for `re`, compiling and caching it on
/// first use.
///
/// # Panics
///
/// Panics if `re` is not a valid regular expression. A bare
/// `Pattern<RE>` therefore panics the first time it is constructed with
/// a malformed `RE`; the [`pattern!`](crate::pattern) macro validates
/// the pattern at compile time so this path is unreachable through the
/// macro front door.
fn compiled(re: &'static str) -> &'static Regex {
    let mut map = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("pattern cache mutex poisoned by an earlier panic");
    if let Some(found) = map.get(re) {
        return found;
    }
    let compiled: &'static Regex = Box::leak(Box::new(
        Regex::new(re).expect("malformed regex in `Pattern<RE>`; use `pattern!` to validate it"),
    ));
    map.insert(re, compiled);
    compiled
}

impl<const RE: &'static str> Rule<String> for Pattern<RE> {
    type Error = PatternError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        let re = compiled(RE);
        // Whole-string semantics: a match must span the entire input.
        // `find` returns the leftmost match; requiring it to start at 0
        // and end at `raw.len()` rejects prefix/substring matches even
        // when `RE` itself is unanchored.
        match re.find(&raw) {
            Some(m) if m.start() == 0 && m.end() == raw.len() => Ok(raw),
            _ => Err(PatternError::NoMatch),
        }
    }
}

// ─── Serde `DeserializeRule` impl: default parse-then-refine. ─────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const RE: &'static str] DeserializeRule<String> for Pattern<RE>
}

// ─── `ArbitraryRule` impl. ────────────────────────────────────────

#[cfg(feature = "proptest")]
impl<const RE: &'static str> ArbitraryRule<String> for Pattern<RE> {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // proptest's `string_regex` rejects zero-width anchors
        // (`^`/`$`) with "anchors/boundaries not supported for string
        // generation". Strip a leading `^` and trailing `$` before
        // handing the pattern to the generator: because `Pattern` is a
        // whole-string match, the generated language is exactly the
        // anchored language, so dropping the anchors is sound and the
        // generated values still satisfy `Pattern::<RE>::refine`. (The
        // blanket `Arbitrary` impl's `expect` would panic if any
        // generated value drifted out of the admissible set.)
        let unanchored = RE.strip_prefix('^').unwrap_or(RE);
        let unanchored = unanchored.strip_suffix('$').unwrap_or(unanchored);
        proptest::string::string_regex(unanchored)
            .expect("pattern is a valid regex for string generation once anchors are stripped")
            .boxed()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
#[expect(
    clippy::needless_raw_strings,
    reason = "regex patterns stay raw strings so a later edit adding a backslash escape \
              does not silently change their meaning"
)]
mod tests {
    use std::string::{String, ToString as _};

    use super::{Pattern, PatternError};
    use crate::rule::Refined;

    /// The user's worked example: an uppercase initial followed by
    /// dash-separated alphabetic runs. Anchored, so it doubles as the
    /// anchor-stripping exercise for `arbitrary_strategy`.
    type Name = Pattern<r"^(?:[A-Z])(?:-?[A-Za-z]+)*$">;

    /// A second, distinct, *unanchored* pattern. Two distinct `RE`
    /// const generics give the generic `refine` body two
    /// monomorphizations (covering its regions), and the unanchored
    /// shape drives the whole-string span check.
    type Digits = Pattern<r"[0-9]+">;

    #[test]
    fn admits_whole_string_match() {
        let r: Refined<String, Name> = Refined::try_new("A-Bc-De".to_string()).unwrap();
        assert_eq!(r.as_inner(), "A-Bc-De");
    }

    #[test]
    fn rejects_non_match() {
        let result: Result<Refined<String, Name>, _> = Refined::try_new("abc".to_string());
        assert_eq!(result.unwrap_err(), PatternError::NoMatch);
    }

    #[test]
    fn rejects_prefix_only_match() {
        // `[0-9]+` matches the `12` prefix of `12x`, but not the whole
        // string. The whole-string span check must reject it, covering
        // the `_ => Err(..)` arm reached via a `Some(m)` that fails the
        // span guard.
        let result: Result<Refined<String, Digits>, _> = Refined::try_new("12x".to_string());
        assert_eq!(result.unwrap_err(), PatternError::NoMatch);
    }

    #[test]
    fn rejects_suffix_only_match() {
        // `[0-9]+` matches the `12` *suffix* of `x12`, so the leftmost
        // match starts at offset 1, not 0. This fails the first operand
        // of the `m.start() == 0 && ..` span guard, covering the branch
        // that `rejects_prefix_only_match` (which fails the *second*
        // operand) does not.
        let result: Result<Refined<String, Digits>, _> = Refined::try_new("x12".to_string());
        assert_eq!(result.unwrap_err(), PatternError::NoMatch);
    }

    #[test]
    fn admits_unanchored_whole_string_match() {
        // Same unanchored pattern, but the input is all digits, so the
        // match spans the whole string and is admitted.
        let r: Refined<String, Digits> = Refined::try_new("12345".to_string()).unwrap();
        assert_eq!(r.as_inner(), "12345");
    }

    #[test]
    fn cache_hit_on_repeated_pattern() {
        // First construction compiles and caches `Name`'s regex (miss);
        // the second hits the cache. Both must succeed, exercising both
        // branches of `compiled` for a single `RE`.
        let first: Refined<String, Name> = Refined::try_new("Aa".to_string()).unwrap();
        let second: Refined<String, Name> = Refined::try_new("Bb".to_string()).unwrap();
        assert_eq!(first.as_inner(), "Aa");
        assert_eq!(second.as_inner(), "Bb");
    }

    #[test]
    fn display_and_error_impl() {
        assert_eq!(
            PatternError::NoMatch.to_string(),
            "input does not match the required pattern",
        );
        let dyn_err: &dyn core::error::Error = &PatternError::NoMatch;
        assert!(dyn_err.source().is_none());
    }

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        /// Every value generated for `Refined<String, Name>` satisfies
        /// the anchored pattern over its whole span. Covers
        /// `arbitrary_strategy` (including the anchor-stripping path)
        /// and the blanket `Arbitrary` -> `try_new` round-trip.
        #[test]
        fn arbitrary_name_is_admissible(
            r in proptest::arbitrary::any::<Refined<String, Name>>()
        ) {
            // Re-running the rule on the inner value must accept it.
            let inner = r.as_inner().clone();
            let again: Refined<String, Name> = Refined::try_new(inner).unwrap();
            proptest::prop_assert_eq!(again.as_inner(), r.as_inner());
        }
    }
}
