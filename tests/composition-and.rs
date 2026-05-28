//! `And<R1, R2>`: both rules must accept.
//!
//! `And` is the primitive composition operator. `A::refine` runs
//! first; on success its (possibly canonicalised) output flows
//! into `B::refine`. Both rules must share the same `Rule::Error`
//! type; that shared type is the composition's `Self::Error`, so
//! the rules' flat enum surfaces directly with no positional
//! `Left`/`Right` wrapping.
//!
//! **Newtype pattern.** Even with the shared-error collapse,
//! `Refined<String, And<LenChars<3, 8>, EachChar<...>>>` exposed as
//! a public API still leaks the composition shape into every
//! caller's type signature. The fix is unchanged: wrap the
//! composition in a nominal newtype with a flat domain enum
//! (see the closing test here, and `flat-domain-error.rs` for the
//! headline pattern).

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::primitive::{
    AtLeast, AtMost, EachChar, IdentChar, LenChars, NumericError, StringError,
};
use whittle::{And, Refined, Rule};

#[test]
fn and_admits_when_both_sides_accept_and_returns_the_rules_shared_error() {
    // `0..=100` expressed via `And`. `Within<0, 100>` would be a
    // better surface API for callers, but the explicit shape is
    // what you reach for when no library primitive matches. Both
    // inner rules produce `NumericError`, so the composition's
    // error is `NumericError` directly.
    type InRange = And<AtLeast<0>, AtMost<100>>;

    let ok: Refined<i32, InRange> = Refined::try_new(50).unwrap();
    assert_eq!(*ok.as_inner(), 50);

    // First rule rejects: the inner-rule error surfaces directly.
    let low = Refined::<i32, InRange>::try_new(-1).unwrap_err();
    assert_eq!(low, NumericError::OutOfRange { value: -1 });

    // Second rule rejects (first accepted): same flat enum.
    let high = Refined::<i32, InRange>::try_new(101).unwrap_err();
    assert_eq!(high, NumericError::OutOfRange { value: 101 });
}

#[test]
fn and_composes_string_length_and_character_predicate() {
    // The same shape for strings: 1..=10 char identifier-body.
    type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;
    let id: Refined<String, Ident> = Refined::try_new("user_42".to_string()).unwrap();
    assert_eq!(id.as_inner(), "user_42");

    // Touch `Rule` so the import isn't unused; the rule's `refine`
    // is the moral equivalent of `Refined::try_new`'s inner call.
    let raw = <Ident as Rule<String>>::refine("u".to_string()).unwrap();
    assert_eq!(raw, "u");
}

#[test]
fn newtype_wraps_and_composition_with_a_flat_domain_enum() {
    // ─── Domain newtype around an `And` composition. ────────────
    //
    // Both inner rules now share `StringError`, so the match on
    // `try_new`'s error is a direct 1:1 mapping into the flat
    // domain enum. The catch-all is required because `StringError`
    // is `#[non_exhaustive]`, but the named arms already cover
    // every variant the composition can emit.
    type Ident = And<LenChars<1, 10>, EachChar<IdentChar>>;

    #[derive(Debug, PartialEq, Eq)]
    enum LabelError {
        Length { actual: usize },
        BadChar { offset: usize },
    }

    #[derive(Debug)]
    struct Label(Refined<String, Ident>);

    impl Label {
        fn try_new(raw: String) -> Result<Self, LabelError> {
            Refined::try_new(raw).map(Self).map_err(|err: StringError| match err {
                StringError::CharCountOutOfRange { actual } => LabelError::Length { actual },
                StringError::BadChar { offset } => LabelError::BadChar { offset },
                // `StringError` is `#[non_exhaustive]`; the
                // catch-all is required even though `LenChars` and
                // `EachChar` only emit the two variants above.
                other => unreachable!("unexpected inner StringError variant: {other:?}"),
            })
        }
    }

    let label = Label::try_new("ok_42".to_string()).unwrap();
    assert_eq!(label.0.as_inner(), "ok_42");

    let too_long = Label::try_new("a".repeat(20)).unwrap_err();
    assert_eq!(too_long, LabelError::Length { actual: 20 });

    let bad_byte = Label::try_new("ok-42".to_string()).unwrap_err();
    assert_eq!(bad_byte, LabelError::BadChar { offset: 2 });
}
