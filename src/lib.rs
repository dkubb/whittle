//! Whittle — parse-don't-validate types in Rust.
//!
//! See [`docs/IDEA.md`] and [`docs/ARCHITECTURE.md`] in the repo root
//! for the specification this crate implements.
//!
//! [`docs/IDEA.md`]: https://github.com/dkubb/whittle/blob/main/docs/IDEA.md
//! [`docs/ARCHITECTURE.md`]: https://github.com/dkubb/whittle/blob/main/docs/ARCHITECTURE.md

#![no_std]

pub use whittle_core::*;

/// Compile-time-validated constructor for [`primitive::Pattern`].
///
/// `pattern!(r"...")` expands to a `Pattern<RE>` rule type and validates
/// the regular expression at build time: a malformed pattern is a
/// compile error rather than a runtime panic on first construction.
///
/// Available behind the `regex` feature.
///
/// # Examples
///
/// A valid pattern expands to a usable rule:
///
/// ```
/// use whittle::{Refined, pattern};
/// use whittle::primitive::PatternError;
///
/// type Name = pattern!(r"^(?:[A-Z])(?:-?[A-Za-z]+)*$");
///
/// // Admit: matches the whole string.
/// let ok: Refined<String, Name> = Refined::try_new("A-Bc-De".to_string()).unwrap();
/// assert_eq!(ok.as_inner(), "A-Bc-De");
///
/// // Reject: does not match.
/// let err = Refined::<String, Name>::try_new("abc".to_string()).unwrap_err();
/// assert_eq!(err, PatternError::NoMatch);
/// ```
///
/// A malformed pattern is a **compile error** (the `[` is unbalanced):
///
/// ```compile_fail
/// type Bad = whittle::pattern!(r"[A-Z");
/// ```
#[cfg(feature = "regex")]
pub use whittle_core::pattern;
