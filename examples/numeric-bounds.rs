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

//! Numeric primitives: bounded ranges, sign, non-zero.
//!
//! Walks through `Within`, `AtLeast`, `AtMost`, `NonZero`,
//! `Positive`, `Negative`. The headline observation: `Within<MIN,
//! MAX>` exposes the flat `NumericError` — its internal
//! `And<AtLeast, AtMost>` composition is hidden. Domain newtypes
//! should follow that pattern: compose internally, present a
//! single flat error externally.
//!
//! Use this example as a reference when picking a numeric rule
//! for a new domain type: which primitive expresses the
//! invariant, and what error variant surfaces to the caller.

use whittle::primitive::{AtLeast, AtMost, Negative, NonZero, NumericError, Positive, Within};
use whittle::Refined;

fn main() {
    // `Within<MIN, MAX>` admits values in `MIN..=MAX` inclusive.
    let mid: Refined<i32, Within<0, 100>> = Refined::try_new(50).unwrap();
    assert_eq!(*mid.as_inner(), 50);

    // `Within` is a nominal newtype. Both below-min and above-max
    // surface as the same flat `NumericError::OutOfRange`. The
    // internal `And<AtLeast<MIN>, AtMost<MAX>>` composition is an
    // implementation detail — callers never see `AndError`.
    let low_err = Refined::<i32, Within<0, 100>>::try_new(-1).unwrap_err();
    assert_eq!(low_err, NumericError::OutOfRange { value: -1 });
    let high_err = Refined::<i32, Within<0, 100>>::try_new(101).unwrap_err();
    assert_eq!(high_err, NumericError::OutOfRange { value: 101 });

    // `AtLeast` / `AtMost` are the one-sided primitives `Within`
    // composes from; both also expose the flat error.
    let above: Refined<i32, AtLeast<10>> = Refined::try_new(10).unwrap();
    let below: Refined<i32, AtMost<100>> = Refined::try_new(100).unwrap();
    assert_eq!(*above.as_inner(), 10);
    assert_eq!(*below.as_inner(), 100);

    // Sign and non-zero rules use the same `NumericError`.
    let pos: Refined<i32, Positive> = Refined::try_new(1).unwrap();
    let neg: Refined<i32, Negative> = Refined::try_new(-1).unwrap();
    let nz: Refined<u32, NonZero> = Refined::try_new(5).unwrap();
    assert_eq!(*pos.as_inner(), 1);
    assert_eq!(*neg.as_inner(), -1);
    assert_eq!(*nz.as_inner(), 5);

    // The standard rejections. The error shape is `NumericError`
    // for every numeric primitive — exact variants below so the
    // training corpus carries the precise pattern, not just
    // "something failed".
    let pos_zero = Refined::<i32, Positive>::try_new(0).unwrap_err();
    assert_eq!(pos_zero, NumericError::OutOfRange { value: 0 });

    let neg_zero = Refined::<i32, Negative>::try_new(0).unwrap_err();
    assert_eq!(neg_zero, NumericError::OutOfRange { value: 0 });

    // `NonZero` carries the offending value too. Widened `i128` is
    // the universal carrier across every `Numeric` impl.
    let nz_zero = Refined::<u32, NonZero>::try_new(0).unwrap_err();
    assert_eq!(nz_zero, NumericError::OutOfRange { value: 0 });

    println!("OK: numeric primitives admit/reject with flat NumericError");
}
