//! The `Rule<T>` trait and `Refined<T, R>` carrier.
//!
//! Existence of a `Refined<T, R>` is the proof that `R::refine`
//! returned `Ok` on its inner value at construction time. There is no
//! separate `Witness` token.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

/// A narrowing rule that maps raw `T` into the rule's admissible
/// state space.
///
/// `refine` consumes the input by value so rules may canonicalise
/// (trim, lowercase, NFC-normalise, ...) rather than only inspect.
/// Rules whose narrowing is purely a predicate return `Ok(raw)`
/// unchanged on admissible input.
///
/// # Soundness obligation
///
/// For every implementation, `R::refine(x) == Ok(y)` implies `y` is
/// admissible under `R`. Implementers are responsible for discharging
/// this obligation; the type system cannot verify it.
///
/// `'static` on the trait is on the rule marker `R`. `T: 'static` is
/// required because the `Schema` reflection (to be added in a later
/// commit) uses `TypeId::of::<T>()`.
pub trait Rule<T>: Sized + 'static
where
    T: 'static,
{
    /// Construction-time error type.
    type Error;

    /// The narrowing morphism. Returns the (possibly canonicalised)
    /// admissible value on success, or a typed error on rejection.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` when `raw` cannot be narrowed into the
    /// admissible state space.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::{Refined, Rule};
    ///
    /// /// Accepts only non-negative `i32`.
    /// enum NonNeg {}
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// struct Negative;
    ///
    /// impl Rule<i32> for NonNeg {
    ///     type Error = Negative;
    ///     fn refine(raw: i32) -> Result<i32, Self::Error> {
    ///         if raw >= 0 { Ok(raw) } else { Err(Negative) }
    ///     }
    /// }
    ///
    /// assert_eq!(<NonNeg as Rule<i32>>::refine(7), Ok(7));
    /// assert_eq!(<NonNeg as Rule<i32>>::refine(-1), Err(Negative));
    ///
    /// // `Refined::try_new` is the standard construction path.
    /// let ok: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
    /// assert_eq!(*ok.as_inner(), 7);
    /// ```
    fn refine(raw: T) -> Result<T, Self::Error>;
}

/// A refined value: a `T` whose inner contents satisfy rule `R`.
///
/// `#[repr(transparent)]` plus a zero-sized phantom guarantees the
/// runtime layout is identical to `T`. Niche optimisations on `T`
/// are preserved.
///
/// The struct itself does not bound `R: Rule<T>`: the bound is
/// applied on impl blocks where it is required (notably `try_new`).
/// This lets accessor and trait impls compile without restating the
/// bound everywhere.
#[repr(transparent)]
pub struct Refined<T, R> {
    inner: T,
    rule: PhantomData<fn() -> R>,
}

impl<T, R> Refined<T, R>
where
    T: 'static,
    R: Rule<T>,
{
    /// Narrow `raw` through `R::refine` and wrap on success.
    ///
    /// This is the sole public construction path for a refined value.
    /// `Deserialize` and any other deserialisation route MUST go
    /// through this method (or a named newtype's `try_new` delegate)
    /// so that no admissible code path produces a refined value
    /// without running the rule's narrowing morphism.
    ///
    /// # Errors
    ///
    /// Forwards the rule's `R::Error` when `R::refine` rejects `raw`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::{NumericError, Within};
    ///
    /// // Admit: value lies in `0..=100`.
    /// let ok: Refined<i32, Within<0, 100>> = Refined::try_new(42).unwrap();
    /// assert_eq!(*ok.as_inner(), 42);
    ///
    /// // Reject: value is outside the admissible range. `Within`
    /// // is a nominal domain newtype with a flat `NumericError`,
    /// // so the composition machinery does not leak.
    /// let err = Refined::<i32, Within<0, 100>>::try_new(-1).unwrap_err();
    /// assert_eq!(err, NumericError::OutOfRange { value: -1 });
    /// ```
    #[inline]
    pub fn try_new(raw: T) -> Result<Self, R::Error> {
        match R::refine(raw) {
            Ok(inner) => Ok(Self {
                inner,
                rule: PhantomData,
            }),
            Err(err) => Err(err),
        }
    }
}

