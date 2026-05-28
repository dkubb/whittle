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

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;
use crate::transform::{StableUnderAsciiLowercase, StableUnderAsciiUppercase, StableUnderTrim};

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
/// assert_eq!(err, StringError::BadFirstChar);
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
    CharCountOutOfRange {
        /// Observed character count of the offending string.
        actual: usize,
    },

    /// Byte length not in the admissible range.
    ByteLenOutOfRange {
        /// Observed byte length of the offending string.
        actual: usize,
    },

    /// `NonEmpty` received an empty string.
    Empty,

    /// `EachChar<P>` rejected a character at the given UTF-8 byte
    /// offset.
    BadChar {
        /// UTF-8 byte offset of the rejected character.
        offset: usize,
    },

    /// `FirstChar<P>` rejected the leading character. Distinct from
    /// `BadChar` so callers can flatten head-versus-body failures
    /// without pattern-matching on `offset: 0` as a sentinel; the
    /// offset is always 0 and carries no information.
    BadFirstChar,

    /// `HexFixedLower<LEN>` / `HexFixedAny<LEN>` saw a string
    /// whose length is not the configured `LEN`. Distinct from
    /// `CharCountOutOfRange` to preserve the fixed-length /
    /// range-bound distinction.
    BadHexLength {
        /// Observed length of the offending hex string.
        actual: usize,
    },
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
            Self::BadFirstChar => f.write_str("first character not admissible"),
            Self::BadHexLength { actual } => write!(
                f,
                "hex string length {actual} does not match expected length",
            ),
        }
    }
}

impl core::error::Error for StringError {}

/// Marker: a `CharPredicate` whose admissible set excludes every
/// character that `str::trim_start_matches(char::is_whitespace)`
/// would remove from the front of a string.
///
/// Required to make `FirstChar<P>: StableUnderTrim` sound: if `P`
/// admits whitespace, the `Arbitrary` strategy can emit a string
/// whose head is whitespace, and trimming exposes a different
/// character — which may not satisfy `P`. Implementations are
/// audited against each predicate's `test` method so the marker
/// reflects the predicate's actual admissible set.
pub trait RejectsAsciiWhitespace: CharPredicate {}

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
    /// // Custom predicate: ASCII lowercase vowels only. The
    /// // example is intentionally narrow — real callers can match
    /// // additional cases (e.g. uppercase or Unicode vowels) by
    /// // extending the `matches!` arm.
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

/// `CharPredicate` that exposes a `proptest` strategy emitting
/// admissible characters.
///
/// Implementations must guarantee that every value the strategy
/// emits satisfies the predicate. `EachChar<P>` and `FirstChar<P>`
/// rely on this to generate strings that pass their per-character
/// invariants by construction.
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait ArbitraryChar: CharPredicate {
    /// Strategy type yielding admissible `char` values.
    type Strategy: proptest::strategy::Strategy<Value = char>;

    /// Construct the predicate's `char`-emitting strategy.
    fn arbitrary_char() -> Self::Strategy;
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
impl RejectsAsciiWhitespace for AsciiAlphanumeric {}

/// Build a `proptest::char::CharStrategy` from a set of inclusive
/// `char` ranges. Used by the `ArbitraryChar` impls below to express
/// "pick a char from the union of these ranges" without rejection
/// sampling against the full Unicode space.
#[cfg(feature = "proptest")]
fn char_strategy_from_ranges(
    ranges: alloc::vec::Vec<core::ops::RangeInclusive<char>>,
) -> proptest::char::CharStrategy<'static> {
    proptest::char::ranges(alloc::borrow::Cow::Owned(ranges))
}

