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
/// required by the schema reflection surface
/// ([`SchemaRule`](crate::SchemaRule)'s bounds).
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

    /// Return whether `raw` can be narrowed by this rule.
    ///
    /// This is the by-value boolean projection of [`Self::refine`].
    /// It consumes `raw` for the same reason `refine` does: a rule
    /// may canonicalise its input before accepting it. Callers that
    /// need to keep the original value must make that choice
    /// explicitly with `Copy` or `Clone`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Rule;
    /// use whittle_core::primitive::Within;
    ///
    /// assert!(Within::<0, 100>::accepts(42_i32));
    /// assert!(!Within::<0, 100>::accepts(101_i32));
    /// ```
    #[inline]
    fn accepts(raw: T) -> bool {
        Self::refine(raw).is_ok()
    }
}

/// Marker: rules whose `refine` is the IDENTITY on admissible input.
///
/// A pure predicate returns `Ok(raw)` unchanged, never a
/// canonicalisation (IDEA §5.1; the planned artifact named in
/// ARCHITECTURE §15.5, landed with its first consumer: composition
/// schemas).
///
/// For a pure filter the accepted set and the carried set
/// (`range(refine)`) coincide, which is what lets derived
/// integrations compose: the combinator
/// [`SchemaRule`](crate::SchemaRule) impls
/// (`And`/`All` as `Intersection`, `Or`/`Any` as `Union`) are sound
/// only over pure operands — a canonicalising operand can move a
/// value out of (or fail to produce a value inside) the other
/// operand's set, so the set algebra stops describing
/// `range(refine)`. `And<LenChars<3, 3>, Trim<NonEmpty>>` is the
/// counterexample: `"a  "` is in both operands' sets, but the
/// composition carries `"a"`, which is in neither intersection
/// claim. Bounding the combinator schemas on this marker makes the
/// unsound composition ABSENT rather than wrong.
///
/// # Implementor obligation
///
/// Implement only when, for EVERY input `x`, `Self::refine(x)` is
/// either `Err(_)` or `Ok(x)` with `x` returned bit-for-bit
/// unchanged. The obligation must hold for every admissible input,
/// not only for the shapes a particular caller produces. Follows
/// the same capability-marker pattern as
/// [`StableUnderTrim`](crate::transform::StableUnderTrim) (see its
/// four-step audit recipe). Transformers
/// ([`Trim`](crate::transform::Trim) and the case foldings) are
/// deliberately NOT pure filters: rewriting input is their purpose.
///
/// For composed rules the marker propagates: `And` / `Or` / `All` /
/// `Any` / `MapErr` are pure iff their operands are (the kernel
/// provides those impls); `Not` / `Xor` are pure unconditionally
/// (their accept path returns the numeric input's own widened
/// round-trip).
///
/// # Examples
///
/// The marker propagates through compositions of pure rules:
///
/// ```
/// use whittle_core::{And, PureFilter};
/// use whittle_core::primitive::{AtLeast, AtMost};
///
/// fn assert_pure<R: PureFilter>() {}
/// assert_pure::<AtLeast<0>>();
/// assert_pure::<And<AtLeast<0>, AtMost<100>>>();
/// ```
///
/// Transformers are deliberately absent — and the absence
/// propagates, so a composition containing one is not pure either:
///
/// ```compile_fail
/// use whittle_core::PureFilter;
/// use whittle_core::primitive::NonEmpty;
/// use whittle_core::transform::Trim;
///
/// fn assert_pure<R: PureFilter>() {}
/// // error[E0277]: Trim<NonEmpty> does not implement PureFilter
/// assert_pure::<Trim<NonEmpty>>();
/// ```
pub trait PureFilter {}

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
    /// Internal constructor for const-capable primitive rules that
    /// have already checked their invariant.
    #[inline]
    pub(crate) const fn from_inner(inner: T) -> Self {
        Self {
            inner,
            rule: PhantomData,
        }
    }

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

    /// Crate-internal mutable borrow of the inner value.
    ///
    /// NOT public: IDEA.md §5.2 forbids public accessors that return
    /// a mutable reference, because an unconstrained `&mut T` would
    /// let callers invalidate the construction-time proof. This
    /// crate-internal escape hatch exists solely for checked
    /// mutation methods (e.g. `try_push` on `LenItems`-ruled
    /// vectors) that verify the rule's invariant *before* committing
    /// the mutation.
    ///
    /// # Soundness obligation
    ///
    /// Every caller MUST guarantee the rule's invariant holds again
    /// by the time the borrow ends, and MUST document the argument
    /// at the call site (the same per-site discipline as
    /// `from_inner`).
    #[inline]
    pub(crate) const fn as_inner_mut(&mut self) -> &mut T {
        &mut self.inner
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

    /// Map the inner value and re-establish a (possibly different)
    /// rule for the result.
    ///
    /// This is sugar over `into_inner` → transform → `try_new`: the
    /// existing proof is erased, `f` rebuilds the carrier, and the
    /// target rule `S` re-runs its narrowing morphism on the result.
    /// Because every output routes through `try_new`, the operation
    /// carries no soundness debt — there is no path that produces a
    /// `Refined<U, S>` without `S::refine` accepting the mapped
    /// value. For length-only target rules the re-validation is a
    /// length compare; for per-element rules it is O(n).
    ///
    /// For length-preserving element-wise maps under a length-only
    /// rule, the infallible `map_items` (on
    /// `Refined<Vec<T>, R>` where `R: StableUnderElementMap`)
    /// avoids the re-validation entirely.
    ///
    /// # Errors
    ///
    /// Forwards the target rule's `S::Error` when `S::refine`
    /// rejects the mapped value.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::{CollectionError, LenItems};
    ///
    /// let files: Refined<Vec<i32>, LenItems<1, 3>> =
    ///     Refined::try_new(vec![10, 20]).unwrap();
    ///
    /// // Admit: the mapped vector still has 2 items, which the
    /// // target rule (here the same `LenItems<1, 3>`) accepts.
    /// let labels: Refined<Vec<String>, LenItems<1, 3>> = files
    ///     .clone()
    ///     .try_map(|v| v.into_iter().map(|n| n.to_string()).collect())
    ///     .unwrap();
    /// assert_eq!(labels.as_inner(), &["10".to_string(), "20".to_string()]);
    ///
    /// // Reject: the map's output (2 items) violates the *target*
    /// // rule `LenItems<5, 9>` — the typed error surfaces.
    /// let err = files
    ///     .try_map::<Vec<i32>, LenItems<5, 9>, _>(|v| v)
    ///     .unwrap_err();
    /// assert_eq!(err, CollectionError::LenOutOfRange { actual: 2 });
    /// ```
    #[inline]
    pub fn try_map<U, S, F>(self, f: F) -> Result<Refined<U, S>, S::Error>
    where
        U: 'static,
        S: Rule<U>,
        F: FnOnce(T) -> U,
    {
        Refined::try_new(f(self.inner))
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

impl<T: fmt::Display, R> fmt::Display for Refined<T, R> {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// Per-rule deserialization hook for `Refined<T, R>`.
///
/// `Refined<T, R>`'s `serde::Deserialize` impl delegates to this
/// trait, so each rule chooses *how* the wire value is consumed:
///
/// - Most rules use the default **parse-then-refine** path
///   ([`parse_then_refine`]): deserialize the raw `T`, then run the
///   rule through `Refined::try_new`. The [`crate::deserialize_rule!`]
///   macro stamps that impl as a one-liner.
/// - Rules whose admissibility bounds the *size* of the input may
///   override the hook to enforce the bound **while** the wire value
///   is being decoded, so a hostile payload is rejected without ever
///   materializing more than the rule admits. `LenItems<MIN, MAX>`
///   over `Vec<T>` is the library's streaming override (IDEA §5.13:
///   bounded inputs; §7: the constructor surface must be robust
///   against resource-exhausting payloads).
///
/// Whatever the strategy, the accept/reject set and the rejection
/// diagnostics MUST be identical to the parse-then-refine path —
/// only the allocation profile may differ. There is still no
/// admissible code path that produces a `Refined` without the rule's
/// admissibility predicate holding (IDEA §5.3).
///
/// # Custom rules
///
/// Downstream rules keep deserializing by stamping the default path:
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::{Refined, Rule, deserialize_rule};
///
/// /// Accepts only even `i32`.
/// enum Even {}
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct Odd;
///
/// impl core::fmt::Display for Odd {
///     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
///         f.write_str("odd")
///     }
/// }
///
/// impl Rule<i32> for Even {
///     type Error = Odd;
///     fn refine(raw: i32) -> Result<i32, Self::Error> {
///         if raw % 2 == 0 { Ok(raw) } else { Err(Odd) }
///     }
/// }
///
/// deserialize_rule! {
///     impl[] DeserializeRule<i32> for Even
/// }
///
/// let ok: Refined<i32, Even> = serde_json::from_str("4").unwrap();
/// assert_eq!(*ok.as_inner(), 4);
/// let err = serde_json::from_str::<Refined<i32, Even>>("5").unwrap_err();
/// assert!(err.to_string().contains("odd"));
/// # }
/// ```
#[cfg(feature = "serde")]
pub trait DeserializeRule<'de, T>: Rule<T>
where
    T: 'static,
{
    /// Deserialize a `Refined<T, Self>` from `deserializer`.
    ///
    /// # Errors
    ///
    /// Returns `D::Error` when the wire value cannot be decoded, or
    /// when the decoded value is inadmissible under the rule (the
    /// rule's typed error rendered through
    /// `serde::de::Error::custom`).
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "serde")] {
    /// use whittle_core::{DeserializeRule, Refined};
    /// use whittle_core::primitive::Within;
    ///
    /// let mut deserializer = serde_json::Deserializer::from_str("42");
    /// let value: Refined<i32, Within<0, 100>> =
    ///     <Within<0, 100> as DeserializeRule<'_, i32>>::deserialize_refined(
    ///         &mut deserializer,
    ///     )
    ///     .unwrap();
    ///
    /// assert_eq!(*value.as_inner(), 42);
    /// # }
    /// ```
    fn deserialize_refined<D>(deserializer: D) -> Result<Refined<T, Self>, D::Error>
    where
        D: serde::Deserializer<'de>;
}

/// Default [`DeserializeRule`] body: deserialize the raw `T`, then
/// run the rule via `Refined::try_new`.
///
/// Rejections surface as `serde::de::Error::custom(rule_error)`, so
/// the wire-level diagnostic is the rule error's `Display` output.
/// Use [`crate::deserialize_rule!`] to stamp a rule's
/// `DeserializeRule` impl with this body.
///
/// # Errors
///
/// Returns `D::Error` when `T::deserialize` fails, or the rule's
/// error (via `serde::de::Error::custom`) when `R::refine` rejects
/// the decoded value.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::primitive::Within;
/// use whittle_core::{Refined, parse_then_refine};
///
/// let mut ok = serde_json::Deserializer::from_str("42");
/// let r: Refined<i32, Within<0, 100>> = parse_then_refine(&mut ok).unwrap();
/// assert_eq!(*r.as_inner(), 42);
///
/// let mut bad = serde_json::Deserializer::from_str("101");
/// let err = parse_then_refine::<i32, Within<0, 100>, _>(&mut bad).unwrap_err();
/// assert!(err.to_string().contains("value 101 not in admissible range"));
/// # }
/// ```
#[cfg(feature = "serde")]
pub fn parse_then_refine<'de, T, R, D>(deserializer: D) -> Result<Refined<T, R>, D::Error>
where
    T: serde::Deserialize<'de> + 'static,
    R: Rule<T>,
    R::Error: fmt::Display,
    D: serde::Deserializer<'de>,
{
    let raw = T::deserialize(deserializer)?;
    Refined::try_new(raw).map_err(serde::de::Error::custom)
}

/// Deserialize a `Refined<T, R>` through the rule's
/// [`DeserializeRule`] hook (parse-then-refine for most rules;
/// streaming bound enforcement for length-bounded collections).
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
    T: 'static,
    R: DeserializeRule<'de, T>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        R::deserialize_refined(deserializer)
    }
}