impl<T, R> Refined<T, R> {
    /// Borrow the inner value.
    ///
    /// Returning a shared reference is proof-preserving: callers
    /// observe the inner value but cannot reconstruct a
    /// `Refined<T, R>` from the borrow without going back through
    /// `try_new`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::Within;
    ///
    /// let r: Refined<i32, Within<0, 100>> = Refined::try_new(42).unwrap();
    /// assert_eq!(*r.as_inner(), 42);
    /// ```
    #[inline]
    pub const fn as_inner(&self) -> &T {
        &self.inner
    }

    /// Consume the carrier and return the inner value.
    ///
    /// `into_inner` is proof-erasing: the caller takes ownership of
    /// `T` but must re-run `try_new` to reconstruct a refined value.
    /// The library exposes no `as_mut`-style mutating accessor; the
    /// only mutation path is `into_inner` → mutate → `try_new`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::Within;
    ///
    /// let r: Refined<i32, Within<0, 100>> = Refined::try_new(42).unwrap();
    /// let inner: i32 = r.into_inner();
    /// assert_eq!(inner, 42);
    /// ```
    #[inline]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

// ─── Pass-through impls. Implemented manually rather than via
//      `#[derive]` so that the bounds are `where T: ...` (the rule
//      marker `R` does not need to satisfy the inner trait). ──────

impl<T: fmt::Debug, R> fmt::Debug for Refined<T, R> {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print as the inner value's Debug, no rule wrapper noise.
        self.inner.fmt(formatter)
    }
}

impl<T: Clone, R> Clone for Refined<T, R> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            rule: PhantomData,
        }
    }
}

impl<T: Copy, R> Copy for Refined<T, R> {}

impl<T: Hash, R> Hash for Refined<T, R> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<T: PartialEq, R> PartialEq for Refined<T, R> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Eq, R> Eq for Refined<T, R> {}

impl<T: PartialOrd, R> PartialOrd for Refined<T, R> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T: Ord, R> Ord for Refined<T, R> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

// ─── Serde. `Serialize` forwards to the inner value (refined
//      values look identical on the wire). `Deserialize` runs
//      the deserialized raw `T` through `R::refine`, so the only
//      construction path remains `try_new` — no escape hatch.

#[cfg(feature = "serde")]
impl<T, R> serde::Serialize for Refined<T, R>
where
    T: serde::Serialize,
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

/// Deserialize a `Refined<T, R>` by deserializing `T` first, then
/// running the rule via `Refined::try_new`.
///
/// **Unknown-field policy is `T`'s decision, not whittle's.**
/// `Refined<T, R>` has no visibility into `T`'s field-level
/// deserialization, so it cannot enforce
/// `#[serde(deny_unknown_fields)]` from the outside — serde's
/// data model doesn't expose field-level callbacks to outer
/// adapters. To reject unknown fields, put the attribute on the
/// inner type:
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use serde::Deserialize;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{LenChars, Within};
///
/// #[derive(Debug, Deserialize)]
/// #[serde(deny_unknown_fields)]
/// struct UserInput {
///     name: Refined<String, LenChars<1, 64>>,
///     age:  Refined<u8, Within<0, 150>>,
/// }
///
/// // Accepted.
/// let _ok: UserInput = serde_json::from_str(
///     r#"{ "name": "Alice", "age": 30 }"#
/// ).unwrap();
///
/// // Rejected — unknown field "email".
/// let err = serde_json::from_str::<UserInput>(
///     r#"{ "name": "Alice", "age": 30, "email": "x" }"#,
/// )
/// .unwrap_err();
/// assert!(err.to_string().contains("unknown field"));
/// # }
/// ```
#[cfg(feature = "serde")]
impl<'de, T, R> serde::Deserialize<'de> for Refined<T, R>
where
    T: serde::Deserialize<'de> + 'static,
    R: Rule<T>,
    R::Error: fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = T::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

// ─── Proptest `Arbitrary`. Generates raw `T` via the inner
//      `Arbitrary` strategy, then runs it through `R::refine`
//      and keeps only admissible values. Downstream property
//      tests can write `let r: Refined<T, R> = arb(...);`
//      without `prop_assume!`-style filtering.
//
//      Note: relies on rejection sampling. If the admissible
//      region is sparse under the inner strategy, proptest may
//      exhaust its retry budget. For very narrow rules, supply a
//      custom strategy that produces admissible values directly.

