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

use crate::rule::Rule;

/// Inclusive bound on the number of Unicode scalar values: `MIN <=
/// chars.count() <= MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{LenChars, StringError};
///
/// // Admit: 3 chars within `1..=5`.
/// let ok: Refined<String, LenChars<1, 5>>
///     = Refined::try_new("abc".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "abc");
///
/// // Reject: too many chars.
/// let err = Refined::<String, LenChars<1, 5>>::try_new(
///     "abcdef".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::CharCountOutOfRange { actual: 6 });
/// ```
pub struct LenChars<const MIN: usize, const MAX: usize>;

/// Inclusive bound on the UTF-8 byte length: `MIN <= bytes.len() <= MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{LenBytes, StringError};
///
/// // Admit: "hello" is 5 bytes, within `1..=5`.
/// let ok: Refined<String, LenBytes<1, 5>>
///     = Refined::try_new("hello".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "hello");
///
/// // Reject: "é" is two UTF-8 bytes but the cap is one byte.
/// let err = Refined::<String, LenBytes<1, 1>>::try_new(
///     "é".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::ByteLenOutOfRange { actual: 2 });
/// ```
pub struct LenBytes<const MIN: usize, const MAX: usize>;

/// Rejects the empty string.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NonEmpty, StringError};
///
/// let ok: Refined<String, NonEmpty>
///     = Refined::try_new("x".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "x");
///
/// let err = Refined::<String, NonEmpty>::try_new(String::new())
///     .unwrap_err();
/// assert_eq!(err, StringError::Empty);
/// ```
pub struct NonEmpty;

/// Every character must satisfy the predicate `P`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     AsciiAlphanumeric, EachChar, StringError,
/// };
///
/// // Admit: every character is ASCII alphanumeric.
/// let ok: Refined<String, EachChar<AsciiAlphanumeric>>
///     = Refined::try_new("user42".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "user42");
///
/// // Reject: `-` violates the predicate at byte offset 4.
/// let err = Refined::<String, EachChar<AsciiAlphanumeric>>::try_new(
///     "user-42".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 4 });
/// ```
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
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{FirstChar, IdentStart, StringError};
///
/// // Admit: starts with an alphabetic character.
/// let ok: Refined<String, FirstChar<IdentStart>>
///     = Refined::try_new("name".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "name");
///
/// // Reject: leading digit fails the head predicate.
/// let err = Refined::<String, FirstChar<IdentStart>>::try_new(
///     "1abc".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 0 });
/// ```
pub struct FirstChar<P>(PhantomData<P>);

/// Errors common to every string primitive.
///
/// `length` and `index` fields carry the offending observation so
/// callers can produce precise diagnostics. Invalid rule
/// configurations (e.g. `LenChars<MIN, MAX>` with `MIN > MAX`)
/// are rejected at compile time via `const { assert!(...) }`
/// blocks inside `Rule::refine`, so their error variant is
/// unrepresentable.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StringError {
    /// Character count not in the admissible range.
    CharCountOutOfRange { actual: usize },

    /// Byte length not in the admissible range.
    ByteLenOutOfRange { actual: usize },

    /// `NonEmpty` received an empty string.
    Empty,

    /// `EachChar<P>` rejected a character at the given UTF-8 byte
    /// offset.
    BadChar { offset: usize },

    /// `HexFixedLower<LEN>` / `HexFixedAny<LEN>` saw a string
    /// whose length is not the configured `LEN`. Distinct from
    /// `CharCountOutOfRange` to preserve the fixed-length /
    /// range-bound distinction.
    BadHexLength { actual: usize },
}

impl core::fmt::Display for StringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::CharCountOutOfRange { actual } => {
                write!(f, "character count {actual} not in admissible range")
            }
            Self::ByteLenOutOfRange { actual } => {
                write!(f, "byte length {actual} not in admissible range")
            }
            Self::Empty => f.write_str("empty string"),
            Self::BadChar { offset } => {
                write!(f, "character at byte offset {offset} not admissible")
            }
            Self::BadHexLength { actual } => write!(
                f,
                "hex string length {actual} does not match expected length",
            ),
        }
    }
}

impl core::error::Error for StringError {}

/// A pure predicate over a single `char`.
///
/// Implementations are zero-sized type markers (no instance state)
/// so they compose cleanly with `EachChar<P>` and the future schema
/// reflection.
pub trait CharPredicate: 'static {
    /// Return `true` when `ch` is admitted by this predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::{
    ///     AsciiAlphanumeric, CharPredicate, IdentStart,
    /// };
    ///
    /// // Library-supplied predicates can be queried directly.
    /// assert!(<AsciiAlphanumeric as CharPredicate>::test('A'));
    /// assert!(!<AsciiAlphanumeric as CharPredicate>::test('-'));
    ///
    /// // Custom predicate: ASCII vowels only.
    /// pub struct Vowel;
    /// impl CharPredicate for Vowel {
    ///     fn test(ch: char) -> bool {
    ///         matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u')
    ///     }
    /// }
    /// assert!(<Vowel as CharPredicate>::test('a'));
    /// assert!(!<Vowel as CharPredicate>::test('z'));
    /// // Identifier-head predicate: leading digits excluded.
    /// assert!(!<IdentStart as CharPredicate>::test('1'));
    /// ```
    fn test(ch: char) -> bool;
}

