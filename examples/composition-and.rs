// Examples are interactive demonstrations: they use `println!` to
// confirm what was demonstrated and `unwrap()` to keep the focus on
// the API, not error plumbing. The workspace lints would otherwise
// deny both.
#![expect(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::items_after_statements,
    reason = "interactive demonstration: println!, unwrap, and items_after_statements keep the focus on the API"
)]

//! `And<R1, R2>`: both rules must accept.
//!
//! `And` is the primitive composition operator. `A::refine` runs
//! first; on success its (possibly canonicalised) output flows
//! into `B::refine`. Failures surface as `AndError::Left` or
//! `AndError::Right`.
//!
//! **Anti-pattern warning.** `AndError<EA, EB>` is a fine internal
//! representation, but it is a poor *domain* error type. A
//! `Refined<String, And<LenChars<3, 8>, EachChar<...>>>` exposed as
//! a public API leaks the composition shape into every caller.
//! The fix: wrap the composition in a nominal newtype with a flat
//! domain enum (see the closing snippet here, and
//! `flat-domain-error.rs` for the headline pattern).

use whittle::primitive::{AtLeast, AtMost, EachChar, IdentChar, LenChars, NumericError, StringError};
use whittle::{And, AndError, Refined, Rule};

fn main() {
    // `0..=100` expressed via `And`. `Within<0, 100>` would be a
    // better surface API for callers, but the explicit shape is
    // what you reach for when no library primitive matches.
    type InRange = And<AtLeast<0>, AtMost<100>>;

    let ok: Refined<i32, InRange> = Refined::try_new(50).unwrap();
    assert_eq!(*ok.as_inner(), 50);

    // `Left` carries the inner-rule error from the first side.
    let low = Refined::<i32, InRange>::try_new(-1).unwrap_err();
    assert_eq!(low, AndError::Left(NumericError::OutOfRange { value: -1 }));

    // `Right` carries the inner-rule error from the second side
    // — only reached when `Left` already accepted.
    let high = Refined::<i32, InRange>::try_new(101).unwrap_err();
    assert_eq!(
        high,
        AndError::Right(NumericError::OutOfRange { value: 101 }),
    );

    // The same shape for strings: 1..=10 char identifier-body.
    type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
    let id: Refined<String, Ident> = Refined::try_new("user_42".to_string()).unwrap();
    assert_eq!(id.as_inner(), "user_42");

    // ─── Flattening the composition into a domain error. ────────
    //
    // The pattern to copy: hide `And` behind a newtype and present
    // a flat enum. Callers see `LabelError::Length` or
    // `LabelError::BadChar`, never `AndError::Left | Right`.

    #[derive(Debug, PartialEq, Eq)]
    enum LabelError {
        Length { actual: usize },
        BadChar { offset: usize },
    }

    #[derive(Debug)]
    struct Label(Refined<String, Ident>);

    impl Label {
        fn try_new(raw: String) -> Result<Self, LabelError> {
            Refined::try_new(raw)
                .map(Self)
                .map_err(|err| match err {
                    AndError::Left(StringError::CharCountOutOfRange { actual }) => {
                        LabelError::Length { actual }
                    }
                    AndError::Right(StringError::BadChar { offset }) => {
                        LabelError::BadChar { offset }
                    }
                    // Other variants of `StringError` are
                    // unreachable here because `LenChars` and
                    // `EachChar` only emit the two above.
                    AndError::Left(other) | AndError::Right(other) => unreachable!(
                        "unexpected inner StringError variant: {other:?}",
                    ),
                })
        }
    }

    let label = Label::try_new("ok_42".to_string()).unwrap();
    assert_eq!(label.0.as_inner(), "ok_42");

    let too_long = Label::try_new("a".repeat(20)).unwrap_err();
    assert_eq!(too_long, LabelError::Length { actual: 20 });

    let bad_byte = Label::try_new("ok-42".to_string()).unwrap_err();
    assert_eq!(bad_byte, LabelError::BadChar { offset: 2 });

    println!("OK: And<L, R> composes; flat domain enum hides the AndError shape");

    // Touch `Rule` so the import isn't unused; the rule's `refine`
    // is the moral equivalent of `Refined::try_new`'s inner call.
    let _ = <Ident as Rule<String>>::refine("u".to_string()).unwrap();
}
