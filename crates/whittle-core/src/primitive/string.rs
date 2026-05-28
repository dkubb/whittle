//! String primitive rules.
//!
//! Length bounds (`LenChars`, `LenBytes`), non-empty marker, and
//! per-character predicates (`EachChar<P>`) with library-supplied
//! `CharPredicate` implementations.
//!
//! Each primitive is a `Rule<String>` (the inner type is owned `T:
//! 'static`); `&str`-based rules cannot satisfy the kernel's
//! `'static` bound and would conflict with the `Schema` reflection
//! that lands in a later commit.

use alloc::string::String;
use core::marker::PhantomData;

use thiserror::Error;

use crate::rule::Rule;

/// Inclusive bound on the number of Unicode scalar values: `MIN <=
/// chars.count() <= MAX`.
pub struct LenChars<const MIN: usize, const MAX: usize>;

/// Inclusive bound on the UTF-8 byte length: `MIN <= bytes.len() <= MAX`.
pub struct LenBytes<const MIN: usize, const MAX: usize>;

/// Rejects the empty string.
pub struct NonEmpty;

/// Every character must satisfy the predicate `P`.
pub struct EachChar<P>(PhantomData<P>);

/// The first character must satisfy the predicate `P`.
///
/// The empty string is admissible (there is no first character to
/// reject). Compose with a length bound — typically `LenChars<1,
/// MAX>` — when the caller wants to reject empty inputs.
///
/// Used to encode head/tail grammars: e.g. an identifier whose
/// first character is alpha or underscore, and whose remaining
/// characters are alphanumeric or underscore, is
/// `And<EachChar<IdentChar>, FirstChar<IdentStart>>`.
pub struct FirstChar<P>(PhantomData<P>);

/// Errors common to every string primitive.
///
/// `length` and `index` fields carry the offending observation so
/// callers can produce precise diagnostics.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StringError {
    /// `LenChars<MIN, MAX>` or `LenBytes<MIN, MAX>` declared with
    /// `MIN > MAX`. The interval is empty so no input is admissible.
    #[error("empty length range")]
    EmptyRange,

    /// Character count not in the admissible range.
    #[error("character count {actual} not in admissible range")]
    CharCountOutOfRange { actual: usize },

    /// Byte length not in the admissible range.
    #[error("byte length {actual} not in admissible range")]
    ByteLenOutOfRange { actual: usize },

    /// `NonEmpty` received an empty string.
    #[error("empty string")]
    Empty,

    /// `EachChar<P>` rejected a character at the given UTF-8 byte
    /// offset.
    #[error("character at byte offset {offset} not admissible")]
    BadChar { offset: usize },
}

/// A pure predicate over a single `char`.
///
/// Implementations are zero-sized type markers (no instance state)
/// so they compose cleanly with `EachChar<P>` and the future schema
/// reflection.
pub trait CharPredicate: 'static {
    /// Return `true` when `ch` is admitted by this predicate.
    fn test(ch: char) -> bool;
}

/// Predicate: ASCII alphanumeric (`A`–`Z`, `a`–`z`, `0`–`9`).
pub struct AsciiAlphanumeric;
impl CharPredicate for AsciiAlphanumeric {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphanumeric()
    }
}

/// Predicate: ASCII alphanumeric or underscore. Matches the usual
/// identifier-body grammar.
pub struct IdentChar;
impl CharPredicate for IdentChar {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_'
    }
}

/// Predicate: ASCII alphabetic or underscore. Matches the usual
/// identifier-head grammar (leading digits are excluded).
pub struct IdentStart;
impl CharPredicate for IdentStart {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_'
    }
}

/// Predicate: not a Unicode control character.
pub struct NonControl;
impl CharPredicate for NonControl {
    #[inline]
    fn test(ch: char) -> bool {
        !ch.is_control()
    }
}

// ─── Rule impls. ──────────────────────────────────────────────────

impl<const MIN: usize, const MAX: usize> Rule<String> for LenChars<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        if MIN > MAX {
            return Err(StringError::EmptyRange);
        }
        let actual = raw.chars().count();
        if actual < MIN || actual > MAX {
            return Err(StringError::CharCountOutOfRange { actual });
        }
        Ok(raw)
    }
}

impl<const MIN: usize, const MAX: usize> Rule<String> for LenBytes<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        if MIN > MAX {
            return Err(StringError::EmptyRange);
        }
        let actual = raw.len();
        if actual < MIN || actual > MAX {
            return Err(StringError::ByteLenOutOfRange { actual });
        }
        Ok(raw)
    }
}

impl Rule<String> for NonEmpty {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        if raw.is_empty() {
            return Err(StringError::Empty);
        }
        Ok(raw)
    }
}

impl<P: CharPredicate> Rule<String> for EachChar<P> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        for (offset, ch) in raw.char_indices() {
            if !P::test(ch) {
                return Err(StringError::BadChar { offset });
            }
        }
        Ok(raw)
    }
}

