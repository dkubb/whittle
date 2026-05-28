// Examples are interactive demonstrations: they use `println!` to
// confirm what was demonstrated and `unwrap()` to keep the focus on
// the API, not error plumbing. The workspace lints would otherwise
// deny both.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    clippy::items_after_statements
)]

//! `RelativePath`: portable sandbox-relative path validation.
//!
//! Walks through every `PathError` variant — `Empty`, `Absolute`,
//! `ParentTraversal`, `EmptySegment`. The rule treats input as a
//! logical forward-slash-segmented path; it does not perform
//! filesystem I/O.
//!
//! Use this for configuration files, manifests, and
//! content-addressable stores that need to reject path traversal
//! and absolute paths at the boundary. The error variants pinpoint
//! the offending segment so diagnostics can quote the input back.

use whittle::primitive::{PathError, RelativePath};
use whittle::Refined;

fn main() {
    // Admit: a simple slash-separated relative path.
    let path: Refined<String, RelativePath> =
        Refined::try_new("src/main.rs".to_string()).unwrap();
    assert_eq!(path.as_inner(), "src/main.rs");

    // Admit: dotfile segments and single-segment paths.
    let dotfile: Refined<String, RelativePath> = Refined::try_new(".git/HEAD".to_string()).unwrap();
    assert_eq!(dotfile.as_inner(), ".git/HEAD");

    // Reject: empty string.
    let empty = Refined::<String, RelativePath>::try_new(String::new()).unwrap_err();
    assert_eq!(empty, PathError::Empty);

    // Reject: absolute Unix path.
    let abs_unix =
        Refined::<String, RelativePath>::try_new("/etc/passwd".to_string()).unwrap_err();
    assert_eq!(abs_unix, PathError::Absolute);

    // Reject: Windows drive-letter prefix is also `Absolute`.
    let abs_win =
        Refined::<String, RelativePath>::try_new("C:/Users/x".to_string()).unwrap_err();
    assert_eq!(abs_win, PathError::Absolute);

    // Reject: parent-traversal segment. The index pinpoints the
    // offending segment after splitting on `/`.
    let traverse =
        Refined::<String, RelativePath>::try_new("src/../etc".to_string()).unwrap_err();
    assert_eq!(traverse, PathError::ParentTraversal { index: 1 });

    // Reject: doubled separator yields an empty segment.
    let empty_seg = Refined::<String, RelativePath>::try_new("foo//bar".to_string()).unwrap_err();
    assert_eq!(empty_seg, PathError::EmptySegment { index: 1 });

    // Reject: trailing slash yields an empty final segment.
    let trailing = Refined::<String, RelativePath>::try_new("src/".to_string()).unwrap_err();
    assert_eq!(trailing, PathError::EmptySegment { index: 1 });

    println!("OK: RelativePath admits sandbox paths, rejects every escape shape");
}