// ─── Proptest `ArbitraryRule` + `Arbitrary`. Each rule supplies a
//      strategy that emits admissible-by-construction values; the
//      blanket `Arbitrary for Refined<T, R>` impl maps that strategy
//      through `Refined::try_new` to produce the carrier. The
//      blanket impl itself does no rejection sampling — it maps the
//      rule's strategy through `try_new` and panics on contract
//      violation. Composition strategies (e.g., `And<A, B>`) may
//      filter their operands' strategies; see the relevant impl
//      docs. For the primitive rules a sparse admissible region
//      (`Within<0, 100>` over `i32`) is as cheap to generate as a
//      dense one (`NonZero`).
//
//      `ArbitraryRule` carries the soundness obligation: a rule's
//      strategy must yield only values that `R::refine` accepts. The
//      blanket impl's `expect` documents the contract; a violation
//      surfaces as a panic at test time, not silent data corruption.

/// A `Rule<T>` that knows how to generate admissible-by-construction
/// `T` values for `proptest`.
///
/// `Refined<T, R>`'s `Arbitrary` impl drives `arbitrary_strategy`
/// directly. Implementers must ensure every value the strategy
/// emits is accepted by `R::refine`; the blanket `Arbitrary` impl
/// panics on contract violation, so a strategy bug surfaces as a
/// panic in property tests rather than as silently dropped samples.
/// (Composition rules like `And<A, B>` may filter their operands'
/// strategies; primitive rules must be constructive.)
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait ArbitraryRule<T>: Rule<T>
where
    T: 'static,
{
    /// Strategy type that yields values admissible under this rule.
    type Strategy: proptest::strategy::Strategy<Value = T>;

    /// Construct the rule's admissible-by-construction strategy.
    ///
    /// # Contract
    ///
    /// Every value produced by the returned strategy MUST satisfy
    /// `R::refine`. The `Arbitrary` blanket impl `expect`s on the
    /// `try_new` step; a violation surfaces as a panic at test
    /// time.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "proptest")] {
    /// use proptest::strategy::{Strategy as _, ValueTree as _};
    /// use proptest::test_runner::TestRunner;
    /// use whittle_core::{ArbitraryRule, Rule};
    /// use whittle_core::primitive::Within;
    ///
    /// let strategy = <Within<0, 10> as ArbitraryRule<i32>>::arbitrary_strategy();
    /// let mut runner = TestRunner::deterministic();
    /// let value = strategy.new_tree(&mut runner).unwrap().current();
    ///
    /// assert!(Within::<0, 10>::accepts(value));
    /// # }
    /// ```
    fn arbitrary_strategy() -> Self::Strategy;

    /// Construct an admissible strategy with an explicit size
    /// profile.
    ///
    /// The default implementation preserves [`Self::arbitrary_strategy`].
    /// Rule families that can make size meaningful, such as string
    /// length rules, may override this to clamp generated sizes
    /// without changing the blanket [`Arbitrary`](proptest::arbitrary::Arbitrary)
    /// path for [`Refined<T, R>`].
    fn arbitrary_strategy_profiled(_profile: SizeProfile) -> proptest::strategy::BoxedStrategy<T>
    where
        Self::Strategy: 'static,
    {
        use proptest::strategy::Strategy as _;
        Self::arbitrary_strategy().boxed()
    }
}

