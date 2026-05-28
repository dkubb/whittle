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

//! String validation primitives: length, content, head.
//!
//! Covers `LenChars`, `LenBytes`, `NonEmpty`, `EachChar`, and
//! `FirstChar`. The crucial distinction: `LenChars` counts
//! Unicode scalar values, `LenBytes` counts UTF-8 bytes — the
//! same input can pass one and fail the other.
//!
//! Use this when modelling fields with explicit length budgets
//! (database `VARCHAR(N)`, API max-length contracts, identifier
//! shapes). Pick `LenChars` for "what a human sees" limits and
//! `LenBytes` for storage / wire-format limits.

use whittle::primitive::{
    AsciiAlphanumeric, EachChar, FirstChar, IdentStart, LenBytes, LenChars, NonEmpty, StringError,
};
use whittle::Refined;

fn main() {
    // `LenChars<MIN, MAX>` counts characters, not bytes.
    let short: Refined<String, LenChars<1, 5>> = Refined::try_new("abc".to_string()).unwrap();
    assert_eq!(short.as_inner(), "abc");

    // "é" is one Unicode scalar value but two UTF-8 bytes. The
    // single-char rule admits it; the single-byte rule rejects it.
    let one_char: Refined<String, LenChars<1, 1>> = Refined::try_new("é".to_string()).unwrap();
    assert_eq!(one_char.as_inner(), "é");

    let byte_err = Refined::<String, LenBytes<1, 1>>::try_new("é".to_string()).unwrap_err();
    assert_eq!(byte_err, StringError::ByteLenOutOfRange { actual: 2 });

    // `NonEmpty` is the smallest length check — empty rejects.
    let ne: Refined<String, NonEmpty> = Refined::try_new("x".to_string()).unwrap();
    assert_eq!(ne.as_inner(), "x");
    let empty_err = Refined::<String, NonEmpty>::try_new(String::new()).unwrap_err();
    assert_eq!(empty_err, StringError::Empty);

    // `EachChar<P>` walks the string and reports the byte offset
    // of the first character that fails the predicate.
    let alnum: Refined<String, EachChar<AsciiAlphanumeric>> =
        Refined::try_new("user42".to_string()).unwrap();
    assert_eq!(alnum.as_inner(), "user42");
    let bad_char =
        Refined::<String, EachChar<AsciiAlphanumeric>>::try_new("user-42".to_string()).unwrap_err();
    assert_eq!(bad_char, StringError::BadChar { offset: 4 });

    // `FirstChar<P>` only inspects the head; the empty string is
    // vacuously admissible. Compose with `LenChars<1, MAX>` when
    // you want to reject empty.
    let head: Refined<String, FirstChar<IdentStart>> = Refined::try_new("name".to_string()).unwrap();
    assert_eq!(head.as_inner(), "name");
    let bad_head =
        Refined::<String, FirstChar<IdentStart>>::try_new("1abc".to_string()).unwrap_err();
    assert_eq!(bad_head, StringError::BadChar { offset: 0 });

    println!("OK: string primitives distinguish chars from bytes");
}