#[cfg(feature = "proptest")]
impl ArbitraryChar for AsciiAlphanumeric {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        char_strategy_from_ranges(alloc::vec!['A'..='Z', 'a'..='z', '0'..='9']).boxed()
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
impl RejectsAsciiWhitespace for IdentChar {}

#[cfg(feature = "proptest")]
impl ArbitraryChar for IdentChar {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        char_strategy_from_ranges(alloc::vec!['A'..='Z', 'a'..='z', '0'..='9', '_'..='_']).boxed()
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
impl RejectsAsciiWhitespace for IdentStart {}

#[cfg(feature = "proptest")]
impl ArbitraryChar for IdentStart {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        char_strategy_from_ranges(alloc::vec!['A'..='Z', 'a'..='z', '_'..='_']).boxed()
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

#[cfg(feature = "proptest")]
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "matches the `Fn(&Self::Value) -> bool` signature `prop_filter` expects"
)]
fn char_is_not_control(ch: &char) -> bool {
    !ch.is_control()
}

#[cfg(feature = "proptest")]
impl ArbitraryChar for NonControl {
    // Control chars are sparse in the Unicode space (the C0 range
    // `\x00..=\x1F`, DEL `\x7F`, and the C1 range `\x80..=\x9F`),
    // so filtering `proptest::char::any()` admits the vast
    // majority of generated values without performance trouble.
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::char::any()
            .prop_filter("not control", char_is_not_control)
            .boxed()
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
#[cfg(feature = "hex")]
impl RejectsAsciiWhitespace for HexChar {}

#[cfg(all(feature = "hex", feature = "proptest"))]
impl ArbitraryChar for HexChar {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        char_strategy_from_ranges(alloc::vec!['0'..='9', 'a'..='f', 'A'..='F']).boxed()
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

#[cfg(all(feature = "unicode", feature = "proptest"))]
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "matches the `Fn(&Self::Value) -> bool` signature `prop_filter` expects"
)]
fn char_is_printable_line(ch: &char) -> bool {
    <PrintableLine as CharPredicate>::test(*ch)
}

#[cfg(all(feature = "unicode", feature = "proptest"))]
impl ArbitraryChar for PrintableLine {
    // The forbidden set is small; filter `proptest::char::any()`.
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::char::any()
            .prop_filter("printable line", char_is_printable_line)
            .boxed()
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

#[cfg(all(feature = "unicode", feature = "proptest"))]
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "matches the `Fn(&Self::Value) -> bool` signature `prop_filter` expects"
)]
fn char_is_printable_multiline(ch: &char) -> bool {
    <PrintableMultiline as CharPredicate>::test(*ch)
}

#[cfg(all(feature = "unicode", feature = "proptest"))]
impl ArbitraryChar for PrintableMultiline {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::char::any()
            .prop_filter("printable multiline", char_is_printable_multiline)
            .boxed()
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
impl RejectsAsciiWhitespace for IdentDashChar {}

#[cfg(feature = "proptest")]
impl ArbitraryChar for IdentDashChar {
    type Strategy = proptest::strategy::BoxedStrategy<char>;

    #[inline]
    fn arbitrary_char() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        char_strategy_from_ranges(alloc::vec![
            'A'..='Z',
            'a'..='z',
            '0'..='9',
            '_'..='_',
            '-'..='-',
        ])
        .boxed()
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
pub type HexFixedNormalized<const LEN: usize> = crate::transform::AsciiLowercase<HexFixedAny<LEN>>;

// ─── Rule impls. ──────────────────────────────────────────────────

impl<const MIN: usize, const MAX: usize> LenChars<MIN, MAX> {
    /// Single source of the bound invariant: `MIN <= MAX`. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }` so the same `assert!` body cannot
    /// drift between the two sites.
    const VALID: () = assert!(MIN <= MAX, "LenChars requires MIN <= MAX");
}

impl<const MIN: usize, const MAX: usize> Rule<String> for LenChars<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { Self::VALID };
        let actual = raw.chars().count();
        if !(MIN..=MAX).contains(&actual) {
            return Err(StringError::CharCountOutOfRange { actual });
        }
        Ok(raw)
    }
}

