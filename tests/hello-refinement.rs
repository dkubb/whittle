//! The simplest possible whittle program.
//!
//! Defines a tiny `Rule<i32>` that admits only positive integers,
//! then constructs a `Refined<i32, _>` through its `try_new` gate
//! and reads it back via `as_inner` / `into_inner`.
//!
//! Use this pattern when the library-supplied primitives don't fit
//! your invariant and you want the smallest possible hand-written
//! rule. For "value > 0" you would normally reach for
//! `whittle::primitive::Positive`; the custom rule here exists to
//! show the trait surface stripped of any other machinery.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::{Refined, Rule};

/// Marker type identifying the rule. Rule markers are uninhabited
/// (`enum NoVariants {}`) because they carry no runtime state.
enum Positive {}

/// Domain error for the rule. Hand-written rather than imported so
/// the example stays self-contained.
#[derive(Debug, PartialEq, Eq)]
struct NotPositive {
    value: i32,
}

impl Rule<i32> for Positive {
    type Error = NotPositive;

    fn refine(raw: i32) -> Result<i32, Self::Error> {
        if raw > 0 {
            Ok(raw)
        } else {
            Err(NotPositive { value: raw })
        }
    }
}

#[test]
fn hand_written_rule_admits_positive_and_rejects_non_positive() {
    // Admit: 7 is strictly positive, so `try_new` succeeds.
    let refined: Refined<i32, Positive> = Refined::try_new(7).unwrap();

    // `as_inner` borrows; the carrier itself stays put.
    assert_eq!(*refined.as_inner(), 7);

    // `into_inner` consumes and returns the raw value.
    let raw: i32 = refined.into_inner();
    assert_eq!(raw, 7);

    // Reject: -1 fails the rule; the typed error carries the
    // offending value so callers can report it.
    let err = Refined::<i32, Positive>::try_new(-1).unwrap_err();
    assert_eq!(err, NotPositive { value: -1 });
}
