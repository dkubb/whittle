//! Path primitive rules.
//!
//! Portable, forward-slash-segmented path checks designed for the
//! "this string is a sandbox-relative path" guarantee that
//! configuration files, manifests, and content-addressable stores
//! require. The rules treat the input as a logical, slash-delimited
//! path and reject anything that is not portably slash-relative —
//! including backslashes and control characters — so an untrusted
//! string cannot smuggle a `..\`-style traversal or an embedded NUL
//! past the segment guard. Richer cross-platform path handling
//! (reserved device names, case-folded volume letters) is out of
//! scope and would require `camino`'s `Utf8PathBuf`.

use alloc::string::String;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// String is a normalized, forward-slash-segmented relative path.
///
/// A path is admissible when **all** of the following hold:
///
/// - it is non-empty,
/// - it contains no control character (including NUL),
/// - it contains no backslash (`\`) — the path is forward-slash
///   only, so a `..\`-style traversal cannot bypass the segment
///   guard,
/// - it is not absolute — does not start with `/` or a Windows
///   drive letter (`C:`-style),
/// - no segment is empty (i.e., there is no `//`, no trailing `/`,
///   and no leading `/`),
/// - no segment equals `..` (parent traversal is forbidden).
///
/// Platform-reserved device names (`CON`, `PRN`, `NUL`, `AUX`,
/// `COM1`...) are out of scope and not rejected by this rule. Add
/// a separate rule when targeting Windows-aware sandboxing.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{PathError, RelativePath};
///
/// // Admit: simple relative path.
/// let ok: Refined<String, RelativePath>
///     = Refined::try_new("src/main.rs".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "src/main.rs");
///
/// // Admit: single-segment path is admissible.
/// let ok: Refined<String, RelativePath>
///     = Refined::try_new("README.md".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "README.md");
///
/// // Reject: empty string.
/// let err = Refined::<String, RelativePath>::try_new(String::new())
///     .unwrap_err();
/// assert_eq!(err, PathError::Empty);
///
/// // Reject: absolute Unix path.
/// let err = Refined::<String, RelativePath>::try_new(
///     "/etc/passwd".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, PathError::Absolute);
///
/// // Reject: parent traversal.
/// let err = Refined::<String, RelativePath>::try_new(
///     "src/../etc".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, PathError::ParentTraversal { index: 1 });
///
/// // Reject: empty segment from a doubled separator.
/// let err = Refined::<String, RelativePath>::try_new(
///     "foo//bar".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, PathError::EmptySegment { index: 1 });
///
/// // Reject: a backslash (here a `..\` traversal) — forward-slash
/// // only, so it can't bypass the `..` segment guard.
/// let err = Refined::<String, RelativePath>::try_new(
///     "..\\escape".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, PathError::Backslash { offset: 2 });
///
/// // Reject: embedded control character (here NUL).
/// let err = Refined::<String, RelativePath>::try_new(
///     "a\0b".to_string(),
/// ).unwrap_err();
/// assert_eq!(err, PathError::ControlChar { offset: 1 });
/// ```
pub struct RelativePath;

/// Errors common to every path primitive.
#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    /// Empty input. Construction-time only; never produced for any
    /// non-empty input.
    Empty,

    /// Path is absolute: a `/`-rooted Unix path or a Windows
    /// drive-letter prefix (`C:`). A UNC `\\` prefix is rejected
    /// earlier as [`PathError::Backslash`].
    Absolute,

    /// A segment equal to `..` (parent traversal) is forbidden.
    /// `index` is the segment position (0-based, after splitting
    /// on `/`).
    ParentTraversal {
        /// Position of the `..` segment.
        index: usize,
    },

    /// An empty segment (produced by a doubled separator or a
    /// trailing slash). `index` is the segment position.
    EmptySegment {
        /// Position of the empty segment.
        index: usize,
    },

    /// A control character (including NUL) appears in the path.
    /// `offset` is its byte offset in the input.
    ControlChar {
        /// Byte offset of the control character.
        offset: usize,
    },

    /// A backslash (`\`) appears in the path. `RelativePath` is
    /// forward-slash only; a backslash is rejected so a `..\`-style
    /// traversal cannot bypass the segment guard. `offset` is its
    /// byte offset in the input.
    Backslash {
        /// Byte offset of the backslash.
        offset: usize,
    },
}