#[cfg(feature = "proptest")]
impl<T, R> proptest::arbitrary::Arbitrary for Refined<T, R>
where
    T: proptest::arbitrary::Arbitrary + 'static,
    R: Rule<T> + 'static,
{
    type Parameters = T::Parameters;
    type Strategy = proptest::strategy::FilterMap<T::Strategy, fn(T) -> Option<Self>>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        T::arbitrary_with(args)
            .prop_filter_map("value rejected by rule", |raw| Self::try_new(raw).ok())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use super::{Refined, Rule};
    use alloc::format;

    /// Test rule: accept only non-negative i32.
    enum NonNeg {}

    #[derive(Debug, PartialEq, Eq)]
    struct Negative;

    impl core::fmt::Display for Negative {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("negative")
        }
    }

    impl Rule<i32> for NonNeg {
        type Error = Negative;
        fn refine(raw: i32) -> Result<i32, Self::Error> {
            if raw >= 0 { Ok(raw) } else { Err(Negative) }
        }
    }

    /// Test rule: identity (always succeeds). Useful for testing the
    /// `try_new` mechanics independent of a specific narrowing.
    enum Always {}
    #[derive(Debug)]
    struct NeverHappens;
    impl Rule<i32> for Always {
        type Error = NeverHappens;
        fn refine(raw: i32) -> Result<i32, Self::Error> {
            Ok(raw)
        }
    }

    /// Test rule: identity on String. Used by tests that need a
    /// non-`Copy` inner type so `clone()` is meaningful.
    enum AnyString {}
    impl Rule<alloc::string::String> for AnyString {
        type Error = core::convert::Infallible;
        fn refine(raw: alloc::string::String) -> Result<alloc::string::String, Self::Error> {
            Ok(raw)
        }
    }

    #[test]
    fn try_new_accepts_admissible_input() {
        let r: Refined<i32, NonNeg> = Refined::try_new(42).expect("42 admissible");
        assert_eq!(*r.as_inner(), 42);
    }

    #[test]
    fn try_new_rejects_inadmissible_input() {
        let result: Result<Refined<i32, NonNeg>, _> = Refined::try_new(-1);
        assert_eq!(result.unwrap_err(), Negative);
    }

    #[test]
    fn into_inner_returns_inner_value() {
        let r: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
        assert_eq!(r.into_inner(), 7);
    }

    #[test]
    fn pass_through_clone_and_eq() {
        // Use String here so Clone is meaningful (i32 is Copy and
        // clippy would otherwise suggest dropping the clone).
        let a: Refined<alloc::string::String, AnyString> =
            Refined::try_new(alloc::string::String::from("hi")).unwrap();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn pass_through_ord() {
        let a: Refined<i32, Always> = Refined::try_new(1).unwrap();
        let b: Refined<i32, Always> = Refined::try_new(2).unwrap();
        assert!(a < b);
    }

    #[test]
    fn pass_through_ord_cmp() {
        use core::cmp::Ordering;
        // `<` goes through PartialOrd; Ord::cmp needs its own
        // exercise so the impl on Refined is reached.
        let a: Refined<i32, Always> = Refined::try_new(1).unwrap();
        let b: Refined<i32, Always> = Refined::try_new(2).unwrap();
        assert_eq!(a.cmp(&b), Ordering::Less);
        assert_eq!(b.cmp(&a), Ordering::Greater);
        assert_eq!(a.cmp(&a), Ordering::Equal);
    }

    #[test]
    fn pass_through_hash() {
        use core::hash::{Hash, Hasher};

        /// Trivial no_std-friendly hasher: a running sum of the
        /// bytes the hashed value's `Hash` impl writes. Exists only
        /// to exercise `Refined`'s `Hash` impl in tests.
        struct CountingHasher(u64);
        impl Hasher for CountingHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for byte in bytes {
                    self.0 = self.0.wrapping_add(u64::from(*byte));
                }
            }
        }

        let r: Refined<i32, Always> = Refined::try_new(7).unwrap();
        let mut h1 = CountingHasher(0);
        let mut h2 = CountingHasher(0);
        r.hash(&mut h1);
        // Same value hashes to the same byte sequence.
        r.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
        // And the impl actually wrote something.
        assert_ne!(h1.finish(), 0);
    }

    #[test]
    fn pass_through_debug_prints_inner() {
        let a: Refined<i32, Always> = Refined::try_new(99).unwrap();
        let formatted = format!("{a:?}");
        assert_eq!(formatted, "99");
    }

    #[test]
    fn layout_matches_inner() {
        use core::mem::size_of;
        assert_eq!(size_of::<Refined<i32, NonNeg>>(), size_of::<i32>());
        assert_eq!(
            size_of::<Option<Refined<i32, NonNeg>>>(),
            size_of::<Option<i32>>(),
        );
    }

    proptest::proptest! {
        #[test]
        fn try_new_admissibility_round_trip(x in 0_i32..=i32::MAX) {
            let r: Refined<i32, NonNeg> = Refined::try_new(x).expect("admissible");
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

        #[test]
        fn try_new_rejects_all_negative(x in i32::MIN..0_i32) {
            let result: Result<Refined<i32, NonNeg>, _> = Refined::try_new(x);
            proptest::prop_assert_eq!(result.unwrap_err(), Negative);
        }

        #[test]
        fn into_inner_round_trip(x in 0_i32..=i32::MAX) {
            let r: Refined<i32, NonNeg> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(r.into_inner(), x);
        }

        /// Self-hosted Arbitrary: every value generated by the
        /// `Refined<i32, NonNeg>` strategy satisfies `NonNeg`.
        /// Replaces the prop_assume!-style filtering downstream
        /// crates would otherwise need.
        #[test]
        fn arbitrary_refined_always_admissible(
            r in proptest::arbitrary::any::<Refined<i32, NonNeg>>()
        ) {
            proptest::prop_assert!(*r.as_inner() >= 0);
        }
    }

    // ─── Serde: round-trip via serde_test (struct-only, no
    //      external JSON dependency). Verifies Deserialize
    //      routes through try_new and rejects inadmissible
    //      payloads with the rule's error.

    /// Custom Serializer / Deserializer combo using
    /// `serde::de::value::I32Deserializer` from serde itself
    /// (no `serde_test` / `serde_json` workspace dep needed).
    #[test]
    fn serde_serialize_forwards_to_inner() {
        // `serde_test::Token::I32(42)` is the wire shape an
        // i32 takes; if Refined's Serialize impl forwards to
        // the inner value, the same token comes out.
        let r: Refined<i32, NonNeg> = Refined::try_new(42).unwrap();
        serde_test::assert_ser_tokens(&r, &[serde_test::Token::I32(42)]);
    }

    #[test]
    fn serde_deserialize_admits_admissible() {
        // Deserializing `42` into Refined<i32, NonNeg> runs the
        // rule and accepts because 42 >= 0.
        let r: Refined<i32, NonNeg> = Refined::try_new(42).unwrap();
        serde_test::assert_de_tokens(&r, &[serde_test::Token::I32(42)]);
    }

    #[test]
    fn serde_deserialize_rejects_inadmissible_through_rule() {
        // Deserializing `-1` runs through `try_new`, which
        // rejects via the rule's `Display`. serde_test verifies
        // that the resulting error message embeds the rule's
        // own Display (`"negative"` for this test rule).
        serde_test::assert_de_tokens_error::<Refined<i32, NonNeg>>(
            &[serde_test::Token::I32(-1)],
            "negative",
        );
    }

    #[test]
    fn serde_deserialize_propagates_inner_decoder_failure() {
        // Feed a `String` token into a `Refined<i32, _>` deserializer.
        // `i32::deserialize` fails first, so the `?` short-circuit
        // in `Refined::deserialize` propagates the underlying serde
        // error — covering the early-return branch.
        serde_test::assert_de_tokens_error::<Refined<i32, NonNeg>>(
            &[serde_test::Token::Str("not an int")],
            "invalid type: string \"not an int\", expected i32",
        );
    }
}