/// Predicate: ASCII alphanumeric (`A`–`Z`, `a`–`z`, `0`–`9`).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     AsciiAlphanumeric, EachChar, StringError,
/// };
///
/// let ok: Refined<String, EachChar<AsciiAlphanumeric>>
///     = Refined::try_new("user42".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "user42");
///
/// let err = Refined::<String, EachChar<AsciiAlphanumeric>>::try_new(
///     "user 42".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 4 });
/// ```
pub struct AsciiAlphanumeric;
impl CharPredicate for AsciiAlphanumeric {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphanumeric()
    }
}

/// Predicate: ASCII alphanumeric or underscore. Matches the usual
/// identifier-body grammar.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, IdentChar, StringError};
///
/// let ok: Refined<String, EachChar<IdentChar>>
///     = Refined::try_new("user_42".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "user_42");
///
/// let err = Refined::<String, EachChar<IdentChar>>::try_new(
///     "user.42".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 4 });
/// ```
pub struct IdentChar;
impl CharPredicate for IdentChar {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_'
    }
}

/// Predicate: ASCII alphabetic or underscore. Matches the usual
/// identifier-head grammar (leading digits are excluded).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, IdentStart, StringError};
///
/// // Admit: every char passes the head predicate.
/// let ok: Refined<String, EachChar<IdentStart>>
///     = Refined::try_new("name".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "name");
///
/// // Reject: digits are not admitted as identifier heads.
/// let err = Refined::<String, EachChar<IdentStart>>::try_new(
///     "abc1".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 3 });
/// ```
pub struct IdentStart;
impl CharPredicate for IdentStart {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_'
    }
}

/// Predicate: not a Unicode control character.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, NonControl, StringError};
///
/// let ok: Refined<String, EachChar<NonControl>>
///     = Refined::try_new("hello world".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "hello world");
///
/// let err = Refined::<String, EachChar<NonControl>>::try_new(
///     "a\tb".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 1 });
/// ```
pub struct NonControl;
impl CharPredicate for NonControl {
    #[inline]
    fn test(ch: char) -> bool {
        !ch.is_control()
    }
}

/// Predicate: ASCII hexadecimal digit (`0`–`9`, `a`–`f`, `A`–`F`).
///
/// Composes with `EachChar<HexChar>` + `LenChars<N, N>` to express
/// the standard "N-character hex string" shape (e.g. Git SHA-1 as
/// 40 hex chars, BLAKE3 digests as 64 hex chars).
///
/// Available behind the `hex` feature.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, HexChar, StringError};
///
/// let ok: Refined<String, EachChar<HexChar>>
///     = Refined::try_new("0aF9".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "0aF9");
///
/// let err = Refined::<String, EachChar<HexChar>>::try_new(
///     "0a1g".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 3 });
/// # }
/// ```
#[cfg(feature = "hex")]
pub struct HexChar;

#[cfg(feature = "hex")]
impl CharPredicate for HexChar {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_hexdigit()
    }
}

/// Predicate: visible single-line character.
///
/// Rejects every Unicode control character (`Cc`, which includes
/// `\n`, `\r`, `\t`, the C0/C1 control range) and the most
/// common invisible / format characters that `is_control()` does
/// not catch: zero-width space, zero-width joiner /
/// non-joiner, word joiner, soft hyphen, and the byte-order mark.
///
/// Designed for the "displayable on a single line, no surprise
/// whitespace" check that user-facing identifiers and labels
/// need. For full `Cf`/`Co`/`Cn` Unicode-category classification,
/// compose with a `unicode-properties`-based predicate.
///
/// Available behind the `unicode` feature.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "unicode")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, PrintableLine, StringError};
///
/// let ok: Refined<String, EachChar<PrintableLine>>
///     = Refined::try_new("Hello, world!".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "Hello, world!");
///
/// // Reject: newline is a control character.
/// let err = Refined::<String, EachChar<PrintableLine>>::try_new(
///     "line1\nline2".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 5 });
/// # }
/// ```
#[cfg(feature = "unicode")]
pub struct PrintableLine;