/// Size profile for opt-in proptest strategies.
///
/// The default [`Arbitrary`](proptest::arbitrary::Arbitrary) impl for
/// [`Refined<T, R>`] always uses the rule's complete strategy. Use
/// [`profiled_refined`] when a property deliberately wants a smaller
/// admissible subset, for example to avoid sending multi-megabyte
/// bounded text through serialization-heavy tests.
///
/// Consumer-only generators belong in `#[cfg(test)]` or test-support
/// modules near the properties that use them. Production
/// [`ArbitraryRule`] impls are public testing surface and should stay
/// covered like other public behavior.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "proptest")] {
/// use proptest::strategy::{Strategy as _, ValueTree as _};
/// use proptest::test_runner::TestRunner;
/// use whittle_core::primitive::LenChars;
/// use whittle_core::{profiled_refined, Refined, SizeProfile};
///
/// type Body = Refined<String, LenChars<0, 10_000_000>>;
///
/// let strategy = profiled_refined::<String, LenChars<0, 10_000_000>>(
///     SizeProfile::small_valid(16),
/// );
/// let mut runner = TestRunner::deterministic();
/// let body: Body = strategy.new_tree(&mut runner).unwrap().current();
///
/// assert!(body.as_inner().chars().count() <= 16);
/// # }
/// ```
#[cfg(feature = "proptest")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeProfile {
    max_len: Option<usize>,
}