impl<P: CharPredicate> Rule<String> for FirstChar<P> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        if let Some(ch) = raw.chars().next()
            && !P::test(ch)
        {
            return Err(StringError::BadChar { offset: 0 });
        }
        Ok(raw)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{
        AsciiAlphanumeric, EachChar, FirstChar, IdentChar, IdentStart,
        LenBytes, LenChars, NonControl, NonEmpty, StringError,
    };
    use crate::composition::And;
    use crate::rule::Refined;

    #[test]
    fn len_chars_inclusive_bounds() {
        let one: Refined<String, LenChars<1, 5>>
            = Refined::try_new("a".to_string()).unwrap();
        assert_eq!(one.as_inner(), "a");
        let five: Refined<String, LenChars<1, 5>>
            = Refined::try_new("ABCDE".to_string()).unwrap();
        assert_eq!(five.as_inner(), "ABCDE");
    }

    #[test]
    fn len_chars_rejects_too_short() {
        let result: Result<Refined<String, LenChars<2, 5>>, _>
            = Refined::try_new("a".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::CharCountOutOfRange { actual: 1 },
        );
    }

    #[test]
    fn len_chars_rejects_too_long() {
        let result: Result<Refined<String, LenChars<1, 3>>, _>
            = Refined::try_new("abcd".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::CharCountOutOfRange { actual: 4 },
        );
    }

    #[test]
    fn len_chars_counts_unicode_scalar_values_not_bytes() {
        // "é" is one Unicode scalar value but two UTF-8 bytes.
        let one: Refined<String, LenChars<1, 1>>
            = Refined::try_new("é".to_string()).unwrap();
        assert_eq!(one.as_inner(), "é");
    }

    #[test]
    fn len_bytes_counts_bytes_not_scalars() {
        // "é" is two UTF-8 bytes; rule with MAX=1 must reject.
        let result: Result<Refined<String, LenBytes<1, 1>>, _>
            = Refined::try_new("é".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::ByteLenOutOfRange { actual: 2 },
        );
    }

    #[test]
    fn non_empty_rejects_empty() {
        let result: Result<Refined<String, NonEmpty>, _>
            = Refined::try_new(String::new());
        assert_eq!(result.unwrap_err(), StringError::Empty);
    }

    #[test]
    fn non_empty_accepts_nonempty() {
        let r: Refined<String, NonEmpty>
            = Refined::try_new("x".to_string()).unwrap();
        assert_eq!(r.as_inner(), "x");
    }

    #[test]
    fn each_char_accepts_uniform_predicate() {
        let r: Refined<String, EachChar<AsciiAlphanumeric>>
            = Refined::try_new("user42".to_string()).unwrap();
        assert_eq!(r.as_inner(), "user42");
    }

    #[test]
    fn each_char_reports_offset_of_first_violation() {
        let result: Result<Refined<String, EachChar<AsciiAlphanumeric>>, _>
            = Refined::try_new("user-42".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::BadChar { offset: 4 },
        );
    }

    #[test]
    fn ident_char_admits_alnum_and_underscore() {
        let r: Refined<String, EachChar<IdentChar>>
            = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(r.as_inner(), "user_42");

        let bad: Result<Refined<String, EachChar<IdentChar>>, _>
            = Refined::try_new("user.42".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 4 });
    }

    // ─── FirstChar / IdentStart. ─────────────────────────────────

    #[test]
    fn first_char_accepts_admissible_head() {
        let r: Refined<String, FirstChar<IdentStart>>
            = Refined::try_new("name".to_string()).unwrap();
        assert_eq!(r.as_inner(), "name");
    }

    #[test]
    fn first_char_admits_empty_string() {
        // Empty string carries no first character. Composition with
        // a length bound is the way to forbid empty; FirstChar
        // itself stays minimal.
        let r: Refined<String, FirstChar<IdentStart>>
            = Refined::try_new(String::new()).unwrap();
        assert!(r.as_inner().is_empty());
    }

    #[test]
    fn first_char_rejects_inadmissible_head() {
        let result: Result<Refined<String, FirstChar<IdentStart>>, _>
            = Refined::try_new("1abc".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::BadChar { offset: 0 },
        );
    }

    #[test]
    fn ident_grammar_via_composition_rejects_leading_digit() {
        // A real identifier grammar: leading char alpha/underscore,
        // rest alnum/underscore.
        type IdentRule = And<EachChar<IdentChar>, FirstChar<IdentStart>>;
        let good: Refined<String, IdentRule>
            = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(good.as_inner(), "user_42");

        let bad: Result<Refined<String, IdentRule>, _>
            = Refined::try_new("1abc".to_string());
        assert!(bad.is_err());
    }

    #[test]
    fn non_control_admits_printable_unicode() {
        let r: Refined<String, EachChar<NonControl>>
            = Refined::try_new("hello world! éé".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello world! éé");

        let bad: Result<Refined<String, EachChar<NonControl>>, _>
            = Refined::try_new("a\tb".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    proptest::proptest! {
        #[test]
        fn len_chars_round_trips_in_range(
            s in "[a-z]{1,10}"
        ) {
            let r: Refined<String, LenChars<1, 10>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }
    }
}
