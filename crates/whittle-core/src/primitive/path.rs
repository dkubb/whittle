//! Path primitive rules.
//!
//! Portable, forward-slash-segmented path checks designed for the
//! "this string is a sandbox-relative path" guarantee that
//! configuration files, manifests, and content-addressable stores
//! require. The rules treat the input as a logical, slash-delimited
//! path; full cross-platform path handling (Windows `\` separators,
//! reserved device names, case-folded volume letters) is out of
//! scope and would require `camino`'s `Utf8PathBuf`.

use alloc::string::String;

use crate::rule::Rule;

/// String is a normalized, forward-slash-segmented relative path.
///
/// A path is admissible when **all** of the following hold:
///
/// - it is non-empty,
/// - it is not absolute — does not start with `/`, a Windows drive
///   letter (`C:`-style), or a UNC prefix (`\\`),
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
/// ```
pub struct RelativePath;

/// Errors common to every path primitive.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathError {
    /// Empty input. Construction-time only; never produced for any
    /// non-empty input.
    Empty,

    /// Path is absolute. Includes `/`-rooted Unix paths, Windows
    /// drive-letter prefixes (`C:`), and UNC prefixes (`\\`).
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
        // Absolute-path detection covers three shapes:
        //   - Unix-style leading `/`,
        //   - Windows UNC `\\` prefix,
        //   - Windows drive letter `X:` where `X` is ASCII alpha.
        if raw.starts_with('/') || raw.starts_with("\\\\") || is_windows_drive_prefix(&raw) {
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
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
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
        let r: Refined<String, RelativePath> =
            Refined::try_new(".git/HEAD".to_string()).unwrap();
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
        let result: Result<Refined<String, RelativePath>, _> = Refined::try_new("c:foo".to_string());
        assert_eq!(result.unwrap_err(), PathError::Absolute);
    }

    #[test]
    fn relative_path_rejects_unc_prefix() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("\\\\server\\share".to_string());
        assert_eq!(result.unwrap_err(), PathError::Absolute);
    }

    #[test]
    fn relative_path_rejects_parent_traversal_at_head() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("../escape".to_string());
        assert_eq!(
            result.unwrap_err(),
            PathError::ParentTraversal { index: 0 }
        );
    }

    #[test]
    fn relative_path_rejects_parent_traversal_deep() {
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("src/../etc".to_string());
        assert_eq!(
            result.unwrap_err(),
            PathError::ParentTraversal { index: 1 }
        );
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
        let result: Result<Refined<String, RelativePath>, _> =
            Refined::try_new("src/".to_string());
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
        let dyn_err: &dyn core::error::Error = &PathError::Empty;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn relative_path_non_drive_colon_admitted() {
        // A colon mid-segment is not a Windows drive prefix.
        // (The library is portable and doesn't try to model Windows
        // device-name semantics here.)
        let r: Refined<String, RelativePath> =
            Refined::try_new("foo:bar/baz".to_string()).unwrap();
        assert_eq!(r.as_inner(), "foo:bar/baz");
    }

    #[test]
    fn relative_path_admits_leading_digit_colon() {
        // First byte is `1` (not ASCII-alphabetic), second is `:`, so
        // this is *not* a Windows drive prefix. Exercises the false
        // arm of the `letter.is_ascii_alphabetic()` guard in
        // `is_windows_drive_prefix`.
        let r: Refined<String, RelativePath> =
            Refined::try_new("1:foo".to_string()).unwrap();
        assert_eq!(r.as_inner(), "1:foo");
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
            let mut segments = head;
            segments.push("..".to_string());
            segments.extend(tail);
            let s = segments.join("/");
            let result: Result<Refined<String, RelativePath>, _>
                = Refined::try_new(s);
            proptest::prop_assert!(result.is_err());
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
            proptest::prop_assert!(result.is_err());
        }
    }
}