#[cfg(feature = "proptest")]
impl SizeProfile {
    /// Full rule-derived strategy.
    ///
    /// This preserves the default [`ArbitraryRule::arbitrary_strategy`]
    /// behavior, including the existing boundary-inclusive strategy
    /// for length-bounded rules.
    pub const FULL_BOUNDARY: Self = Self { max_len: None };

    /// Clamp generated valid sizes to `max_len` where a rule has a
    /// size dimension.
    ///
    /// The clamp never creates invalid samples. If a rule's minimum
    /// valid size is greater than `max_len`, the generated subset uses
    /// that minimum so every sample remains admissible.
    #[inline]
    #[must_use]
    pub const fn small_valid(max_len: usize) -> Self {
        Self {
            max_len: Some(max_len),
        }
    }

    #[inline]
    pub(crate) const fn clamp_inclusive_max(self, min: usize, max: usize) -> usize {
        match self.max_len {
            Some(limit) => {
                let limited = if max < limit { max } else { limit };
                if limited < min { min } else { limited }
            }
            None => max,
        }
    }
}

/// Build an opt-in profiled strategy for refined values.
///
/// This is separate from the blanket
/// [`Arbitrary`](proptest::arbitrary::Arbitrary) impl so profiles never
/// weaken the default complete generator. Every emitted value still
/// routes through [`Refined::try_new`], so a buggy [`ArbitraryRule`]
/// implementation fails loudly instead of leaking invalid samples.
///
/// # Panics
///
/// Panics if `R`'s profiled strategy emits a value rejected by
/// `R::refine`. That panic is the documented diagnostic surface for a
/// buggy [`ArbitraryRule`] implementation.
#[cfg(feature = "proptest")]
#[expect(
    clippy::panic,
    reason = "soundness-contract violation: panicking with the violating type name \
              is the documented diagnostic surface for a buggy ArbitraryRule strategy"
)]
pub fn profiled_refined<T, R>(
    profile: SizeProfile,
) -> proptest::strategy::BoxedStrategy<Refined<T, R>>
where
    T: core::fmt::Debug + 'static,
    R: ArbitraryRule<T> + 'static,
    R::Strategy: 'static,
{
    use proptest::strategy::Strategy as _;
    R::arbitrary_strategy_profiled(profile)
        .prop_map(|raw| {
            Refined::try_new(raw).unwrap_or_else(|_| {
                panic!(
                    "ArbitraryRule for {} must yield admissible values \
                     (got a value rejected by `Rule::refine`)",
                    core::any::type_name::<R>(),
                )
            })
        })
        .boxed()
}

