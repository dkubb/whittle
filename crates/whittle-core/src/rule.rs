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
    #[inline]
    pub fn try_new(raw: T) -> Result<Self, R::Error> {
        match R::refine(raw) {
            Ok(inner) => Ok(Self { inner, rule: PhantomData }),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used,
        reason = "explicit in test code")]
mod tests {
    use super::{Refined, Rule};
    use alloc::format;

    /// Test rule: accept only non-negative i32.
    enum NonNeg {}

    #[derive(Debug, PartialEq, Eq)]
    struct Negative;

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
        fn refine(raw: alloc::string::String)
            -> Result<alloc::string::String, Self::Error>
        { Ok(raw) }
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
            proptest::prop_assert!(result.is_err());
        }

        #[test]
        fn into_inner_round_trip(x in 0_i32..=i32::MAX) {
            let r: Refined<i32, NonNeg> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(r.into_inner(), x);
        }
    }
}