#[cfg(feature = "unicode")]
impl CharPredicate for PrintableLine {
    #[inline]
    fn test(ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        !matches!(
            ch,
            '\u{00AD}'   // soft hyphen
            | '\u{200B}' // zero-width space
            | '\u{200C}' // zero-width non-joiner
            | '\u{200D}' // zero-width joiner
            | '\u{2060}' // word joiner
            | '\u{FEFF}' // BOM / zero-width no-break space
        )
    }
}

/// Predicate: `PrintableLine` but admits `\n` (newline).
///
/// For multi-line free-form text — commit messages, doc
/// comments, descriptions — where newlines are part of the
/// content but other control characters and invisible
/// formatting characters are still rejected.
///
/// Available behind the `unicode` feature.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "unicode")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     EachChar, PrintableMultiline, StringError,
/// };
///
/// // Admit: newlines are allowed.
/// let ok: Refined<String, EachChar<PrintableMultiline>>
///     = Refined::try_new("line1\nline2".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "line1\nline2");
///
/// // Reject: tab is still a control character.
/// let err = Refined::<String, EachChar<PrintableMultiline>>::try_new(
///     "a\tb".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 1 });
/// # }
/// ```
#[cfg(feature = "unicode")]
pub struct PrintableMultiline;

#[cfg(feature = "unicode")]
impl CharPredicate for PrintableMultiline {
    #[inline]
    fn test(ch: char) -> bool {
        ch == '\n' || PrintableLine::test(ch)
    }
}

/// Predicate: ASCII alphanumeric, underscore, or `-`. Matches
/// `cargo`-package-name and DNS-label body grammars (leading `-`
/// must be excluded separately via `FirstChar`).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EachChar, IdentDashChar, StringError};
///
/// // Admit: every char is alnum, underscore, or `-`.
/// let ok: Refined<String, EachChar<IdentDashChar>>
///     = Refined::try_new("my-crate_42".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "my-crate_42");
///
/// // Reject: `.` is not admissible.
/// let err = Refined::<String, EachChar<IdentDashChar>>::try_new(
///     "my.crate".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 2 });
/// ```
pub struct IdentDashChar;
impl CharPredicate for IdentDashChar {
    #[inline]
    fn test(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
    }
}

/// Fixed-length lowercase hex string: exactly `LEN` characters, each
/// `0`–`9` or `a`–`f`. Available behind the `hex` feature.
///
/// Encodes the canonical lowercase rendering of fixed-width digests:
/// `HexFixedLower<40>` is the SHA-1 shape, `HexFixedLower<64>` is the
/// BLAKE3 / SHA-256 shape, `HexFixedLower<128>` is the SHA-512 shape.
///
/// `LEN` is required to be even at compile time via
/// `const { assert!(...) }` (a hex pair encodes one byte). `LEN == 0`
/// is admitted; callers wanting to reject the empty string can
/// compose with `NonEmpty` or pick a non-zero `LEN`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::{HexFixedLower, StringError};
///
/// // Admit: 40 lowercase hex characters (SHA-1 shape).
/// let ok: Refined<String, HexFixedLower<40>>
///     = Refined::try_new("a".repeat(40)).unwrap();
/// assert_eq!(ok.as_inner().len(), 40);
///
/// // Reject: wrong length.
/// let err = Refined::<String, HexFixedLower<40>>::try_new(
///     "a".repeat(39),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadHexLength { actual: 39 });
///
/// // Reject: uppercase character is not admissible under the
/// // lowercase-only variant.
/// let err = Refined::<String, HexFixedLower<4>>::try_new(
///     "0aFB".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 2 });
/// # }
/// ```
#[cfg(feature = "hex")]
pub struct HexFixedLower<const LEN: usize>;

/// Fixed-length mixed-case hex string: exactly `LEN` characters,
/// each `0`–`9`, `a`–`f`, or `A`–`F`. Available behind the `hex`
/// feature.
///
/// Use when accepted input may be either case (the typical
/// hand-written / pasted-hash case). For canonical lowercase-only
/// rendering, use `HexFixedLower<LEN>` instead.
///
/// `LEN` is required to be even at compile time via
/// `const { assert!(...) }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::{HexFixedAny, StringError};
///
/// // Admit: mixed-case 4-char hex string.
/// let ok: Refined<String, HexFixedAny<4>>
///     = Refined::try_new("0aFB".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "0aFB");
///
/// // Reject: wrong length.
/// let err = Refined::<String, HexFixedAny<4>>::try_new(
///     "0aF".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadHexLength { actual: 3 });
///
/// // Reject: `g` is outside the hex alphabet.
/// let err = Refined::<String, HexFixedAny<4>>::try_new(
///     "0a1g".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, StringError::BadChar { offset: 3 });
/// # }
/// ```
#[cfg(feature = "hex")]
pub struct HexFixedAny<const LEN: usize>;

