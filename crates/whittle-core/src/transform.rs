//! String transformer rules.
//!
//! These adapters normalize the input *before* delegating to an inner
//! rule. The principle: a `Refined<String, Transformer<R>>` stores the
//! canonical form, not the input the user passed. `try_new("ABC")` and
//! `try_new("abc")` produce equal `Refined` values when wrapped in
//! `AsciiLowercase<R>`, because the carrier is normalized first.
//!
//! Tradeoff: silently rewriting input is a different semantic from
//! validation-only. Use these adapters only when canonical form is
//! actually part of the contract (e.g., hex hashes, hostnames, IANA
//! tokens). For invariants where the input form should be preserved
//! verbatim, use the validation-only rules directly.

use alloc::string::{String, ToString};
use core::marker::PhantomData;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// Lowercase ASCII letters in `raw` and delegate to `R`.
///
/// The stored carrier is the lowercased form. Two inputs that differ
/// only in ASCII case produce equal `Refined` values when wrapped in
/// `AsciiLowercase<R>`. Non-ASCII characters pass through unchanged
/// (this is `String::to_ascii_lowercase`'s contract; full Unicode
/// case folding is out of scope).
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::HexFixedAny;
/// use whittle_core::transform::AsciiLowercase;
///
/// // Admit any case, store lowercase.
/// let mixed = Refined::<String, AsciiLowercase<HexFixedAny<4>>>::try_new(
///     "ABcd".to_string(),
/// ).unwrap();
/// assert_eq!(mixed.as_inner(), "abcd");
///
/// // Same canonical form regardless of input case.
/// let lower = Refined::<String, AsciiLowercase<HexFixedAny<4>>>::try_new(
///     "abcd".to_string(),
/// ).unwrap();
/// assert_eq!(mixed.as_inner(), lower.as_inner());
///
/// // The inner rule still rejects: lowercasing `"GHIJ"` yields
/// // `"ghij"`, which is not hex.
/// let err = Refined::<String, AsciiLowercase<HexFixedAny<4>>>::try_new(
///     "GHIJ".to_string(),
/// );
/// err.unwrap_err();
/// # }
/// ```
pub struct AsciiLowercase<R>(PhantomData<fn() -> R>);

/// Uppercase ASCII letters in `raw` and delegate to `R`.
///
/// Symmetric counterpart to `AsciiLowercase`. Use when the canonical
/// form is uppercase (e.g., uppercase hex hashes in some APIs, IANA
/// language subtags rendered in uppercase).
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "hex")] {
/// use whittle_core::Refined;
/// use whittle_core::primitive::HexFixedAny;
/// use whittle_core::transform::AsciiUppercase;
///
/// // Admit any case, store uppercase.
/// let mixed = Refined::<String, AsciiUppercase<HexFixedAny<4>>>::try_new(
///     "ABcd".to_string(),
/// ).unwrap();
/// assert_eq!(mixed.as_inner(), "ABCD");
///
/// // Inner rule still rejects non-hex input post-transform.
/// let err = Refined::<String, AsciiUppercase<HexFixedAny<4>>>::try_new(
///     "ghij".to_string(),
/// );
/// err.unwrap_err();
/// # }
/// ```
pub struct AsciiUppercase<R>(PhantomData<fn() -> R>);

/// Trim leading and trailing whitespace from `raw` and delegate to `R`.
///
/// "Whitespace" is `char::is_whitespace` — the Unicode whitespace
/// definition `str::trim` uses. Composes with other transformers
/// (`Trim<AsciiLowercase<...>>`) and with validation rules (e.g.,
/// `Trim<NonEmpty>` to reject inputs that are empty *after* trimming).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::NonEmpty;
/// use whittle_core::transform::Trim;
///
/// // Admit padded input, store the trimmed form.
/// let padded = Refined::<String, Trim<NonEmpty>>::try_new(
///     "  hello  ".to_string(),
/// ).unwrap();
/// assert_eq!(padded.as_inner(), "hello");
///
/// // Reject input that is empty after trimming.
/// let blank = Refined::<String, Trim<NonEmpty>>::try_new(
///     "   ".to_string(),
/// );
/// blank.unwrap_err();
/// ```
pub struct Trim<R>(PhantomData<fn() -> R>);

impl<R> Rule<String> for AsciiLowercase<R>
where
    R: Rule<String>,
{
    type Error = R::Error;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        // `String::to_ascii_lowercase` allocates a fresh String; for
        // input that is already lowercase the carrier still goes
        // through one allocation. If profile-driven evidence shows
        // this matters, switch to `make_ascii_lowercase` (in-place).
        R::refine(raw.to_ascii_lowercase())
    }
}