impl core::fmt::Display for PathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Empty => f.write_str("path is empty"),
            Self::Absolute => f.write_str("path is absolute"),
            Self::ParentTraversal { index } => {
                write!(f, "path segment {index} is `..` (parent traversal)")
            }
            Self::EmptySegment { index } => {
                write!(f, "path segment {index} is empty")
            }
            Self::ControlChar { offset } => {
                write!(f, "path contains a control character at byte {offset}")
            }
            Self::Backslash { offset } => {
                write!(f, "path contains a backslash at byte {offset}")
            }
        }
    }
}

impl core::error::Error for PathError {}

impl Rule<String> for RelativePath {
    type Error = PathError;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        if raw.is_empty() {
            return Err(PathError::Empty);
        }
        // Reject forbidden characters anywhere first: control
        // characters (including NUL, which truncates C strings and
        // can slip past filesystem APIs) and backslash (a `..\`
        // would otherwise defeat the `/`-only segment guard below,
        // and a leading `\\` is a Windows UNC root).
        for (offset, ch) in raw.char_indices() {
            if ch.is_control() {
                return Err(PathError::ControlChar { offset });
            }
            if ch == '\\' {
                return Err(PathError::Backslash { offset });
            }
        }
        // Absolute-path detection covers two shapes:
        //   - Unix-style leading `/`,
        //   - Windows drive letter `X:` where `X` is ASCII alpha.
        if raw.starts_with('/') || is_windows_drive_prefix(&raw) {
            return Err(PathError::Absolute);
        }
        for (index, segment) in raw.split('/').enumerate() {
            if segment.is_empty() {
                return Err(PathError::EmptySegment { index });
            }
            if segment == ".." {
                return Err(PathError::ParentTraversal { index });
            }
        }
        Ok(raw)
    }
}

// ─── `PureFilter` impl. ───────────────────────────────────────────
//
// SOUNDNESS: `refine` inspects the path's segments and returns the
// input String itself on acceptance — no canonicalisation.

impl crate::rule::PureFilter for RelativePath {}

// ─── Serde `DeserializeRule` impl: default parse-then-refine. ─────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[] DeserializeRule<String> for RelativePath
}

// ─── `ArbitraryRule` impl. ────────────────────────────────────────

#[cfg(feature = "proptest")]
impl ArbitraryRule<String> for RelativePath {
    type Strategy = proptest::strategy::BoxedStrategy<String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // Generate a non-empty `Vec<char>` drawn from a segment-
        // safe ASCII alphabet (alnum + `_` + `-` + `/`), then
        // collect into a `String`. Forbidden shapes:
        //
        // - leading `/` is excluded by the post-`collect` fixup
        //   below: the first char is constrained to alnum/`_`/`-`,
        //   so absolute Unix paths can't appear.
        // - `..` segments are excluded by the alphabet: the only
        //   non-alnum chars are `/`, `_`, `-`, and `.` is never
        //   sampled, so the `..` segment is unrepresentable.
        // - Windows drive prefixes (`C:`) are excluded: `:` is not
        //   in the alphabet.
        // - empty segments would arise from `//`. The alphabet
        //   never emits the path as starting with `/`, but interior
        //   `//` would still emit `EmptySegment`. The fixup below
        //   coalesces runs of `/` into a single `/`.
        proptest::collection::vec(
            proptest::char::ranges(alloc::borrow::Cow::Owned(alloc::vec![
                'A'..='Z',
                'a'..='z',
                '0'..='9',
                '_'..='_',
                '-'..='-',
                '/'..='/',
            ])),
            1_usize..=16_usize,
        )
        .prop_map(collect_relative_path_chars)
        .boxed()
    }
}