#[cfg(feature = "proptest")]
impl<T, R> proptest::arbitrary::Arbitrary for Refined<T, R>
where
    T: core::fmt::Debug + 'static,
    R: ArbitraryRule<T> + 'static,
{
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    #[expect(
        clippy::panic,
        reason = "soundness-contract violation: panicking with the violating type name \
                  is the documented diagnostic surface for a buggy ArbitraryRule strategy"
    )]
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        R::arbitrary_strategy()
            .prop_map(|raw| {
                // Naming the violating `ArbitraryRule` impl in the
                // panic message turns a blanket-contract failure
                // into a localized diagnostic: when a strategy bug
                // yields a value that `R::refine` rejects, the
                // panic identifies which `R` is at fault rather
                // than only restating the contract.
                Self::try_new(raw).unwrap_or_else(|_| {
                    panic!(
                        "ArbitraryRule for {} must yield admissible values \
                         (got a value rejected by `Rule::refine`)",
                        core::any::type_name::<R>(),
                    )
                })
            })
            .boxed()
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

    // The downstream-facing one-liner: a custom rule keeps
    // deserializing by stamping the default parse-then-refine
    // `DeserializeRule` impl.
    #[cfg(feature = "serde")]
    crate::deserialize_rule! {
        impl[] DeserializeRule<i32> for NonNeg
    }

    #[cfg(feature = "proptest")]
    impl super::ArbitraryRule<i32> for NonNeg {
        type Strategy = proptest::strategy::BoxedStrategy<i32>;
        fn arbitrary_strategy() -> Self::Strategy {
            use proptest::strategy::Strategy as _;
            (0_i32..=i32::MAX).boxed()
        }
    }

    /// Test rule whose `Rule::refine` always rejects and whose
    /// `ArbitraryRule` strategy still emits a value. Used by
    /// `arbitrary_panics_on_strategy_bug` to exercise the blanket
    /// `Arbitrary` impl's soundness-violation panic — the only
    /// branch of `arbitrary_with` that documents the `Rule::refine`
    /// rejection path. The unconditional rejection keeps the rule's
    /// branch count minimal so the helper does not itself introduce
    /// uncovered regions.
    #[cfg(feature = "proptest")]
    enum AlwaysRejects {}
    #[cfg(feature = "proptest")]
    impl Rule<i32> for AlwaysRejects {
        type Error = Negative;
        fn refine(_raw: i32) -> Result<i32, Self::Error> {
            Err(Negative)
        }
    }
    #[cfg(feature = "proptest")]
    impl super::ArbitraryRule<i32> for AlwaysRejects {
        type Strategy = proptest::strategy::BoxedStrategy<i32>;
        fn arbitrary_strategy() -> Self::Strategy {
            use proptest::strategy::Strategy as _;
            // Deliberate contract violation: emit a value that
            // `AlwaysRejects::refine` will reject. The blanket impl
            // must panic with the violating type name.
            proptest::strategy::Just(0_i32).boxed()
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
    fn accepts_matches_refine_for_admissible_input() {
        let raw = 42;

        assert_eq!(NonNeg::accepts(raw), NonNeg::refine(raw).is_ok());
    }

    #[test]
    fn accepts_matches_refine_for_rejected_input() {
        let raw = -1;

        assert_eq!(NonNeg::accepts(raw), NonNeg::refine(raw).is_ok());
    }

    #[test]
    fn into_inner_returns_inner_value() {
        let r: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
        assert_eq!(r.into_inner(), 7);
    }

    #[test]
    fn try_map_admits_when_target_rule_accepts() {
        let r: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
        let mapped: Refined<i32, NonNeg> = r.try_map(|x| x + 1).unwrap();
        assert_eq!(*mapped.as_inner(), 8);
    }

    #[test]
    fn try_map_rejects_when_target_rule_rejects() {
        let r: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
        let err = r.try_map::<i32, NonNeg, _>(|x| -x).unwrap_err();
        assert_eq!(err, Negative);
    }

    #[test]
    fn try_map_crosses_rule_pairs() {
        // The target rule is independent of the source rule: a value
        // inadmissible under `NonNeg` is admissible under `Always`.
        let r: Refined<i32, NonNeg> = Refined::try_new(7).unwrap();
        let mapped: Refined<i32, Always> = r.try_map(|x| -x).unwrap();
        assert_eq!(*mapped.as_inner(), -7);
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

        /// Trivial no_std-friendly hasher: a wrapping sum of the
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
    fn pass_through_display_prints_inner() {
        let a: Refined<i32, Always> = Refined::try_new(99).unwrap();
        let formatted = format!("{a}");
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

        /// `try_map` happy path: the identity map keeps the value
        /// inside `NonNeg`'s admissible region, so re-validation
        /// through `try_new` accepts every sample.
        #[test]
        fn try_map_identity_round_trip(x in 0_i32..=i32::MAX) {
            let r: Refined<i32, NonNeg> = Refined::try_new(x).unwrap();
            let mapped: Refined<i32, NonNeg> = r.try_map(|v| v).unwrap();
            proptest::prop_assert_eq!(*mapped.as_inner(), x);
        }

        /// `try_map` rejection path: the valid grammar feeds the
        /// source; the map fabricates an output outside `NonNeg`'s
        /// admissible region, so the target rule's typed error
        /// surfaces for every sample.
        #[test]
        fn try_map_rejects_inadmissible_map_output(x in 0_i32..=i32::MAX) {
            let r: Refined<i32, NonNeg> = Refined::try_new(x).unwrap();
            let result: Result<Refined<i32, NonNeg>, _> = r.try_map(|_| -1);
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
        #[cfg(feature = "proptest")]
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
    #[cfg(feature = "proptest")]
    #[test]
    #[should_panic(expected = "ArbitraryRule for")]
    fn arbitrary_panics_on_strategy_bug() {
        // A buggy `ArbitraryRule` whose strategy emits values
        // `Rule::refine` rejects must surface as a panic naming
        // the violating impl, not as silently dropped samples.
        // Drive one sample through the blanket `Arbitrary` impl.
        use proptest::strategy::{Strategy as _, ValueTree as _};
        use proptest::test_runner::TestRunner;
        let strategy =
            <Refined<i32, AlwaysRejects> as proptest::arbitrary::Arbitrary>::arbitrary_with(());
        let mut runner = TestRunner::default();
        // The `current()` call runs the `prop_map`, which panics.
        let _value: Refined<i32, AlwaysRejects> = strategy.new_tree(&mut runner).unwrap().current();
    }

    #[cfg(feature = "proptest")]
    #[test]
    #[should_panic(expected = "ArbitraryRule for")]
    fn profiled_refined_panics_on_strategy_bug() {
        // The opt-in profiled path has the same soundness boundary
        // as the blanket `Arbitrary` impl.
        use proptest::strategy::{Strategy as _, ValueTree as _};
        use proptest::test_runner::TestRunner;
        let strategy =
            super::profiled_refined::<i32, AlwaysRejects>(super::SizeProfile::small_valid(1));
        let mut runner = TestRunner::default();
        let _value: Refined<i32, AlwaysRejects> = strategy.new_tree(&mut runner).unwrap().current();
    }

    #[cfg(feature = "serde")]
    mod serde_round_trip {
        use super::{NonNeg, Refined};

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
}