impl<R> Rule<String> for AsciiUppercase<R>
where
    R: Rule<String>,
{
    type Error = R::Error;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        R::refine(raw.to_ascii_uppercase())
    }
}

impl<R> Rule<String> for Trim<R>
where
    R: Rule<String>,
{
    type Error = R::Error;

    #[inline]
    fn refine(raw: String) -> Result<String, Self::Error> {
        // `str::trim` returns a borrow; allocate a fresh String so
        // the carrier is owned. Avoiding allocation here would
        // require either reusing `raw`'s buffer (drains/copies) or
        // a non-`String` carrier — out of scope for the transformer.
        R::refine(raw.trim().to_string())
    }
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// Each transformer is implemented as `R::arbitrary_strategy()`
// composed with the transformer's normalisation step. The stored
// carrier is always the post-transform canonical form — the same
// guarantee `Rule::refine` makes.

#[cfg(feature = "proptest")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "matches `prop_map`'s `Fn(Self::Value) -> Out` signature"
)]
fn ascii_lowercase_string(raw: String) -> String {
    raw.to_ascii_lowercase()
}

#[cfg(feature = "proptest")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "matches `prop_map`'s `Fn(Self::Value) -> Out` signature"
)]
fn ascii_uppercase_string(raw: String) -> String {
    raw.to_ascii_uppercase()
}

#[cfg(feature = "proptest")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "matches `prop_map`'s `Fn(Self::Value) -> Out` signature"
)]
fn trim_to_owned(raw: String) -> String {
    raw.trim().to_string()
}

#[cfg(feature = "proptest")]
impl<R> ArbitraryRule<String> for AsciiLowercase<R>
where
    R: ArbitraryRule<String>,
{
    type Strategy =
        proptest::strategy::Map<<R as ArbitraryRule<String>>::Strategy, fn(String) -> String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        R::arbitrary_strategy().prop_map(ascii_lowercase_string)
    }
}

#[cfg(feature = "proptest")]
impl<R> ArbitraryRule<String> for AsciiUppercase<R>
where
    R: ArbitraryRule<String>,
{
    type Strategy =
        proptest::strategy::Map<<R as ArbitraryRule<String>>::Strategy, fn(String) -> String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        R::arbitrary_strategy().prop_map(ascii_uppercase_string)
    }
}