#[cfg(feature = "proptest")]
fn collect_relative_path_chars(chars: alloc::vec::Vec<char>) -> String {
    // Normalize the generated `Vec<char>` into a relative path:
    //
    // 1. Coalesce consecutive `/`s to a single `/`.
    // 2. Drop a leading `/`.
    // 3. Drop a trailing `/`.
    // 4. If the result is empty (everything collapsed away), seed
    //    a single alnum char so the path is non-empty.
    let mut out = String::with_capacity(chars.len());
    let mut prev_slash = false;
    for ch in chars {
        if ch == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(ch);
    }
    if out.starts_with('/') {
        out.remove(0);
    }
    if out.ends_with('/') {
        out.pop();
    }
    if out.is_empty() {
        out.push('a');
    }
    out
}

/// Detect a Windows drive-letter prefix like `C:` / `c:foo`.
///
/// Bare `C:` (alpha + colon at offset 0..2) is treated as an
/// absolute path even without a following separator: the rule's
/// intent is to reject any input that could be interpreted as
/// drive-anchored on Windows.
#[inline]
fn is_windows_drive_prefix(raw: &str) -> bool {
    let mut bytes = raw.bytes();
    matches!(
        (bytes.next(), bytes.next()),
        (Some(letter), Some(b':')) if letter.is_ascii_alphabetic()
    )
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{PathError, RelativePath};
    use crate::rule::Refined;

    #[test]
    fn relative_path_admits_simple_path() {
        let r: Refined<String, RelativePath> = Refined::try_new("src/main.rs".to_string()).unwrap();
        assert_eq!(r.as_inner(), "src/main.rs");
    }

    #[test]
    fn relative_path_admits_single_segment() {
        let r: Refined<String, RelativePath> = Refined::try_new("README.md".to_string()).unwrap();
        assert_eq!(r.as_inner(), "README.md");
    }

    #[test]
    fn relative_path_admits_deep_path() {
        let r: Refined<String, RelativePath> =
            Refined::try_new("crates/whittle-core/src/lib.rs".to_string()).unwrap();
        assert_eq!(r.as_inner(), "crates/whittle-core/src/lib.rs");
    }

    #[test]
    fn relative_path_admits_segment_that_starts_with_dot() {
        // `.git`, `.hidden`, etc. are valid relative segments — only
        // the bare `..` segment is forbidden.
        let r: Refined<String, RelativePath> = Refined::try_new(".git/HEAD".to_string()).unwrap();
        assert_eq!(r.as_inner(), ".git/HEAD");
    }

    #[test]
    fn relative_path_admits_single_dot_segment() {
        // `.` is not parent-traversal; `..` is. Treating `.` as
        // admissible matches Unix path semantics (no-op step).
        let r: Refined<String, RelativePath> =
            Refined::try_new("./src/main.rs".to_string()).unwrap();
        assert_eq!(r.as_inner(), "./src/main.rs");
    }

    #[test]
    fn relative_path_rejects_empty() {
        let result: Result<Refined<String, RelativePath>, _> = Refined::try_new(String::new());
        assert_eq!(result.unwrap_err(), PathError::Empty);
    }

    #[test]
    fn relative_path_rejects_absolute_unix() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("/etc/passwd".to_string());
        assert_eq!(result.unwrap_err(), PathError::Absolute);
    }

    #[test]
    fn relative_path_rejects_windows_drive() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("C:/Users/x".to_string());
        assert_eq!(result.unwrap_err(), PathError::Absolute);
    }

    #[test]
    fn relative_path_rejects_bare_windows_drive() {
        // No following separator: still drive-anchored on Windows.
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("c:foo".to_string());
        assert_eq!(result.unwrap_err(), PathError::Absolute);
    }

    #[test]
    fn relative_path_rejects_unc_backslash_prefix() {
        // A Windows UNC `\\` root is rejected as a backslash (the
        // path is forward-slash only) at byte 0.
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("\\\\server\\share".to_string());
        assert_eq!(result.unwrap_err(), PathError::Backslash { offset: 0 });
    }

    #[test]
    fn relative_path_rejects_backslash_traversal() {
        // `..\escape` must not slip past the `/`-only segment guard;
        // the backslash at byte 2 is rejected directly.
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("..\\escape".to_string());
        assert_eq!(result.unwrap_err(), PathError::Backslash { offset: 2 });
    }

    #[test]
    fn relative_path_rejects_embedded_nul() {
        let result: Result<Refined<String, RelativePath>, _> = Refined::try_new("a\0b".to_string());
        assert_eq!(result.unwrap_err(), PathError::ControlChar { offset: 1 });
    }

    #[test]
    fn relative_path_rejects_embedded_newline_control() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("foo\nbar".to_string());
        assert_eq!(result.unwrap_err(), PathError::ControlChar { offset: 3 });
    }

    #[test]
    fn relative_path_rejects_parent_traversal_at_head() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("../escape".to_string());
        assert_eq!(result.unwrap_err(), PathError::ParentTraversal { index: 0 });
    }

    #[test]
    fn relative_path_rejects_parent_traversal_deep() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("src/../etc".to_string());
        assert_eq!(result.unwrap_err(), PathError::ParentTraversal { index: 1 });
    }

    #[test]
    fn relative_path_rejects_empty_segment_from_doubled_slash() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("foo//bar".to_string());
        assert_eq!(result.unwrap_err(), PathError::EmptySegment { index: 1 });
    }

    #[test]
    fn relative_path_rejects_trailing_slash() {
        // Trailing slash yields an empty final segment.
        let result: Result<Refined<String, RelativePath>, _> = Refined::try_new("src/".to_string());
        assert_eq!(result.unwrap_err(), PathError::EmptySegment { index: 1 });
    }

    #[test]
    fn display_formats_every_variant() {
        // Hand-rolled `Display` arms — one assertion per variant so
        // each arm is exercised. The trait cast confirms
        // `core::error::Error` is implemented with no source chain.
        assert_eq!(PathError::Empty.to_string(), "path is empty");
        assert_eq!(PathError::Absolute.to_string(), "path is absolute");
        assert_eq!(
            PathError::ParentTraversal { index: 1 }.to_string(),
            "path segment 1 is `..` (parent traversal)",
        );
        assert_eq!(
            PathError::EmptySegment { index: 2 }.to_string(),
            "path segment 2 is empty",
        );
        assert_eq!(
            PathError::ControlChar { offset: 3 }.to_string(),
            "path contains a control character at byte 3",
        );
        assert_eq!(
            PathError::Backslash { offset: 0 }.to_string(),
            "path contains a backslash at byte 0",
        );
        let dyn_err: &dyn core::error::Error = &PathError::Empty;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn relative_path_non_drive_colon_admitted() {
        // A colon mid-segment is not a Windows drive prefix.
        // (The library is portable and doesn't try to model Windows
        // device-name semantics here.)
        let r: Refined<String, RelativePath> = Refined::try_new("foo:bar/baz".to_string()).unwrap();
        assert_eq!(r.as_inner(), "foo:bar/baz");
    }

    #[test]
    fn relative_path_admits_leading_digit_colon() {
        // First byte is `1` (not ASCII-alphabetic), second is `:`, so
        // this is *not* a Windows drive prefix. Exercises the false
        // arm of the `letter.is_ascii_alphabetic()` guard in
        // `is_windows_drive_prefix`.
        let r: Refined<String, RelativePath> = Refined::try_new("1:foo".to_string()).unwrap();
        assert_eq!(r.as_inner(), "1:foo");
    }

    // ─── Strategy fixup: `collect_relative_path_chars`. ──────────
    //
    // Deterministic coverage for the post-`collect` fixups in the
    // `ArbitraryRule` strategy. The proptests below also reach these
    // branches, but only when the RNG happens to draw the triggering
    // shape (a `/` run, an edge `/`, or an all-`/` vector), which
    // made the 100%-coverage gate flaky. Each test drives exactly
    // one fixup with a crafted input.

    #[cfg(feature = "proptest")]
    #[test]
    fn collect_relative_path_chars_passes_through_clean_input() {
        let out = super::collect_relative_path_chars(alloc::vec!['a', '/', 'b']);
        assert_eq!(out, "a/b");
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn collect_relative_path_chars_coalesces_slash_run() {
        let out = super::collect_relative_path_chars(alloc::vec!['a', '/', '/', '/', 'b']);
        assert_eq!(out, "a/b");
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn collect_relative_path_chars_drops_leading_slash() {
        let out = super::collect_relative_path_chars(alloc::vec!['/', 'a']);
        assert_eq!(out, "a");
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn collect_relative_path_chars_drops_trailing_slash() {
        let out = super::collect_relative_path_chars(alloc::vec!['a', '/']);
        assert_eq!(out, "a");
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn collect_relative_path_chars_seeds_fallback_when_all_slashes() {
        // `['/', '/']` coalesces to `"/"`, the leading-slash trim
        // then empties the string, and the fallback seeds `'a'`.
        let out = super::collect_relative_path_chars(alloc::vec!['/', '/']);
        assert_eq!(out, "a");
    }

    proptest::proptest! {
        // ─── RelativePath admit. ──────────────────────────────

        #[test]
        fn relative_path_admits_alnum_segments(
            // Each segment alnum-only, 1..=8 chars; 1..=4 segments.
            segments in proptest::collection::vec(
                "[a-zA-Z0-9_-]{1,8}",
                1_usize..=4_usize,
            )
        ) {
            let s = segments.join("/");
            let r: Refined<String, RelativePath>
                = Refined::try_new(s.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &s);
        }

        // ─── RelativePath reject: parent traversal anywhere. ──

        #[test]
        fn relative_path_rejects_parent_traversal(
            head in proptest::collection::vec(
                "[a-zA-Z0-9_-]{1,5}",
                0_usize..=3_usize,
            ),
            tail in proptest::collection::vec(
                "[a-zA-Z0-9_-]{1,5}",
                0_usize..=3_usize,
            ),
        ) {
            // Splice `..` between two clean segment lists.
            let parent_index = head.len();
            let mut segments = head;
            segments.push("..".to_string());
            segments.extend(tail);
            let s = segments.join("/");
            let result: Result<Refined<String, RelativePath>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                PathError::ParentTraversal { index: parent_index },
            );
        }

        // ─── RelativePath reject: absolute Unix paths. ────────

        #[test]
        fn relative_path_rejects_absolute_unix_paths(
            tail in "[a-zA-Z0-9_/-]{0,20}"
        ) {
            let mut s = String::from("/");
            s.push_str(&tail);
            let result: Result<Refined<String, RelativePath>, _>
                = Refined::try_new(s);
            proptest::prop_assert_eq!(result.unwrap_err(), PathError::Absolute);
        }

        // ─── `ArbitraryRule` for `RelativePath`. The strategy
        //     emits values that pass `RelativePath::refine` by
        //     construction (no `/`-prefix, no empty segments, no
        //     `..` segments, no Windows drive prefix).

        #[cfg(feature = "proptest")]
        #[test]
        fn arbitrary_relative_path_is_admissible(
            r in proptest::arbitrary::any::<Refined<String, RelativePath>>()
        ) {
            let s = r.as_inner();
            proptest::prop_assert!(!s.is_empty());
            proptest::prop_assert!(!s.starts_with('/'));
            proptest::prop_assert!(!s.ends_with('/'));
            proptest::prop_assert!(!s.contains("//"));
            proptest::prop_assert!(s.split('/').all(|segment| segment != ".."));
        }
    }
}
