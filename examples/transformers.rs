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

//! Transformers: rewrite input before the inner rule runs.
//!
//! `AsciiLowercase<R>`, `AsciiUppercase<R>`, and `Trim<R>` are
//! adapters that normalize the input first and then delegate to
//! `R`. The stored carrier is the canonical form — `try_new(" Hi
//! ")` and `try_new("hi")` produce equal `Refined` values when
//! wrapped in `Trim<AsciiLowercase<NonEmpty>>`.
//!
//! Use these when canonical form is part of the contract (hex
//! hashes, hostnames, IANA tokens). For invariants where the
//! input should be preserved verbatim, stick with validation-only
//! rules.

use whittle::primitive::NonEmpty;
use whittle::transform::{AsciiLowercase, AsciiUppercase, Trim};
use whittle::Refined;

fn main() {
    // `AsciiLowercase<R>` lowercases first, then validates with `R`.
    let lower: Refined<String, AsciiLowercase<NonEmpty>> = Refined::try_new("HELLO".to_string()).unwrap();
    assert_eq!(lower.as_inner(), "hello");

    // `AsciiUppercase<R>` is the symmetric counterpart.
    let upper: Refined<String, AsciiUppercase<NonEmpty>> = Refined::try_new("hello".to_string()).unwrap();
    assert_eq!(upper.as_inner(), "HELLO");

    // `Trim<R>` strips leading + trailing whitespace, then validates.
    let trimmed: Refined<String, Trim<NonEmpty>> = Refined::try_new("  hi  ".to_string()).unwrap();
    assert_eq!(trimmed.as_inner(), "hi");

    // Whitespace-only input is empty after trimming, so the inner
    // `NonEmpty` rule rejects.
    let blank = Refined::<String, Trim<NonEmpty>>::try_new("   ".to_string());
    assert!(blank.is_err());

    // Transformers compose. Outer runs first: `Trim` strips, then
    // `AsciiLowercase` lowercases, then `NonEmpty` validates.
    type Canonical = Trim<AsciiLowercase<NonEmpty>>;
    let canon: Refined<String, Canonical> = Refined::try_new(" Hello ".to_string()).unwrap();
    assert_eq!(canon.as_inner(), "hello");

    // Two inputs that differ only in case + surrounding whitespace
    // produce equal `Refined` values under the same composition.
    let other: Refined<String, Canonical> = Refined::try_new("hello".to_string()).unwrap();
    assert_eq!(canon.as_inner(), other.as_inner());

    println!("canonical: {}", canon.as_inner());
    println!("OK: transformers store canonical form, not input");
}