/// Fixed-length hex string normalized to canonical lowercase:
/// accepts any case on input, stores the lowercase form.
///
/// Alias for `AsciiLowercase<HexFixedAny<LEN>>`. This is the
/// headline transformer use case: hex hashes can be hand-written
/// or pasted in either case, but the canonical wire / storage form
/// is lowercase. Available behind the `hex` feature.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::HexFixedNormalized;
///
/// // Admit mixed case, store lowercase.
/// let r: Refined<String, HexFixedNormalized<4>>
///     = Refined::try_new("AbCd".to_string()).unwrap();
/// assert_eq!(r.as_inner(), "abcd");
/// # }
/// ```
#[cfg(feature = "hex")]
pub type HexFixedNormalized<const LEN: usize> =
    crate::transform::AsciiLowercase<HexFixedAny<LEN>>;

// ─── Rule impls. ──────────────────────────────────────────────────

impl<const MIN: usize, const MAX: usize> Rule<String> for LenChars<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { assert!(MIN <= MAX, "LenChars requires MIN <= MAX") };
        let actual = raw.chars().count();
        if !(MIN..=MAX).contains(&actual) {
            return Err(StringError::CharCountOutOfRange { actual });
        }
        Ok(raw)
    }
}

impl<const MIN: usize, const MAX: usize> Rule<String> for LenBytes<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { assert!(MIN <= MAX, "LenBytes requires MIN <= MAX") };
        let actual = raw.len();
        if !(MIN..=MAX).contains(&actual) {
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

#[cfg(feature = "hex")]
impl<const LEN: usize> Rule<String> for HexFixedLower<LEN> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const {
            assert!(
                LEN.is_multiple_of(2),
                "HexFixedLower requires LEN to be even (one byte = two hex chars)",
            );
        }
        // ASCII-only alphabet: byte length equals char count.
        let actual = raw.len();
        if actual != LEN {
            return Err(StringError::BadHexLength { actual });
        }
        for (offset, byte) in raw.bytes().enumerate() {
            // Lowercase only: `0`–`9` or `a`–`f`.
            if !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
                return Err(StringError::BadChar { offset });
            }
        }
        Ok(raw)
    }
}