impl<const MIN: usize, const MAX: usize> LenBytes<MIN, MAX> {
    /// Single source of the bound invariant: `MIN <= MAX`. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }`.
    const VALID: () = assert!(MIN <= MAX, "LenBytes requires MIN <= MAX");
}

impl<const MIN: usize, const MAX: usize> Rule<String> for LenBytes<MIN, MAX> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { Self::VALID };
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
            return Err(StringError::BadFirstChar);
        }
        Ok(raw)
    }
}

#[cfg(feature = "hex")]
impl<const LEN: usize> HexFixedLower<LEN> {
    /// Single source of the bound invariant: `LEN` is even. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }`.
    const VALID: () = assert!(
        LEN.is_multiple_of(2),
        "HexFixedLower requires LEN to be even (one byte = two hex chars)",
    );
}

#[cfg(feature = "hex")]
impl<const LEN: usize> Rule<String> for HexFixedLower<LEN> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { Self::VALID };
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
impl<const LEN: usize> HexFixedAny<LEN> {
    /// Single source of the bound invariant: `LEN` is even. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }`.
    const VALID: () = assert!(
        LEN.is_multiple_of(2),
        "HexFixedAny requires LEN to be even (one byte = two hex chars)",
    );
}

#[cfg(feature = "hex")]
impl<const LEN: usize> Rule<String> for HexFixedAny<LEN> {
    type Error = StringError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        const { Self::VALID };
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

// ─── Transformer stability impls. ─────────────────────────────────
//
// Each impl below records that the rule's admissible region is
// invariant under the corresponding transformation — i.e. wrapping
// the rule in `Trim<...>` / `AsciiLowercase<...>` / `AsciiUppercase
// <...>` will not violate the inner rule when the transformer's
// `ArbitraryRule` strategy applies the transformation post hoc.
//
// `LenChars<MIN, MAX>`: lowercase / uppercase preserve scalar count,
// so both case-stability markers apply. *Not* `StableUnderTrim`:
// trimming can reduce the count below MIN (the whitespace-only
// counter-example for any rule whose strategy can emit whitespace).
//
// `LenBytes<MIN, MAX>`: the strategy generates ASCII-only chars (one
// byte each); ASCII case-lowering / case-raising never crosses the
// ASCII boundary, so byte length is preserved. Same `Trim` caveat
// as `LenChars`.
//
// `NonEmpty`: lowercase / uppercase preserve length; *not* trim-
// stable (`"   "` is non-empty but trims to `""`).
//
// `EachChar<P>`: trimming removes chars from start / end; the
// remaining chars all still satisfy `P`. Case-stability requires
// `P`'s alphabet to be closed under ASCII case-change; that is per
// `P`, not blanket.
//
// `FirstChar<P>`: parallel to `EachChar<P>`. Trimming may remove the
// head but the empty string is admissible. Case-stability requires
// `P`'s alphabet to be case-closed.
//
// `HexFixedLower<LEN>`: already lowercase; idempotent under
// lowercase. Uppercase would no longer be lowercase hex. Length is
// fixed, so trim breaks it.
//
// `HexFixedAny<LEN>`: alphabet (`0-9a-fA-F`) is closed under ASCII
// case-change. Length is fixed, so trim breaks it.

impl<const MIN: usize, const MAX: usize> StableUnderAsciiLowercase for LenChars<MIN, MAX> {}
impl<const MIN: usize, const MAX: usize> StableUnderAsciiUppercase for LenChars<MIN, MAX> {}

impl<const MIN: usize, const MAX: usize> StableUnderAsciiLowercase for LenBytes<MIN, MAX> {}
impl<const MIN: usize, const MAX: usize> StableUnderAsciiUppercase for LenBytes<MIN, MAX> {}

impl StableUnderAsciiLowercase for NonEmpty {}
impl StableUnderAsciiUppercase for NonEmpty {}

// `EachChar<P>` is trim-stable for every `P`: trimming only removes
// characters, so the surviving characters still satisfy `P`. The
// case-stability markers are added per-`P` below, only for those
// predicates whose alphabet is closed under ASCII case-change.
impl<P> StableUnderTrim for EachChar<P> {}

// `FirstChar<P>` is trim-stable only when `P` rejects every
// whitespace character. The unrestricted blanket would be unsound:
// if `P` admits whitespace, the `ArbitraryRule` strategy can emit
// `" 1"` (whitespace head, arbitrary tail). `Trim<FirstChar<P>>`
// transforms that to `"1"` and re-checks `FirstChar<P>::refine`,
// which may then reject the new first character. The
// `RejectsAsciiWhitespace` marker bound makes the impl
// predicate-aware: only predicates whose admissible set excludes
// whitespace get the trim-stability marker, so the
// transformer's `Arbitrary` `expect` cannot panic.
impl<P> StableUnderTrim for FirstChar<P> where P: RejectsAsciiWhitespace {}

// Case-symmetric predicates. The alphabet of each predicate listed
// here is closed under both `char::to_ascii_lowercase` and
// `char::to_ascii_uppercase`:
//
// - `AsciiAlphanumeric`: `A-Za-z0-9`. Letters case-flip within the
//   alphabet; digits are unchanged.
// - `IdentChar` / `IdentDashChar` / `IdentStart`: the above plus
//   `_` (and `-` for `IdentDashChar`), which are case-invariant.
// - `HexChar`: `0-9a-fA-F`; same closure as `AsciiAlphanumeric` on
//   the relevant subset.
// - `NonControl`: ASCII case-change of a non-control character is
//   still a non-control character (lowercase / uppercase of a letter
//   is still a letter, etc.).
// - `PrintableLine` / `PrintableMultiline`: ASCII case-change keeps
//   the character in the ASCII visible range and never produces a
//   forbidden zero-width / BOM character.
impl StableUnderAsciiLowercase for EachChar<AsciiAlphanumeric> {}
impl StableUnderAsciiUppercase for EachChar<AsciiAlphanumeric> {}
impl StableUnderAsciiLowercase for EachChar<IdentChar> {}
impl StableUnderAsciiUppercase for EachChar<IdentChar> {}
impl StableUnderAsciiLowercase for EachChar<IdentStart> {}
impl StableUnderAsciiUppercase for EachChar<IdentStart> {}
impl StableUnderAsciiLowercase for EachChar<IdentDashChar> {}
impl StableUnderAsciiUppercase for EachChar<IdentDashChar> {}
impl StableUnderAsciiLowercase for EachChar<NonControl> {}
impl StableUnderAsciiUppercase for EachChar<NonControl> {}

impl StableUnderAsciiLowercase for FirstChar<AsciiAlphanumeric> {}
impl StableUnderAsciiUppercase for FirstChar<AsciiAlphanumeric> {}
impl StableUnderAsciiLowercase for FirstChar<IdentChar> {}
impl StableUnderAsciiUppercase for FirstChar<IdentChar> {}
impl StableUnderAsciiLowercase for FirstChar<IdentStart> {}
impl StableUnderAsciiUppercase for FirstChar<IdentStart> {}
impl StableUnderAsciiLowercase for FirstChar<IdentDashChar> {}
impl StableUnderAsciiUppercase for FirstChar<IdentDashChar> {}
impl StableUnderAsciiLowercase for FirstChar<NonControl> {}
impl StableUnderAsciiUppercase for FirstChar<NonControl> {}

#[cfg(feature = "hex")]
impl StableUnderAsciiLowercase for EachChar<HexChar> {}
#[cfg(feature = "hex")]
impl StableUnderAsciiUppercase for EachChar<HexChar> {}
#[cfg(feature = "hex")]
impl StableUnderAsciiLowercase for FirstChar<HexChar> {}
#[cfg(feature = "hex")]
impl StableUnderAsciiUppercase for FirstChar<HexChar> {}

#[cfg(feature = "unicode")]
impl StableUnderAsciiLowercase for EachChar<PrintableLine> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiUppercase for EachChar<PrintableLine> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiLowercase for EachChar<PrintableMultiline> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiUppercase for EachChar<PrintableMultiline> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiLowercase for FirstChar<PrintableLine> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiUppercase for FirstChar<PrintableLine> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiLowercase for FirstChar<PrintableMultiline> {}
#[cfg(feature = "unicode")]
impl StableUnderAsciiUppercase for FirstChar<PrintableMultiline> {}

#[cfg(feature = "hex")]
impl<const LEN: usize> StableUnderAsciiLowercase for HexFixedLower<LEN> {}
// `HexFixedLower` is NOT `StableUnderAsciiUppercase`: uppercasing
// `"abcd"` yields `"ABCD"`, which the lowercase-only rule rejects.

#[cfg(feature = "hex")]
impl<const LEN: usize> StableUnderAsciiLowercase for HexFixedAny<LEN> {}
#[cfg(feature = "hex")]
impl<const LEN: usize> StableUnderAsciiUppercase for HexFixedAny<LEN> {}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// Length-bounded strings draw their `char`s from a single ASCII
// range so the per-char count and the post-`String` byte length
// line up; per-character rules draw from their predicate's
// `ArbitraryChar`. Each rule's strategy emits admissible-by-
// construction values — no rejection sampling inside the
// blanket `Refined` Arbitrary impl.

/// Cap on the number of `chars` generated when a rule's admissible
/// length is unbounded (`NonEmpty`, `EachChar<P>`, `FirstChar<P>`).
/// Picked to mirror the bounded-strategy default used by proptest's
/// regex string strategies.
#[cfg(feature = "proptest")]
const STRING_ARBITRARY_MAX_LEN: usize = 32;

#[cfg(feature = "proptest")]
fn collect_chars(chars: alloc::vec::Vec<char>) -> String {
    chars.into_iter().collect()
}

#[cfg(feature = "proptest")]
impl<const MIN: usize, const MAX: usize> ArbitraryRule<String> for LenChars<MIN, MAX> {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        // `proptest::char::any()` emits any Unicode scalar value
        // (no surrogate code points). `char.count() == vec.len()`
        // for the generated `Vec<char>`, so the resulting `String`
        // has exactly the requested scalar count.
        proptest::collection::vec(proptest::char::any(), MIN..=MAX)
            .prop_map(collect_chars)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MIN: usize, const MAX: usize> ArbitraryRule<String> for LenBytes<MIN, MAX> {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        // ASCII-only chars: every char is exactly one UTF-8 byte,
        // so the resulting `String`'s byte length equals the
        // `Vec<char>` length.
        proptest::collection::vec(proptest::char::range('\u{20}', '\u{7E}'), MIN..=MAX)
            .prop_map(collect_chars)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryRule<String> for NonEmpty {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::collection::vec(proptest::char::any(), 1_usize..=STRING_ARBITRARY_MAX_LEN)
            .prop_map(collect_chars)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<P> ArbitraryRule<String> for EachChar<P>
where
    P: ArbitraryChar,
{
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::collection::vec(P::arbitrary_char(), 0_usize..=STRING_ARBITRARY_MAX_LEN)
            .prop_map(collect_chars)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<P> ArbitraryRule<String> for FirstChar<P>
where
    P: ArbitraryChar,
{
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // `FirstChar<P>` admits the empty string. Generate either
        // an empty string or a `P`-admissible head followed by an
        // arbitrary-char tail. `BoxedStrategy` hides the strategy
        // tree from the public type to keep the API surface
        // tractable.
        let tail =
            proptest::collection::vec(proptest::char::any(), 0_usize..STRING_ARBITRARY_MAX_LEN);
        proptest::prop_oneof![
            proptest::strategy::Just(String::new()),
            (P::arbitrary_char(), tail).prop_map(|(head, tail)| {
                let mut out = String::with_capacity(tail.len() + 1);
                out.push(head);
                for ch in tail {
                    out.push(ch);
                }
                out
            }),
        ]
        .boxed()
    }
}

#[cfg(all(feature = "hex", feature = "proptest"))]
impl<const LEN: usize> ArbitraryRule<String> for HexFixedLower<LEN> {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        proptest::collection::vec(
            char_strategy_from_ranges(alloc::vec!['0'..='9', 'a'..='f']),
            LEN..=LEN,
        )
        .prop_map(collect_chars)
        .boxed()
    }
}

#[cfg(all(feature = "hex", feature = "proptest"))]
impl<const LEN: usize> ArbitraryRule<String> for HexFixedAny<LEN> {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        proptest::collection::vec(
            char_strategy_from_ranges(alloc::vec!['0'..='9', 'a'..='f', 'A'..='F']),
            LEN..=LEN,
        )
        .prop_map(collect_chars)
        .boxed()
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
        assert_eq!(result.unwrap_err(), StringError::BadFirstChar);
    }

    #[test]
    fn ident_grammar_via_composition_rejects_leading_digit() {
        // A real identifier grammar: leading char alpha/underscore,
        // rest alnum/underscore.
        type IdentRule = And<EachChar<IdentChar>, FirstChar<IdentStart>>;
        let good: Refined<String, IdentRule> = Refined::try_new("user_42".to_string()).unwrap();
        assert_eq!(good.as_inner(), "user_42");

        let bad: Result<Refined<String, IdentRule>, _> = Refined::try_new("1abc".to_string());
        bad.unwrap_err();
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
        bad.unwrap_err();
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
        let bad: Result<Refined<String, HexFixedLower<40>>, _> = Refined::try_new("a".repeat(39));
        assert_eq!(bad.unwrap_err(), StringError::BadHexLength { actual: 39 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_rejects_uppercase() {
        // Lowercase-only variant: an uppercase character is rejected
        // even when the length is correct.
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<4>>, _> =
            Refined::try_new("0aFB".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 2 });
    }

    #[cfg(feature = "hex")]
    #[test]
    fn hex_fixed_lower_rejects_non_hex() {
        use super::HexFixedLower;
        let bad: Result<Refined<String, HexFixedLower<4>>, _> =
            Refined::try_new("0a1g".to_string());
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
        short.unwrap_err();
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
            StringError::BadFirstChar.to_string(),
            "first character not admissible",
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
        bad.unwrap_err();
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
            let actual = s.chars().count();
            let result: Result<Refined<String, LenChars<1, 10>>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::CharCountOutOfRange { actual },
            );
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
            let actual = s.len();
            let result: Result<Refined<String, LenBytes<1, 5>>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::ByteLenOutOfRange { actual },
            );
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
            proptest::prop_assert_eq!(result.unwrap_err(), StringError::Empty);
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
            // forbidden character; head is ASCII so byte offset of
            // the `-` equals head.len().
            let offset = head.len();
            let mut s = head;
            s.push('-');
            s.push_str(&tail);
            let result: Result<
                Refined<String, EachChar<AsciiAlphanumeric>>,
                _,
            > = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::BadChar { offset },
            );
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
            let offset = head.len();
            let mut s = head;
            s.push('.');
            s.push_str(&tail);
            let result: Result<
                Refined<String, EachChar<IdentDashChar>>,
                _,
            > = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::BadChar { offset },
            );
        }

