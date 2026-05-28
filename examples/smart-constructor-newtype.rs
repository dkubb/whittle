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

//! Smart-constructor newtype via the `refinement!` macro.
//!
//! Defines a nominal type `UserName` that wraps
//! `Refined<String, NonEmpty>`. The macro generates a private
//! tuple struct with a single public construction path
//! (`UserName::try_new`), plus `as_inner` / `into_inner`.
//!
//! Use this pattern whenever a domain concept (a user name, a
//! product SKU, a request id) is currently passed around as a
//! bare `String` or `i32`. The newtype gives the concept a name
//! that the type system tracks; the macro keeps the boilerplate
//! to a single line.

use whittle::primitive::NonEmpty;
use whittle::refinement;

refinement! {
    /// User-supplied display name. Always at least one character.
    ///
    /// `#[derive(...)]` attributes pass through to the generated
    /// tuple struct, so the newtype gets `Debug` / `Clone` / `Eq`
    /// "for free" from `Refined<String, NonEmpty>`'s pass-through
    /// impls — they don't have to be implemented by hand.
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub UserName: String, NonEmpty;
}

fn main() {
    // Admit: non-empty input passes the rule.
    let name = UserName::try_new("Ada".to_string()).unwrap();
    assert_eq!(name.as_inner(), "Ada");

    // Clone works because `#[derive(Clone)]` was passed through.
    let cloned = name.clone();
    assert_eq!(name, cloned);

    // The point: `parse(s: &str) -> UserName` cannot exist except
    // via `UserName::try_new`. Anywhere a `UserName` is in scope,
    // the "non-empty" invariant is structurally guaranteed —
    // downstream code doesn't have to re-check.
    let owned: String = name.into_inner();
    assert_eq!(owned, "Ada");

    // Reject: empty string fails the rule.
    let bad = UserName::try_new(String::new());
    assert!(bad.is_err());

    println!("user: {}", cloned.as_inner());
    println!("OK: `refinement!` newtype admits \"Ada\", rejects empty");
}
