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

//! `Or<R1, R2>`: either rule may accept.
//!
//! `A::refine` runs first; on `Ok` its output is the result. On
//! `Err` the *original* input is tried against `B::refine`. When
//! both reject, the error is `OrError { left, right }` carrying
//! both inner errors.
//!
//! **Anti-pattern warning.** Same lesson as `And`: `OrError<EA,
//! EB>` is fine internally but ugly as a domain error. Wrap the
//! `Or` in a nominal newtype and present a flat enum the caller
//! can pattern-match against. The closing snippet shows the
//! pattern; `flat-domain-error.rs` is the canonical reference.
//!
//! Note: `Or` requires `T: Clone` because the original input must
//! be preserved for the second attempt. This is the only `Clone`
//! constraint in the kernel.

use whittle::primitive::{AtLeast, AtMost, NumericError};
use whittle::{Or, OrError, Refined};

fn main() {
    // "Out of the middle band": value <= 10 OR value >= 100.
    type OutsideMiddle = Or<AtMost<10>, AtLeast<100>>;

    // Admit via the left alternative.
    let small: Refined<i32, OutsideMiddle> = Refined::try_new(5).unwrap();
    assert_eq!(*small.as_inner(), 5);

    // Admit via the right alternative.
    let big: Refined<i32, OutsideMiddle> = Refined::try_new(150).unwrap();
    assert_eq!(*big.as_inner(), 150);

    // Reject: 50 falls into the forbidden middle. Both
    // alternatives reject, and `OrError` carries both inner errors.
    let stuck = Refined::<i32, OutsideMiddle>::try_new(50).unwrap_err();
    let both: OrError<NumericError, NumericError> = stuck;
    assert_eq!(both.left, NumericError::OutOfRange { value: 50 });
    assert_eq!(both.right, NumericError::OutOfRange { value: 50 });

    // ─── Flattening `OrError` into a domain enum. ───────────────
    //
    // The pattern to copy: a newtype with a flat error enum.
    // Callers see one variant ("not extreme"), not the composition
    // shape ("Left and Right both rejected with NumericError").

    #[derive(Debug, PartialEq, Eq)]
    enum ExtremeError {
        NotExtreme { value: i128 },
    }

    #[derive(Debug)]
    struct Extreme(Refined<i32, OutsideMiddle>);

    impl Extreme {
        fn try_new(raw: i32) -> Result<Self, ExtremeError> {
            Refined::try_new(raw).map(Self).map_err(|err| {
                // Both inner errors carry the same offending value;
                // collapse to a single variant. `NumericError` is
                // `#[non_exhaustive]`, so the match must include a
                // catch-all even though only one variant exists today.
                let value = match err.left {
                    NumericError::OutOfRange { value } => value,
                    _ => i128::from(raw),
                };
                ExtremeError::NotExtreme { value }
            })
        }
    }

    let edge = Extreme::try_new(150).unwrap();
    assert_eq!(*edge.0.as_inner(), 150);

    let stuck = Extreme::try_new(50).unwrap_err();
    assert_eq!(stuck, ExtremeError::NotExtreme { value: 50 });

    println!("OK: Or<L, R> admits either side; flat domain enum hides OrError");
}