        // ─── `ArbitraryRule` impls. Each string rule's strategy
        //     emits admissible-by-construction values; the carrier
        //     is generated through `Refined`'s blanket Arbitrary
        //     impl so each rule's strategy is exercised.

        #[test]
        fn arbitrary_len_chars_in_range(
            r in proptest::arbitrary::any::<Refined<String, LenChars<2, 8>>>()
        ) {
            let count = r.as_inner().chars().count();
            proptest::prop_assert!((2..=8).contains(&count));
        }

        #[test]
        fn arbitrary_len_bytes_in_range(
            r in proptest::arbitrary::any::<Refined<String, LenBytes<2, 8>>>()
        ) {
            let bytes = r.as_inner().len();
            proptest::prop_assert!((2..=8).contains(&bytes));
        }

        #[test]
        fn arbitrary_non_empty_is_non_empty(
            r in proptest::arbitrary::any::<Refined<String, NonEmpty>>()
        ) {
            proptest::prop_assert!(!r.as_inner().is_empty());
        }

        #[test]
        fn arbitrary_each_char_alnum_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<AsciiAlphanumeric>>>()
        ) {
            proptest::prop_assert!(r.as_inner().chars().all(|c| c.is_ascii_alphanumeric()));
        }

        #[test]
        fn arbitrary_each_char_ident_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<IdentChar>>>()
        ) {
            proptest::prop_assert!(
                r.as_inner().chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            );
        }

        #[test]
        fn arbitrary_each_char_ident_start_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<IdentStart>>>()
        ) {
            proptest::prop_assert!(
                r.as_inner().chars().all(|c| c.is_ascii_alphabetic() || c == '_'),
            );
        }

        #[test]
        fn arbitrary_each_char_ident_dash_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<IdentDashChar>>>()
        ) {
            proptest::prop_assert!(
                r.as_inner().chars().all(
                    |c| c.is_ascii_alphanumeric() || c == '_' || c == '-',
                ),
            );
        }

        #[test]
        fn arbitrary_each_char_non_control_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<NonControl>>>()
        ) {
            proptest::prop_assert!(r.as_inner().chars().all(|c| !c.is_control()));
        }

        #[test]
        fn arbitrary_first_char_ident_start_admissible(
            r in proptest::arbitrary::any::<Refined<String, FirstChar<IdentStart>>>()
        ) {
            // The empty string is admissible; a non-empty string
            // must start with alpha or `_`.
            if let Some(ch) = r.as_inner().chars().next() {
                proptest::prop_assert!(ch.is_ascii_alphabetic() || ch == '_');
            }
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
            let actual = s.len();
            let result: Result<Refined<String, HexFixedLower<4>>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::BadHexLength { actual },
            );
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
            let actual = s.len();
            let result: Result<Refined<String, HexFixedAny<4>>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                StringError::BadHexLength { actual },
            );
        }

        // ─── `ArbitraryRule` for hex-fixed rules. Each
        //     monomorphisation gets its own strategy invocation.

        #[test]
        fn arbitrary_hex_fixed_lower_admissible(
            r in proptest::arbitrary::any::<Refined<String, super::HexFixedLower<4>>>()
        ) {
            proptest::prop_assert_eq!(r.as_inner().len(), 4);
            proptest::prop_assert!(
                r.as_inner().bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            );
        }

        #[test]
        fn arbitrary_hex_fixed_any_admissible(
            r in proptest::arbitrary::any::<Refined<String, super::HexFixedAny<4>>>()
        ) {
            proptest::prop_assert_eq!(r.as_inner().len(), 4);
            proptest::prop_assert!(r.as_inner().bytes().all(|b| b.is_ascii_hexdigit()));
        }

        #[test]
        fn arbitrary_hex_char_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<super::HexChar>>>()
        ) {
            proptest::prop_assert!(r.as_inner().bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }

    #[cfg(all(feature = "unicode", feature = "proptest"))]
    proptest::proptest! {
        #[test]
        fn arbitrary_printable_line_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<super::PrintableLine>>>()
        ) {
            proptest::prop_assert!(r.as_inner().chars().all(<super::PrintableLine as CharPredicate>::test));
        }

        #[test]
        fn arbitrary_printable_multiline_admissible(
            r in proptest::arbitrary::any::<Refined<String, EachChar<super::PrintableMultiline>>>()
        ) {
            proptest::prop_assert!(
                r.as_inner().chars().all(<super::PrintableMultiline as CharPredicate>::test),
            );
        }
    }
}