#[cfg(feature = "hex")]
impl<const LEN: usize> Rule<String> for HexFixedAny<LEN> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const {
            assert!(
                LEN.is_multiple_of(2),
                "HexFixedAny requires LEN to be even (one byte = two hex chars)",
            );
        }
        // ASCII-only alphabet: byte length equals char count.
        let actual = raw.len();
        if actual != LEN {
            return Err(StringError::BadHexLength { actual });
        }
        for (offset, byte) in raw.bytes().enumerate() {
            if !byte.is_ascii_hexdigit() {
                return Err(StringError::BadChar { offset });
            }
        }
        Ok(raw)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{
        AsciiAlphanumeric, CharPredicate, EachChar, FirstChar, IdentChar, IdentDashChar,
        IdentStart, LenBytes, LenChars, NonControl, NonEmpty, StringError,
    };
    use crate::composition::And;
    use crate::rule::Refined;

    refinement! {
        /// Macro-generated newtype for testing: short ASCII alnum
        /// label, 1..=10 chars. Exercises `refinement!` from the
        /// string primitive test module.
        #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
        pub TestLabel:
            String,
            And<LenChars<1, 10>, EachChar<AsciiAlphanumeric>>;
    }

    #[test]
    fn len_chars_inclusive_bounds() {
        let one: Refined<String, LenChars<1, 5>> = Refined::try_new("a".to_string()).unwrap();
        assert_eq!(one.as_inner(), "a");
        let five: Refined<String, LenChars<1, 5>> = Refined::try_new("ABCDE".to_string()).unwrap();
        assert_eq!(five.as_inner(), "ABCDE");
    }

    #[test]
    fn len_chars_rejects_too_short() {
        let result: Result<Refined<String, LenChars<2, 5>>, _> = Refined::try_new("a".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::CharCountOutOfRange { actual: 1 },
        );
    }

    #[test]
    fn len_chars_rejects_too_long() {
        let result: Result<Refined<String, LenChars<1, 3>>, _> =
            Refined::try_new("abcd".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::CharCountOutOfRange { actual: 4 },
        );
    }

    #[test]
    fn len_chars_counts_unicode_scalar_values_not_bytes() {
        // "é" is one Unicode scalar value but two UTF-8 bytes.
        let one: Refined<String, LenChars<1, 1>> = Refined::try_new("é".to_string()).unwrap();
        assert_eq!(one.as_inner(), "é");
    }

    #[test]
    fn len_bytes_counts_bytes_not_scalars() {
        // "é" is two UTF-8 bytes; rule with MAX=1 must reject.
        let result: Result<Refined<String, LenBytes<1, 1>>, _> = Refined::try_new("é".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::ByteLenOutOfRange { actual: 2 },
        );
    }

    #[test]
    fn len_bytes_admits_within_range() {
        let r: Refined<String, LenBytes<1, 5>> = Refined::try_new("hello".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello");
    }

    #[test]
    fn len_bytes_rejects_too_short() {
        // Exercises the below-min branch of LenBytes' range check
        // (the `actual < MIN` short-circuit) — distinct from
        // `len_bytes_counts_bytes_not_scalars`, which only covers
        // the above-max branch.
        let result: Result<Refined<String, LenBytes<5, 10>>, _> =
            Refined::try_new("ab".to_string());
        assert_eq!(
            result.unwrap_err(),
            StringError::ByteLenOutOfRange { actual: 2 },
        );
    }

    // Each LenBytes<MIN, MAX> is its own monomorphization with
    // its own set of coverage regions; per-monomorphization
    // accounting means every monomorphization must exercise both
    // the Ok and Err branches of `refine` for the per-file
    // coverage tally to hit 100%. The next two tests provide the
    // Ok-path counterparts to `len_bytes_counts_bytes_not_scalars`
    // and `len_bytes_rejects_too_short`, which only hit Err.

    #[test]
    fn len_bytes_one_one_admits_single_byte() {
        let r: Refined<String, LenBytes<1, 1>> = Refined::try_new("a".to_string()).unwrap();
        assert_eq!(r.as_inner(), "a");
    }

    #[test]
    fn len_bytes_five_ten_admits_within_range() {
        let r: Refined<String, LenBytes<5, 10>> = Refined::try_new("hello".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello");
    }

    // LenChars/LenBytes with MIN > MAX is rejected at compile
    // time via `const { assert!(MIN <= MAX) }`; the previous
    // runtime tests for the empty-range error are no longer needed
    // because the offending monomorphization is unrepresentable.

    #[test]
    fn non_empty_rejects_empty() {
        let result: Result<Refined<String, NonEmpty>, _> = Refined::try_new(String::new());
        assert_eq!(result.unwrap_err(), StringError::Empty);
    }

    #[test]
    fn non_empty_accepts_nonempty() {
        let r: Refined<String, NonEmpty> = Refined::try_new("x".to_string()).unwrap();
        assert_eq!(r.as_inner(), "x");
    }

    #[test]
    fn each_char_accepts_uniform_predicate() {
        let r: Refined<String, EachChar<AsciiAlphanumeric>> =
            Refined::try_new("user42".to_string()).unwrap();
        assert_eq!(r.as_inner(), "user42");
    }

    #[test]
    fn each_char_reports_offset_of_first_violation() {
        let result: Result<Refined<String, EachChar<AsciiAlphanumeric>>, _> =
            Refined::try_new("user-42".to_string());
        assert_eq!(result.unwrap_err(), StringError::BadChar { offset: 4 },);
    }

    #[test]
    fn ident_char_admits_alnum_and_underscore() {
        let r: Refined<String, EachChar<IdentChar>> =
            Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(r.as_inner(), "user_42");

        let bad: Result<Refined<String, EachChar<IdentChar>>, _> =
            Refined::try_new("user.42".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 4 });
    }

    // ─── FirstChar / IdentStart. ─────────────────────────────────

    #[test]
    fn first_char_accepts_admissible_head() {
        let r: Refined<String, FirstChar<IdentStart>> =
            Refined::try_new("name".to_string()).unwrap();
        assert_eq!(r.as_inner(), "name");
    }

    #[test]
    fn first_char_admits_empty_string() {
        // Empty string carries no first character. Composition with
        // a length bound is the way to forbid empty; FirstChar
        // itself stays minimal.
        let r: Refined<String, FirstChar<IdentStart>> = Refined::try_new(String::new()).unwrap();
        assert!(r.as_inner().is_empty());
    }

    #[test]
    fn first_char_rejects_inadmissible_head() {
        let result: Result<Refined<String, FirstChar<IdentStart>>, _> =
            Refined::try_new("1abc".to_string());
        assert_eq!(result.unwrap_err(), StringError::BadChar { offset: 0 },);
    }

    #[test]
    fn ident_grammar_via_composition_rejects_leading_digit() {
        // A real identifier grammar: leading char alpha/underscore,
        // rest alnum/underscore.
        type IdentRule = And<EachChar<IdentChar>, FirstChar<IdentStart>>;
        let good: Refined<String, IdentRule> = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(good.as_inner(), "user_42");

        let bad: Result<Refined<String, IdentRule>, _> = Refined::try_new("1abc".to_string());
        assert!(bad.is_err());
    }

    #[test]
    fn non_control_admits_printable_unicode() {
        let r: Refined<String, EachChar<NonControl>> =
            Refined::try_new("hello world! éé".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello world! éé");

        let bad: Result<Refined<String, EachChar<NonControl>>, _> =
            Refined::try_new("a\tb".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    // ─── HexChar (hex feature). ──────────────────────────────────

    #[cfg(feature = "hex")]
    #[test]
    fn hex_char_admits_ascii_hex_digits() {
        use super::HexChar;
        // Mix of digits, lower-hex letters, and upper-hex letters.
        let r: Refined<String, EachChar<HexChar>> =
            Refined::try_new("0123456789abcdefABCDEF".to_string()).unwrap();
        assert_eq!(r.as_inner(), "0123456789abcdefABCDEF");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_char_rejects_non_hex_character() {
        use super::HexChar;
        let bad: Result<Refined<String, EachChar<HexChar>>, _> =
            Refined::try_new("0a1g".to_string());
        // 'g' (offset 3) is the first non-hex character.
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 3 });
    }

    // ─── PrintableLine / PrintableMultiline (unicode feature). ──

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_line_admits_ordinary_text() {
        use super::PrintableLine;
        let r: Refined<String, EachChar<PrintableLine>> =
            Refined::try_new("Hello, world! éé 日本語 - punctuation.".to_string()).unwrap();
        assert_eq!(r.as_inner(), "Hello, world! éé 日本語 - punctuation.",);
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_line_rejects_newline() {
        use super::PrintableLine;
        let bad: Result<Refined<String, EachChar<PrintableLine>>, _> =
            Refined::try_new("line1\nline2".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 5 });
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_line_rejects_tab() {
        use super::PrintableLine;
        let bad: Result<Refined<String, EachChar<PrintableLine>>, _> =
            Refined::try_new("a\tb".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_line_rejects_zero_width_space() {
        use super::PrintableLine;
        // U+200B is 3 bytes in UTF-8 (E2 80 8B); appears at byte
        // offset 1 after the leading 'a'.
        let bad: Result<Refined<String, EachChar<PrintableLine>>, _> =
            Refined::try_new("a\u{200B}b".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_line_rejects_bom() {
        use super::PrintableLine;
        let bad: Result<Refined<String, EachChar<PrintableLine>>, _> =
            Refined::try_new("a\u{FEFF}b".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_multiline_admits_newlines() {
        use super::PrintableMultiline;
        let r: Refined<String, EachChar<PrintableMultiline>> =
            Refined::try_new("line1\nline2\n".to_string()).unwrap();
        assert_eq!(r.as_inner(), "line1\nline2\n");
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_multiline_still_rejects_tab() {
        use super::PrintableMultiline;
        let bad: Result<Refined<String, EachChar<PrintableMultiline>>, _> =
            Refined::try_new("ok\tbad".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 2 });
    }

    #[cfg(feature = "unicode")]
    #[test]
    fn printable_multiline_still_rejects_zero_width() {
        use super::PrintableMultiline;
        let bad: Result<Refined<String, EachChar<PrintableMultiline>>, _> =
            Refined::try_new("a\u{200B}b".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    // ─── IdentDashChar. ──────────────────────────────────────────

    #[test]
    fn ident_dash_char_admits_alnum_underscore_and_dash() {
        let r: Refined<String, EachChar<IdentDashChar>> =
            Refined::try_new("my-crate_42".to_string()).unwrap();
        assert_eq!(r.as_inner(), "my-crate_42");
    }

    #[test]
    fn ident_dash_char_rejects_dot() {
        let bad: Result<Refined<String, EachChar<IdentDashChar>>, _> =
            Refined::try_new("my.crate".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 2 });
    }

    #[test]
    fn ident_dash_char_predicate_membership() {
        // Spot-check the alphabet directly so the trait `test` impl
        // gets a non-`refine` exercise too.
        assert!(<IdentDashChar as CharPredicate>::test('a'));
        assert!(<IdentDashChar as CharPredicate>::test('Z'));
        assert!(<IdentDashChar as CharPredicate>::test('0'));
        assert!(<IdentDashChar as CharPredicate>::test('_'));
        assert!(<IdentDashChar as CharPredicate>::test('-'));
        assert!(!<IdentDashChar as CharPredicate>::test('.'));
        assert!(!<IdentDashChar as CharPredicate>::test(' '));
    }

    #[test]
    fn cargo_package_name_via_composition_rejects_leading_dash() {
        // The motivating composition: leading char alphanumeric,
        // body alnum/underscore/dash. (`AsciiAlphanumeric` is a
        // tighter head predicate than `IdentDashChar` itself.)
        use crate::composition::And;
        type CargoName = And<EachChar<IdentDashChar>, FirstChar<AsciiAlphanumeric>>;

        let ok: Refined<String, CargoName> = Refined::try_new("my-crate_42".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "my-crate_42");

        let bad: Result<Refined<String, CargoName>, _> = Refined::try_new("-leading".to_string());
        assert!(bad.is_err());
    }

    // ─── HexFixedLower / HexFixedAny (hex feature). ──────────────

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_admits_sha1_shape() {
        use super::HexFixedLower;
        let r: Refined<String, HexFixedLower<40>> =
            Refined::try_new("356a192b7913b04c54574d18c28d46e6395428ab".to_string()).unwrap();
        assert_eq!(r.as_inner().len(), 40);
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_rejects_wrong_length() {
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<40>>, _> =
            Refined::try_new("a".repeat(39));
        assert_eq!(bad.unwrap_err(), StringError::BadHexLength { actual: 39 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_rejects_uppercase() {
        // Lowercase-only variant: an uppercase character is rejected
        // even when the length is correct.
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<4>>, _> = Refined::try_new("0aFB".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 2 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_rejects_non_hex() {
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<4>>, _> = Refined::try_new("0a1g".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 3 });
    }

    // Per-monomorphization Ok-path for HexFixedLower's smallest
    // non-trivial monomorphization (LEN = 2).
    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_two_admits_single_byte() {
        use super::HexFixedLower;
        let r: Refined<String, HexFixedLower<2>> = Refined::try_new("ab".to_string()).unwrap();
        assert_eq!(r.as_inner(), "ab");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_two_rejects_length() {
        use super::HexFixedLower;
        // Wrong length for the LEN=2 monomorphization.
        let bad: Result<Refined<String, HexFixedLower<2>>, _> = Refined::try_new("abc".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadHexLength { actual: 3 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_two_rejects_uppercase() {
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<2>>, _> = Refined::try_new("aB".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_admits_mixed_case() {
        use super::HexFixedAny;
        let r: Refined<String, HexFixedAny<4>> = Refined::try_new("0aFB".to_string()).unwrap();
        assert_eq!(r.as_inner(), "0aFB");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_rejects_wrong_length() {
        use super::HexFixedAny;
        let bad: Result<Refined<String, HexFixedAny<4>>, _> = Refined::try_new("0aF".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadHexLength { actual: 3 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_rejects_non_hex() {
        use super::HexFixedAny;
        let bad: Result<Refined<String, HexFixedAny<4>>, _> = Refined::try_new("0a1g".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 3 });
    }

    // Per-monomorphization Ok-path for HexFixedAny LEN=2.
    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_two_admits_single_byte() {
        use super::HexFixedAny;
        let r: Refined<String, HexFixedAny<2>> = Refined::try_new("aB".to_string()).unwrap();
        assert_eq!(r.as_inner(), "aB");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_two_rejects_length() {
        use super::HexFixedAny;
        let bad: Result<Refined<String, HexFixedAny<2>>, _> = Refined::try_new("a".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadHexLength { actual: 1 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_two_rejects_non_hex() {
        use super::HexFixedAny;
        let bad: Result<Refined<String, HexFixedAny<2>>, _> = Refined::try_new("0z".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 1 });
    }

    // SHA-1-sized HexFixedAny Ok path to exercise that monomorphization.
    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_forty_admits_sha1_shape() {
        use super::HexFixedAny;
        let r: Refined<String, HexFixedAny<40>> =
            Refined::try_new("356A192B7913B04C54574D18C28D46E6395428AB".to_string()).unwrap();
        assert_eq!(r.as_inner().len(), 40);
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_any_forty_rejects_non_hex() {
        use super::HexFixedAny;
        // Length-correct but contains a non-hex char.
        let mut s = "a".repeat(39);
        s.push('z');
        let bad: Result<Refined<String, HexFixedAny<40>>, _> = Refined::try_new(s);
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 39 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_string_composition_matches_sha1_shape() {
        // A 40-char hex SHA-1 string.
        use super::HexChar;
        use crate::composition::And;

        type Sha1Hex = And<LenChars<40, 40>, EachChar<HexChar>>;

        let good: Refined<String, Sha1Hex> =
            Refined::try_new("356a192b7913b04c54574d18c28d46e6395428ab".to_string()).unwrap();
        assert_eq!(good.as_inner().len(), 40);

        // 39 chars — too short.
        let short: Result<Refined<String, Sha1Hex>, _> =
            Refined::try_new("356a192b7913b04c54574d18c28d46e639542".to_string());
        assert!(short.is_err());
    }

    #[test]
    fn display_formats_every_variant() {
        // Hand-rolled `Display` arms — one assertion per variant so
        // each arm of the `match` is hit. `Error::source` returns
        // `None`; confirmed via the `dyn Error` cast.
        assert_eq!(
            StringError::CharCountOutOfRange { actual: 6 }.to_string(),
            "character count 6 not in admissible range",
        );
        assert_eq!(
            StringError::ByteLenOutOfRange { actual: 2 }.to_string(),
            "byte length 2 not in admissible range",
        );
        assert_eq!(StringError::Empty.to_string(), "empty string");
        assert_eq!(
            StringError::BadChar { offset: 4 }.to_string(),
            "character at byte offset 4 not admissible",
        );
        assert_eq!(
            StringError::BadHexLength { actual: 3 }.to_string(),
            "hex string length 3 does not match expected length",
        );
        let dyn_err: &dyn core::error::Error = &StringError::Empty;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn refinement_macro_label_admits_and_rejects() {
        // Macro-generated newtype: admit a clean alnum label and
        // reject one with a forbidden character.
        let ok = TestLabel::try_new("user42".to_string()).unwrap();
        assert_eq!(ok.as_inner(), "user42");
        let owned: String = ok.into_inner();
        assert_eq!(owned, "user42");
        let bad = TestLabel::try_new("user-42".to_string());
        assert!(bad.is_err());
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

        // ─── LenChars reject: strings outside the char-count band.

        #[test]
        fn len_chars_rejects_too_long_via_proptest(
            s in "[a-z]{11,30}"
        ) {
            let result: Result<Refined<String, LenChars<1, 10>>, _>
                = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }

        // ─── LenBytes admit + reject. ─────────────────────────

        #[test]
        fn len_bytes_round_trips_in_range(
            s in "[a-z]{1,5}"
        ) {
            // ASCII-only input keeps char count == byte length.
            let r: Refined<String, LenBytes<1, 5>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn len_bytes_rejects_too_long_via_proptest(
            s in "[a-z]{6,20}"
        ) {
            let result: Result<Refined<String, LenBytes<1, 5>>, _>
                = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }

        // ─── NonEmpty admit + reject. ─────────────────────────

        #[test]
        fn non_empty_admits_non_empty_strings(
            s in "[a-z]{1,20}"
        ) {
            let r: Refined<String, NonEmpty>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn non_empty_rejects_empty_string_via_proptest(
            _seed in 0_u8..=255_u8
        ) {
            // No interesting input space; use a one-shot proptest
            // for symmetry with the other reject properties.
            let result: Result<Refined<String, NonEmpty>, _>
                = Refined::try_new(String::new());
            proptest::prop_assert!(result.is_err());
        }

        // ─── EachChar<AsciiAlphanumeric> admit + reject. ─────

        #[test]
        fn each_char_alnum_admits_alnum_strings(
            s in "[a-zA-Z0-9]{1,10}"
        ) {
            let r: Refined<String, EachChar<AsciiAlphanumeric>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn each_char_alnum_rejects_with_dash(
            head in "[a-zA-Z0-9]{0,5}",
            tail in "[a-zA-Z0-9]{0,5}",
        ) {
            // Inject a `-` so the resulting string has at least one
            // forbidden character.
            let mut s = head;
            s.push('-');
            s.push_str(&tail);
            let result: Result<
                Refined<String, EachChar<AsciiAlphanumeric>>,
                _,
            > = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }

        // ─── IdentDashChar admit + reject. ───────────────────

        #[test]
        fn ident_dash_admits_alnum_underscore_dash(
            s in "[a-zA-Z0-9_-]{1,20}"
        ) {
            let r: Refined<String, EachChar<IdentDashChar>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn ident_dash_rejects_with_dot(
            head in "[a-zA-Z0-9_-]{0,5}",
            tail in "[a-zA-Z0-9_-]{0,5}",
        ) {
            let mut s = head;
            s.push('.');
            s.push_str(&tail);
            let result: Result<
                Refined<String, EachChar<IdentDashChar>>,
                _,
            > = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }
    }

    #[cfg(feature = "hex")]
    proptest::proptest! {
        // ─── HexFixedLower<4> admit + reject. ────────────────

        #[test]
        fn hex_fixed_lower_admits_lowercase_quad(
            s in "[0-9a-f]{4}"
        ) {
            use super::HexFixedLower;
            let r: Refined<String, HexFixedLower<4>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn hex_fixed_lower_rejects_when_too_short(
            s in "[0-9a-f]{0,3}"
        ) {
            use super::HexFixedLower;
            let result: Result<Refined<String, HexFixedLower<4>>, _>
                = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }

        // ─── HexFixedAny<4> admit + reject. ──────────────────

        #[test]
        fn hex_fixed_any_admits_mixed_quad(
            s in "[0-9a-fA-F]{4}"
        ) {
            use super::HexFixedAny;
            let r: Refined<String, HexFixedAny<4>>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        #[test]
        fn hex_fixed_any_rejects_when_too_long(
            s in "[0-9a-fA-F]{5,10}"
        ) {
            use super::HexFixedAny;
            let result: Result<Refined<String, HexFixedAny<4>>, _>
                = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
        }
    }
}