#[cfg(feature = "proptest")]
impl<R> ArbitraryRule<String> for Trim<R>
where
    R: ArbitraryRule<String>,
{
    // Contract: `R`'s strategy must yield strings whose trimmed
    // form is still admissible under `R::refine` (the inner rule
    // sees the trimmed string). For rules whose admissible region
    // is invariant under trimming — `NonEmpty` over non-whitespace
    // characters, alnum-only regimes, hex — this holds trivially.
    type Strategy =
        proptest::strategy::Map<<R as ArbitraryRule<String>>::Strategy, fn(String) -> String>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        R::arbitrary_strategy().prop_map(trim_to_owned)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{AsciiLowercase, AsciiUppercase, Trim};
    use crate::primitive::NonEmpty;
    use crate::rule::Refined;

    #[cfg(feature = "hex")]
    use crate::primitive::{HexFixedAny, HexFixedLower, StringError};

    // ─── AsciiLowercase normalize + admit (hex feature). ─────────

    #[cfg(feature = "hex")]
    #[test]
    fn ascii_lowercase_normalizes_and_admits() {
        // Wrap a case-insensitive rule. The stored carrier must be
        // the lowercased form, not the input the user passed.
        let r: Refined<String, AsciiLowercase<HexFixedAny<4>>> =
            Refined::try_new("ABcd".to_string()).unwrap();
        assert_eq!(r.as_inner(), "abcd");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn ascii_lowercase_idempotent() {
        // Already lowercase under a strict-lowercase inner rule:
        // admit unchanged.
        let r: Refined<String, AsciiLowercase<HexFixedLower<4>>> =
            Refined::try_new("abcd".to_string()).unwrap();
        assert_eq!(r.as_inner(), "abcd");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn ascii_lowercase_rejects_after_normalize() {
        // Lowercase first, then validate against a strict-lowercase
        // hex rule. `"GHIJ"` lowercases to `"ghij"`, still not hex,
        // so the inner rule's `BadChar` surfaces.
        let bad: Result<Refined<String, AsciiLowercase<HexFixedLower<4>>>, _> =
            Refined::try_new("GHIJ".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 0 });
    }

    // ─── AsciiUppercase normalize + admit. ───────────────────────

    #[cfg(feature = "hex")]
    #[test]
    fn ascii_uppercase_normalizes_and_admits() {
        let r: Refined<String, AsciiUppercase<HexFixedAny<4>>> =
            Refined::try_new("ABcd".to_string()).unwrap();
        assert_eq!(r.as_inner(), "ABCD");
    }

    #[cfg(feature = "hex")]
    #[test]
    fn ascii_uppercase_rejects_after_normalize() {
        // Uppercasing `"ghij"` produces `"GHIJ"`, still not hex.
        let bad: Result<Refined<String, AsciiUppercase<HexFixedAny<4>>>, _> =
            Refined::try_new("ghij".to_string());
        assert_eq!(bad.unwrap_err(), StringError::BadChar { offset: 0 });
    }

    // ─── Trim normalize + admit / reject. ────────────────────────

    #[test]
    fn trim_removes_whitespace() {
        let r: Refined<String, Trim<NonEmpty>> = Refined::try_new("  hello  ".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello");
    }

    #[test]
    fn trim_then_validate_rejects_empty_after_trim() {
        // Only whitespace: empty after trimming, so `NonEmpty`
        // rejects the canonical form.
        let bad: Result<Refined<String, Trim<NonEmpty>>, _> = Refined::try_new("   ".to_string());
        bad.unwrap_err();
    }

    #[test]
    fn trim_admits_already_trimmed() {
        // Per-monomorphization Ok path for `Trim<NonEmpty>` that
        // does not involve any leading/trailing whitespace.
        let r: Refined<String, Trim<NonEmpty>> = Refined::try_new("hello".to_string()).unwrap();
        assert_eq!(r.as_inner(), "hello");
    }

    // ─── Transformers compose. ───────────────────────────────────

    #[cfg(feature = "hex")]
    #[test]
    fn transformers_compose() {
        // Outer transformer runs first: `Trim` strips whitespace,
        // then `AsciiLowercase` lowercases, then `HexFixedAny<4>`
        // validates.
        let r: Refined<String, Trim<AsciiLowercase<HexFixedAny<4>>>> =
            Refined::try_new("  ABCD  ".to_string()).unwrap();
        assert_eq!(r.as_inner(), "abcd");
    }

    // ─── Proptest: stored carrier must be canonical. ─────────────

    #[cfg(all(feature = "proptest", feature = "hex"))]
    proptest::proptest! {
        /// The post-transform invariant: every value stored in
        /// `Refined<String, AsciiLowercase<HexFixedAny<2>>>` must
        /// already be in its canonical form — equal to its own
        /// ASCII-lowercase.
        ///
        /// `Refined`'s `Arbitrary` impl drives `String::arbitrary`,
        /// whose distribution over arbitrary Unicode never produces
        /// hex strings at a tractable rate. Drive with a bounded
        /// regex strategy that emits any-case hex pairs, then route
        /// through `try_new` so the transformer + inner rule both
        /// run on the input.
        #[test]
        fn ascii_lowercase_arbitrary_is_canonical(
            raw in "[0-9a-fA-F]{2}"
        ) {
            let value: Refined<String, AsciiLowercase<HexFixedAny<2>>>
                = Refined::try_new(raw).unwrap();
            proptest::prop_assert_eq!(
                value.as_inner(),
                &value.as_inner().to_ascii_lowercase(),
            );
        }
    }
}
