//! Closed-set parsing: a wire string admitted against a declared
//! string ↔ variant table and parsed into an enum.
//!
//! A closed set is an enumerated domain whose admitted wire values
//! are a small fixed set of nominal tokens (`"active"`/`"inactive"`,
//! ISO currency codes, provider branch codes). The target artifact
//! is a plain Rust enum — already **constructive**: the enum's
//! representable states are exactly the admissible states, so no
//! `Refined` carrier is involved. What this module adds is the
//! vocabulary that declares the admitted set **once** — the
//! [`ClosedSet::MEMBERS`] table — and derives everything else from
//! that single determinant:
//!
//! - [`parse`] — the boundary morphism `&str → Result<E, _>`;
//! - [`as_str`] — the lossless inverse onto the wire form;
//! - [`ClosedSetError`] — a typed rejection carrying the (bounded)
//!   offending value and a `'static` borrow of the expected table;
//! - behind the `serde` feature, [`serialize`] / [`deserialize`] —
//!   the plain-wire-string codec (`Serialize` = [`as_str`],
//!   `Deserialize` = [`parse`]), emitted automatically for
//!   macro-generated enums and usable via `#[serde(with)]` for
//!   hand-written impls;
//! - behind the `proptest` feature, [`admissible`] / [`rejects`] —
//!   a select-from-`MEMBERS` strategy (admissible by construction,
//!   support exactly the closed set) and a derived reject-input
//!   generator (case-flips, truncations, extensions, the empty
//!   string, filtered arbitrary strings), so boundary tests need no
//!   hand-maintained reject list.
//!
//! Prefer the [`closed_set!`](macro@crate::closed_set) macro over a
//! hand-written impl: the macro generates the enum and the table
//! from one declaration list, which makes "variant without a wire
//! string", "wire string without a variant", and "variant declared
//! twice" unrepresentable in the declaration artifact itself.
//!
//! # Hand-written impl obligations
//!
//! A hand-written [`ClosedSet`] impl carries two obligations the
//! macro discharges structurally:
//!
//! 1. **Wire-string injectivity** — no two table entries share a
//!    wire string. This is compile-time checked by
//!    [`ClosedSet::VALID`], which [`parse`] and [`as_str`] force at
//!    monomorphisation.
//! 2. **Variant coverage** — every variant of the enum appears in
//!    `MEMBERS` exactly once. The type system cannot see "every
//!    variant" generically, so this is a documented contract;
//!    [`as_str`] panics loudly when it is violated (see its docs
//!    for why that is a contract-violation diagnostic, not a
//!    fallback).
//!
//! [`verify_table`] discharges these obligations mechanically to
//! the extent expressible: hand-written impls call it once in a
//! test; macro-generated impls don't need it.

use alloc::string::String;

/// Maximum number of characters of the offending value retained by
/// [`ClosedSetError`]. Closed-set wire values are short nominal
/// tokens; anything longer is noise (or an attack) and is truncated
/// at construction so error payloads stay bounded.
const MAX_VALUE_CHARS: usize = 64;

/// Maximum number of expected members rendered by
/// [`ClosedSetError`]'s `Display` (and the serde `expecting` text).
/// Larger sets (ISO currency) render the first
/// `MAX_RENDERED_MEMBERS` followed by `… (N total)` so message size
/// stays bounded.
const MAX_RENDERED_MEMBERS: usize = 8;

/// An enum whose variants are in bijection with a fixed set of wire
/// strings, declared once as the [`MEMBERS`](Self::MEMBERS) table.
///
/// The table is THE single determinant: [`parse`], [`as_str`], and
/// the error's expected-set payload are all derived from it.
///
/// The table must be injective in both directions — no duplicate
/// wire strings (checked at compile time by [`VALID`](Self::VALID))
/// and no duplicate variants (structural under
/// [`closed_set!`](macro@crate::closed_set); a documented
/// obligation for hand-written impls). Aliases (many wire strings
/// mapping to one variant) are deliberately not supported.
///
/// # Examples
///
/// A hand-written impl (prefer the macro, which generates all of
/// this from one declaration):
///
/// ```
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Account activity status.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum ActivityStatus {
///     Active,
///     Inactive,
/// }
///
/// impl ClosedSet for ActivityStatus {
///     const MEMBERS: &'static [(&'static str, Self)] = &[
///         ("active", Self::Active),
///         ("inactive", Self::Inactive),
///     ];
/// }
///
/// let status: ActivityStatus = closed_set::parse("active").unwrap();
/// assert_eq!(status, ActivityStatus::Active);
/// assert_eq!(closed_set::as_str(status), "active");
/// ```
pub trait ClosedSet: Copy + PartialEq + Sized + 'static {
    /// The single determinant: the injective wire-string ↔ variant
    /// table. Order is the declaration order and is what
    /// [`ClosedSetError`]'s `Display` renders.
    const MEMBERS: &'static [(&'static str, Self)];

    /// Compile-time witness that the table is well-formed:
    /// non-empty and with no duplicate wire strings.
    ///
    /// [`parse`] and [`as_str`] evaluate this const at
    /// monomorphisation (the same house pattern as `Within`'s
    /// `MIN <= MAX` gate and [`Implies::VALID`](crate::Implies)),
    /// so an impl whose table declares the same wire string twice
    /// is a compile error at first use. The
    /// [`closed_set!`](macro@crate::closed_set) macro additionally
    /// forces it at declaration time.
    ///
    /// A table mapping one wire string to two variants is rejected
    /// here; a table mapping one *variant* to two wire strings (an
    /// alias) cannot be detected generically at compile time
    /// (`PartialEq` is not const-callable) and is instead made
    /// unrepresentable by the macro's single-declaration shape —
    /// one table row per declared variant.
    ///
    /// ```compile_fail
    /// use whittle_core::ClosedSet;
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// enum Dup {
    ///     A,
    ///     B,
    /// }
    ///
    /// impl ClosedSet for Dup {
    ///     const MEMBERS: &'static [(&'static str, Self)] =
    ///         &[("same", Self::A), ("same", Self::B)];
    /// }
    ///
    /// // error[E0080]: duplicate wire string in ClosedSet::MEMBERS
    /// const _: () = <Dup as ClosedSet>::VALID;
    /// ```
    const VALID: () = {
        assert!(
            !Self::MEMBERS.is_empty(),
            "ClosedSet::MEMBERS must be non-empty: an empty closed set admits \
             nothing and cannot witness any enum variant",
        );
        assert!(
            !has_duplicate_wire_string(Self::MEMBERS),
            "duplicate wire string in ClosedSet::MEMBERS: the table must map \
             each wire string to exactly one variant",
        );
    };
}

/// Const-evaluable byte-wise string equality (`==` on `str` is not
/// const-callable).
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Const-evaluable pairwise duplicate scan over the table's wire
/// strings. `O(n^2)` at compile time; closed sets are small by
/// definition.
const fn has_duplicate_wire_string<E>(members: &[(&str, E)]) -> bool {
    let mut i = 0;
    while i < members.len() {
        let mut j = i + 1;
        while j < members.len() {
            if str_eq(members[i].0, members[j].0) {
                return true;
            }
            j += 1;
        }
        i += 1;
    }
    false
}

/// Rejection produced by [`parse`] when the input is not a member
/// of `E`'s closed set.
///
/// Carries the offending value truncated to a fixed bound
/// (64 characters — closed-set wire values are short nominal
/// tokens) and the expected set as a `'static` borrow of
/// [`ClosedSet::MEMBERS`] — one determinant, not a copy. `Display`
/// caps the rendered list at the first 8 members plus an
/// `… (N total)` suffix so message size stays bounded for large
/// sets, and renders the offending value through
/// [`str::escape_debug`] so the message stays printable.
///
/// # Examples
///
/// ```
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Account activity status.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum ActivityStatus {
///     Active,
///     Inactive,
/// }
///
/// impl ClosedSet for ActivityStatus {
///     const MEMBERS: &'static [(&'static str, Self)] = &[
///         ("active", Self::Active),
///         ("inactive", Self::Inactive),
///     ];
/// }
///
/// let err = closed_set::parse::<ActivityStatus>("actve").unwrap_err();
/// assert_eq!(err.value(), "actve");
/// assert_eq!(err.expected(), ActivityStatus::MEMBERS);
/// assert_eq!(
///     err.to_string(),
///     r#"invalid value "actve": expected one of "active", "inactive""#,
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedSetError<E: 'static> {
    /// The offending input, truncated to [`MAX_VALUE_CHARS`]
    /// characters at construction. Private so the bound is an
    /// invariant, not a convention.
    value: String,
    /// `'static` borrow of the rejecting set's
    /// [`ClosedSet::MEMBERS`] table.
    expected: &'static [(&'static str, E)],
}

impl<E: ClosedSet> ClosedSetError<E> {
    /// Build the rejection for `raw` against `E`'s table,
    /// truncating the retained value to [`MAX_VALUE_CHARS`].
    fn new(raw: &str) -> Self {
        Self {
            value: raw.chars().take(MAX_VALUE_CHARS).collect(),
            expected: E::MEMBERS,
        }
    }
}

impl<E> ClosedSetError<E> {
    /// The offending input, truncated to a 64-character bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::ClosedSet;
    /// use whittle_core::closed_set;
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// enum Toggle {
    ///     On,
    ///     Off,
    /// }
    ///
    /// impl ClosedSet for Toggle {
    ///     const MEMBERS: &'static [(&'static str, Self)] =
    ///         &[("on", Self::On), ("off", Self::Off)];
    /// }
    ///
    /// let error = closed_set::parse::<Toggle>("missing").unwrap_err();
    ///
    /// assert_eq!(error.value(), "missing");
    /// ```
    #[inline]
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The expected set: a `'static` borrow of the rejecting
    /// [`ClosedSet::MEMBERS`] table.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::ClosedSet;
    /// use whittle_core::closed_set;
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// enum Toggle {
    ///     On,
    ///     Off,
    /// }
    ///
    /// impl ClosedSet for Toggle {
    ///     const MEMBERS: &'static [(&'static str, Self)] =
    ///         &[("on", Self::On), ("off", Self::Off)];
    /// }
    ///
    /// let error = closed_set::parse::<Toggle>("missing").unwrap_err();
    ///
    /// assert_eq!(error.expected(), Toggle::MEMBERS);
    /// ```
    #[inline]
    #[must_use]
    pub const fn expected(&self) -> &'static [(&'static str, E)] {
        self.expected
    }
}

/// Render the capped expected-member list: the first
/// [`MAX_RENDERED_MEMBERS`] wire strings, then `… (N total)` when
/// the set is larger. A single rendering helper so every consumer
/// of the expected list shares one determinant. Deliberately
/// non-generic (`dyn Iterator` over the wire strings): the cap
/// condition is constant per table, so a per-`E` monomorphisation
/// could never exercise both sides of it.
fn write_expected_list(
    f: &mut core::fmt::Formatter<'_>,
    total: usize,
    names: &mut dyn Iterator<Item = &'static str>,
) -> core::fmt::Result {
    for (i, name) in names.take(MAX_RENDERED_MEMBERS).enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "\"{name}\"")?;
    }
    if total > MAX_RENDERED_MEMBERS {
        write!(f, ", … ({total} total)")?;
    }
    Ok(())
}

impl<E> core::fmt::Display for ClosedSetError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "invalid value \"{}\": expected one of ",
            self.value.escape_debug(),
        )?;
        write_expected_list(
            f,
            self.expected.len(),
            &mut self.expected.iter().map(|member| member.0),
        )
    }
}

impl<E: core::fmt::Debug> core::error::Error for ClosedSetError<E> {}

/// Parse a wire string into the closed-set enum `E`: the boundary
/// morphism whose admitted set is exactly the wire strings of
/// [`ClosedSet::MEMBERS`].
///
/// Matching is exact (case-sensitive, no trimming): the table is
/// injective and alias-free by contract, so each admitted input has
/// exactly one witness.
///
/// # Errors
///
/// Returns [`ClosedSetError`] — carrying the (bounded) offending
/// value and the expected set — when `raw` is not a member.
///
/// # Examples
///
/// ```
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Feature toggle wire form.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// // Admit: an exact member of the set.
/// assert_eq!(closed_set::parse::<Toggle>("on").unwrap(), Toggle::On);
///
/// // Reject: case-sensitive exact matching.
/// let err = closed_set::parse::<Toggle>("ON").unwrap_err();
/// assert_eq!(err.value(), "ON");
/// ```
#[inline]
pub fn parse<E: ClosedSet>(raw: &str) -> Result<E, ClosedSetError<E>> {
    const { E::VALID };
    for &(wire, variant) in E::MEMBERS {
        if wire == raw {
            return Ok(variant);
        }
    }
    Err(ClosedSetError::new(raw))
}

/// Return the wire form of `value`: the lossless inverse of
/// [`parse`] onto the [`ClosedSet::MEMBERS`] table.
///
/// `as_str` is total over every well-formed [`ClosedSet`] impl. For
/// macro-generated impls that is structural: the table has one row
/// per declared variant, so every value has a witness. For
/// hand-written impls it follows from the documented
/// variant-coverage obligation (see the [module docs](self)). No
/// fallback value exists or is wanted: returning a synthesized
/// string for an uncovered variant would silently violate the
/// `parse(as_str(v)) == Ok(v)` round-trip, so a violated obligation
/// surfaces as a loud panic naming the contract instead.
///
/// # Panics
///
/// Panics when `value` does not appear in `E::MEMBERS` — only
/// possible for a hand-written impl that violates the
/// variant-coverage obligation. This is the documented diagnostic
/// surface for a buggy hand-written table, the same surface a buggy
/// `ArbitraryRule` strategy gets from the blanket `Arbitrary` impl.
///
/// # Examples
///
/// ```
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Feature toggle wire form.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// assert_eq!(closed_set::as_str(Toggle::Off), "off");
///
/// // The round-trip law: parse ∘ as_str is the identity.
/// let round: Toggle = closed_set::parse(closed_set::as_str(Toggle::On)).unwrap();
/// assert_eq!(round, Toggle::On);
/// ```
#[inline]
#[must_use]
#[expect(
    clippy::panic,
    reason = "a variant missing from MEMBERS is a violated ClosedSet contract; \
              panicking with the obligation named is the documented diagnostic \
              surface for a buggy hand-written impl"
)]
pub fn as_str<E: ClosedSet>(value: E) -> &'static str {
    const { E::VALID };
    for &(wire, variant) in E::MEMBERS {
        if variant == value {
            return wire;
        }
    }
    panic!(
        "ClosedSet contract violated: every enum variant must appear in MEMBERS \
         (a hand-written impl omitted this value's row; the closed_set! macro \
         makes this unrepresentable)"
    );
}

// ─── Test support for hand-written impls. ─────────────────────────

/// Mechanically discharge the hand-written-impl obligations of
/// [`ClosedSet`] (see the [module docs](self)).
///
/// Hand-written impls call this once in a test; macro-generated
/// impls don't need it — the
/// [`closed_set!`](macro@crate::closed_set) declaration shape makes
/// the violations unrepresentable.
///
/// Checks, to the extent expressible over an unenumerable enum:
///
/// 1. **Wire-string injectivity** (and non-emptiness) — re-forced
///    at compile time via [`ClosedSet::VALID`]; a violating table
///    fails to compile at this call.
/// 2. **Variant coverage / table ↔ enum bijectivity** — for each
///    table row `(wire, variant)`, [`as_str`] applied to `variant`
///    must return that row's `wire`. A variant declared in two rows
///    (an alias) fails on the second row. Together with injectivity
///    this makes `parse(as_str(variant)) == Ok(variant)` hold for
///    every declared row — the `parse` ∘ `as_str` round-trip is a
///    corollary, not a separate runtime check.
///
/// What it cannot check: a variant that appears in **no** row. The
/// enum is not enumerable generically; that violation surfaces as
/// [`as_str`]'s own loud panic at first use.
///
/// # Panics
///
/// Panics naming the violated obligation when a table row's variant
/// does not map back to that row's wire string.
///
/// # Examples
///
/// ```
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Feature toggle wire form.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// // The hand-written impl's one-test obligation discharge.
/// closed_set::verify_table::<Toggle>();
/// ```
pub fn verify_table<E>()
where
    E: ClosedSet + core::fmt::Debug,
{
    const { E::VALID };
    for &(wire, variant) in E::MEMBERS {
        let round = as_str(variant);
        assert!(
            round == wire,
            "ClosedSet variant-coverage obligation violated: variant {variant:?} \
             maps to wire string \"{round}\" but the table also declares the row \
             (\"{wire}\", {variant:?}) — every variant must appear in MEMBERS \
             exactly once (aliases are not supported; the closed_set! macro makes \
             this unrepresentable)",
        );
    }
}

// ─── Serde: the wire shape is the plain wire string (P5). ─────────

/// Serialize `value` as its plain wire string ([`as_str`]) — no
/// enum-variant wrapping, the same shape the provider sent.
///
/// This is the body of the `Serialize` impl the
/// [`closed_set!`](macro@crate::closed_set) macro emits; the
/// by-reference signature also makes the module directly usable as
/// `#[serde(with = "whittle::closed_set")]` on a field whose type
/// is a hand-written [`ClosedSet`] impl.
///
/// # Errors
///
/// Propagates the serializer's own error; the wire form itself is
/// total (see [`as_str`]).
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::ClosedSet;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// #[derive(serde::Serialize)]
/// struct Payload {
///     #[serde(with = "whittle_core::closed_set")]
///     toggle: Toggle,
/// }
///
/// let value = serde_json::to_string(&Payload { toggle: Toggle::On }).unwrap();
///
/// assert_eq!(value, r#"{"toggle":"on"}"#);
/// # }
/// ```
#[cfg(feature = "serde")]
#[inline]
pub fn serialize<E, S>(value: &E, serializer: S) -> Result<S::Ok, S::Error>
where
    E: ClosedSet,
    S: serde::Serializer,
{
    serializer.serialize_str(as_str(*value))
}

/// Visitor admitting exactly the wire strings of `E`'s table.
#[cfg(feature = "serde")]
struct ClosedSetVisitor<E>(core::marker::PhantomData<E>);

#[cfg(feature = "serde")]
impl<E: ClosedSet> serde::de::Visitor<'_> for ClosedSetVisitor<E> {
    type Value = E;

    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("one of ")?;
        write_expected_list(
            f,
            E::MEMBERS.len(),
            &mut E::MEMBERS.iter().map(|member| member.0),
        )
    }

    fn visit_str<Failure>(self, raw: &str) -> Result<Self::Value, Failure>
    where
        Failure: serde::de::Error,
    {
        parse(raw).map_err(Failure::custom)
    }
}

/// Deserialize a closed-set enum from its plain wire string by
/// routing through [`parse`].
///
/// Untrusted ingress is gated by the same boundary morphism as
/// every other construction path, and a rejection surfaces
/// [`ClosedSetError`]'s domain diagnostics (bounded offending
/// value, expected set) through `serde::de::Error::custom`.
///
/// This is the body of the `Deserialize` impl the
/// [`closed_set!`](macro@crate::closed_set) macro emits, and the
/// `#[serde(with = "whittle::closed_set")]` hook for hand-written
/// impls.
///
/// # Errors
///
/// Returns the deserializer's error for non-string input, and the
/// rendered [`ClosedSetError`] when the string is not a member.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::ClosedSet;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// #[derive(serde::Deserialize)]
/// struct Payload {
///     #[serde(with = "whittle_core::closed_set")]
///     toggle: Toggle,
/// }
///
/// let value: Payload = serde_json::from_str(r#"{"toggle":"on"}"#).unwrap();
///
/// assert_eq!(value.toggle, Toggle::On);
/// # }
/// ```
#[cfg(feature = "serde")]
#[inline]
pub fn deserialize<'de, E, D>(deserializer: D) -> Result<E, D::Error>
where
    E: ClosedSet,
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(ClosedSetVisitor(core::marker::PhantomData))
}

// ─── Proptest strategies (behind the `proptest` feature). ─────────

/// Strategy generating admissible closed-set values by uniform
/// selection from the [`ClosedSet::MEMBERS`] table.
///
/// Admissible **by construction** — the support is exactly the
/// closed set, with no generate-then-filter step — and trivially
/// boundary-complete for small `n`: every member is in the support
/// of every run's sample space.
///
/// # Examples
///
/// ```
/// use proptest::strategy::Strategy as _;
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Feature toggle wire form.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// proptest::proptest! {
///     |(toggle in closed_set::admissible::<Toggle>())| {
///         // Every generated value round-trips through the table.
///         let wire = closed_set::as_str(toggle);
///         proptest::prop_assert_eq!(closed_set::parse::<Toggle>(wire).unwrap(), toggle);
///     }
/// }
/// ```
#[cfg(feature = "proptest")]
pub fn admissible<E>() -> impl proptest::strategy::Strategy<Value = E>
where
    E: ClosedSet + core::fmt::Debug,
{
    use proptest::strategy::Strategy as _;
    proptest::sample::select(E::MEMBERS).prop_map(|(_, variant)| variant)
}

/// Membership test against `E`'s table (the reject generators must
/// never emit an admissible value).
#[cfg(feature = "proptest")]
fn is_member<E: ClosedSet>(candidate: &str) -> bool {
    E::MEMBERS.iter().any(|&(wire, _)| wire == candidate)
}

/// Push `candidate` onto the derived reject list unless it is a
/// member of the closed set or already present.
#[cfg(feature = "proptest")]
fn push_reject<E: ClosedSet>(derived: &mut alloc::vec::Vec<String>, candidate: String) {
    if !is_member::<E>(&candidate) && !derived.contains(&candidate) {
        derived.push(candidate);
    }
}

/// Strategy generating reject inputs for `E`, derived from the
/// [`ClosedSet::MEMBERS`] table — no hand-maintained reject list.
///
/// The support mixes near-miss candidates derived from each member
/// (ASCII case-flips, last-character truncations, one-character
/// extensions, plus the empty string) with arbitrary strings, every
/// candidate filtered against membership so the strategy never
/// emits an admissible value. The derived list is never empty: a
/// maximal-length member's one-character extension is longer than
/// every member, hence never a member itself.
///
/// # Examples
///
/// ```
/// use proptest::strategy::Strategy as _;
/// use whittle_core::ClosedSet;
/// use whittle_core::closed_set;
///
/// /// Feature toggle wire form.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// enum Toggle {
///     On,
///     Off,
/// }
///
/// impl ClosedSet for Toggle {
///     const MEMBERS: &'static [(&'static str, Self)] =
///         &[("on", Self::On), ("off", Self::Off)];
/// }
///
/// proptest::proptest! {
///     |(raw in closed_set::rejects::<Toggle>())| {
///         // Every generated input is rejected by the parse.
///         proptest::prop_assert!(closed_set::parse::<Toggle>(&raw).is_err());
///     }
/// }
/// ```
#[cfg(feature = "proptest")]
pub fn rejects<E>() -> impl proptest::strategy::Strategy<Value = String>
where
    E: ClosedSet,
{
    use proptest::strategy::Strategy as _;
    let mut derived: alloc::vec::Vec<String> = alloc::vec::Vec::new();
    for candidate in near_miss_candidates(E::MEMBERS.iter().map(|&(wire, _)| wire)) {
        push_reject::<E>(&mut derived, candidate);
    }
    proptest::prop_oneof![
        proptest::sample::select(derived),
        proptest::arbitrary::any::<String>()
            .prop_filter("must not be a member of the closed set", |raw| {
                !is_member::<E>(raw)
            }),
    ]
}

/// Near-miss candidates derived from a set of wire labels: the empty
/// string, plus an ASCII case-flip, a last-character truncation, and
/// a one-character extension per label.
///
/// UNFILTERED by design — a candidate can collide with a label (the
/// truncation of `"ab"` in `{"a", "ab"}` is `"a"`), so each caller
/// classifies against its own membership determinant: [`rejects`]
/// filters against [`ClosedSet::MEMBERS`]; the schema boundary fold
/// ([`crate::schema::Schema::string_boundaries`]) classifies with the
/// schema's own membership verdict. One derivation, two consumers.
pub(crate) fn near_miss_candidates<'label, I>(labels: I) -> alloc::vec::Vec<String>
where
    I: IntoIterator<Item = &'label str>,
{
    let mut candidates: alloc::vec::Vec<String> = alloc::vec![String::new()];
    for wire in labels {
        let flipped: String = wire.chars().map(flip_ascii_case).collect();
        candidates.push(flipped);
        let mut truncated = String::from(wire);
        truncated.pop();
        candidates.push(truncated);
        let mut extended = String::from(wire);
        extended.push('x');
        candidates.push(extended);
    }
    candidates
}

/// Swap the ASCII case of `c` (identity on non-letters).
const fn flip_ascii_case(c: char) -> char {
    if c.is_ascii_uppercase() {
        c.to_ascii_lowercase()
    } else {
        c.to_ascii_uppercase()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString as _;

    use super::{ClosedSet, as_str, has_duplicate_wire_string, parse, str_eq, verify_table};

    /// First monomorphisation of the generic fns.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Status {
        Active,
        Inactive,
    }

    impl ClosedSet for Status {
        const MEMBERS: &'static [(&'static str, Self)] =
            &[("active", Self::Active), ("inactive", Self::Inactive)];
    }

    /// Second, distinct monomorphisation: the generic `parse` /
    /// `as_str` bodies are exercised through two different `E`
    /// instantiations (the per-monomorphisation coverage floor).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Toggle {
        On,
        Off,
    }

    impl ClosedSet for Toggle {
        const MEMBERS: &'static [(&'static str, Self)] = &[("on", Self::On), ("off", Self::Off)];
    }

    /// Ten members: drives the `Display` rendering cap (first 8 +
    /// `… (N total)`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Digit {
        Zero,
        One,
        Two,
        Three,
        Four,
        Five,
        Six,
        Seven,
        Eight,
        Nine,
    }

    impl ClosedSet for Digit {
        const MEMBERS: &'static [(&'static str, Self)] = &[
            ("zero", Self::Zero),
            ("one", Self::One),
            ("two", Self::Two),
            ("three", Self::Three),
            ("four", Self::Four),
            ("five", Self::Five),
            ("six", Self::Six),
            ("seven", Self::Seven),
            ("eight", Self::Eight),
            ("nine", Self::Nine),
        ];
    }

    /// Deliberately violates the variant-coverage obligation: a
    /// hand-written table omitting a variant's row. Exists only to
    /// cover `as_str`'s contract-violation diagnostic.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Lawless {
        Listed,
        Unlisted,
    }

    impl ClosedSet for Lawless {
        const MEMBERS: &'static [(&'static str, Self)] = &[("listed", Self::Listed)];
    }

    #[test]
    fn parse_admits_every_member_of_the_set() {
        assert_eq!(parse::<Status>("active").unwrap(), Status::Active);
        assert_eq!(parse::<Status>("inactive").unwrap(), Status::Inactive);
    }

    #[test]
    fn parse_rejects_non_member_with_value_and_expected_set() {
        let err = parse::<Status>("actve").unwrap_err();
        assert_eq!(err.value(), "actve");
        assert_eq!(err.expected(), Status::MEMBERS);
    }

    #[test]
    fn parse_is_case_sensitive() {
        parse::<Status>("ACTIVE").unwrap_err();
    }

    #[test]
    fn parse_second_monomorphisation_admits_and_rejects() {
        assert_eq!(parse::<Toggle>("off").unwrap(), Toggle::Off);
        parse::<Toggle>("offf").unwrap_err();
    }

    #[test]
    fn as_str_returns_the_wire_form_for_every_variant() {
        assert_eq!(as_str(Status::Active), "active");
        assert_eq!(as_str(Status::Inactive), "inactive");
        assert_eq!(as_str(Toggle::On), "on");
        assert_eq!(as_str(Toggle::Off), "off");
    }

    #[test]
    fn as_str_covers_the_listed_variant_of_the_lawless_table() {
        assert_eq!(as_str(Lawless::Listed), "listed");
    }

    #[test]
    #[should_panic(expected = "ClosedSet contract violated")]
    fn as_str_panics_when_a_variant_is_missing_from_members() {
        // `Lawless::Unlisted` has no table row: the documented
        // variant-coverage obligation is violated, so the totality
        // diagnostic fires.
        let _wire: &str = as_str(Lawless::Unlisted);
    }

    /// Deliberately violates the one-row-per-variant obligation: a
    /// hand-written table declaring the same variant under two wire
    /// strings (an alias). Exists only to cover `verify_table`'s
    /// rejection arm.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Aliased {
        On,
        Off,
    }

    impl ClosedSet for Aliased {
        const MEMBERS: &'static [(&'static str, Self)] =
            &[("on", Self::On), ("enabled", Self::On), ("off", Self::Off)];
    }

    #[test]
    fn verify_table_passes_for_well_formed_tables() {
        verify_table::<Status>();
        verify_table::<Toggle>();
        verify_table::<Digit>();
    }

    #[test]
    fn verify_table_cannot_see_an_omitted_variant() {
        // Documented expressibility limit: the enum is not
        // enumerable generically, so a variant with no row at all
        // (`Lawless::Unlisted`) is invisible to the row scan; that
        // violation surfaces through `as_str`'s own panic instead.
        verify_table::<Lawless>();
    }

    #[test]
    #[should_panic(expected = "ClosedSet variant-coverage obligation violated")]
    fn verify_table_panics_when_a_variant_is_declared_twice() {
        // `Aliased::On` appears in two rows: `as_str` resolves it to
        // the first row's wire, so the second row's check fails with
        // the obligation named.
        verify_table::<Aliased>();
    }

    #[test]
    fn display_renders_small_sets_in_full() {
        let err = parse::<Status>("actve").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid value "actve": expected one of "active", "inactive""#,
        );
    }

    #[test]
    fn display_caps_large_sets_at_eight_plus_total() {
        let err = parse::<Digit>("ten").unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid value \"ten\": expected one of \"zero\", \"one\", \"two\", \
             \"three\", \"four\", \"five\", \"six\", \"seven\", … (10 total)",
        );
    }

    #[test]
    fn display_escapes_unprintable_offending_values() {
        let err = parse::<Status>("act\nve").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid value "act\nve": expected one of "active", "inactive""#,
        );
    }

    #[test]
    fn error_value_is_truncated_to_the_char_bound() {
        let long = "x".repeat(200);
        let err = parse::<Status>(&long).unwrap_err();
        assert_eq!(err.value().chars().count(), 64);
        assert_eq!(err.value(), "x".repeat(64));
    }

    /// Writer with a budget of successful writes. Sweeping the
    /// budget from zero upward injects a formatter failure at every
    /// write boundary in a rendering, driving each `?`
    /// error-propagation branch.
    struct FailAfter {
        remaining: usize,
    }

    impl core::fmt::Write for FailAfter {
        fn write_str(&mut self, _s: &str) -> core::fmt::Result {
            if self.remaining == 0 {
                return Err(core::fmt::Error);
            }
            self.remaining -= 1;
            Ok(())
        }
    }

    /// Render `value` once per write budget in `0..256`: every
    /// prefix of the write sequence fails exactly once, and the
    /// sweep must eventually succeed (the rendering is finite).
    fn assert_display_propagates_at_every_write(value: &dyn core::fmt::Display) {
        let succeeded = (0..256).any(|budget| {
            let mut sink = FailAfter { remaining: budget };
            core::fmt::write(&mut sink, format_args!("{value}")).is_ok()
        });
        assert!(
            succeeded,
            "rendering did not complete within the write budget",
        );
    }

    #[test]
    fn display_propagates_formatter_errors_at_every_write() {
        // Small set: the uncapped list rendering.
        let small = parse::<Status>("nope").unwrap_err();
        assert_display_propagates_at_every_write(&small);
        // Large set: the capped rendering including the total suffix.
        let large = parse::<Digit>("ten").unwrap_err();
        assert_display_propagates_at_every_write(&large);
    }

    #[test]
    fn error_is_clonable_comparable_and_a_source_free_error() {
        let err = parse::<Status>("nope").unwrap_err();
        assert_eq!(err.clone(), err);
        let dyn_err: &dyn core::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn digit_set_admits_and_round_trips() {
        // Drive the third monomorphisation through both `parse`
        // branches and `as_str`.
        let nine = parse::<Digit>("nine").unwrap();
        assert_eq!(as_str(nine), "nine");
    }

    // ─── Const helpers, exercised at runtime for coverage. ────────

    #[test]
    fn str_eq_distinguishes_equal_prefixes_and_lengths() {
        assert!(str_eq("active", "active"));
        assert!(!str_eq("active", "activé"));
        assert!(!str_eq("active", "act"));
    }

    #[test]
    fn has_duplicate_wire_string_detects_pairwise_duplicates() {
        assert!(!has_duplicate_wire_string(&[("a", 0_u8), ("b", 1_u8)]));
        assert!(has_duplicate_wire_string(&[
            ("a", 0_u8),
            ("b", 1_u8),
            ("a", 2_u8),
        ]));
    }

    #[test]
    fn valid_default_passes_for_well_formed_tables() {
        // Force the side condition for each well-formed impl; a
        // failure would be a compile error, so compiling (and
        // reaching) these consts is the assertion.
        const { <Status as ClosedSet>::VALID };
        const { <Toggle as ClosedSet>::VALID };
        const { <Digit as ClosedSet>::VALID };
    }

    // ─── Serde codec (behind the `serde` feature). ────────────────

    #[cfg(feature = "serde")]
    #[test]
    fn serialize_writes_the_plain_wire_string() {
        let value = super::serialize(&Status::Active, serde_json::value::Serializer).unwrap();
        assert_eq!(value, serde_json::json!("active"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn deserialize_admits_members_through_parse() {
        let status: Status =
            super::deserialize(serde_json::Value::String("inactive".into())).unwrap();
        assert_eq!(status, Status::Inactive);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn deserialize_rejects_non_members_with_domain_diagnostics() {
        let err =
            super::deserialize::<Status, _>(serde_json::Value::String("nope".into())).unwrap_err();
        assert!(
            err.to_string()
                .contains(r#"invalid value "nope": expected one of "active", "inactive""#),
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn deserialize_reports_the_expected_set_on_type_mismatch() {
        // A non-string wire value drives the visitor's `expecting`
        // rendering (shared with `ClosedSetError`'s `Display`).
        let err = super::deserialize::<Status, _>(serde_json::Value::from(7_i32)).unwrap_err();
        let message = err.to_string();
        assert!(message.contains(r#"one of "active", "inactive""#));
    }

    /// Adapter exposing the visitor's `expecting` rendering as a
    /// `Display`, so the budget sweep can drive its `?` branch.
    #[cfg(feature = "serde")]
    struct ExpectingText;

    #[cfg(feature = "serde")]
    impl core::fmt::Display for ExpectingText {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            let visitor = super::ClosedSetVisitor::<Status>(core::marker::PhantomData);
            serde::de::Visitor::expecting(&visitor, f)
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn expecting_propagates_formatter_errors_at_every_write() {
        assert_display_propagates_at_every_write(&ExpectingText);
    }

    // ─── Proptest strategies (behind the `proptest` feature). ─────

    /// Case-flip collisions (`"a"` vs `"A"`) and a letterless wire
    /// (`"1"`, its own case-flip): drives the member-skip and
    /// dedupe paths of the derived reject generator.
    #[cfg(feature = "proptest")]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WireEdge {
        Lower,
        Upper,
        Digit,
    }

    #[cfg(feature = "proptest")]
    impl ClosedSet for WireEdge {
        const MEMBERS: &'static [(&'static str, Self)] =
            &[("a", Self::Lower), ("A", Self::Upper), ("1", Self::Digit)];
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn wire_edge_set_admits_and_round_trips() {
        // Cover this monomorphisation's admit branches; the
        // property below only ever drives the reject branch.
        assert_eq!(parse::<WireEdge>("a").unwrap(), WireEdge::Lower);
        assert_eq!(as_str(WireEdge::Digit), "1");
    }

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        /// `admissible` support is exactly the closed set: every
        /// generated value round-trips through the table.
        #[test]
        fn admissible_values_round_trip_through_the_table(
            status in super::admissible::<Status>()
        ) {
            let again = parse::<Status>(as_str(status)).unwrap();
            proptest::prop_assert_eq!(again, status);
        }

        /// Every derived or arbitrary reject input is rejected by
        /// the boundary morphism.
        #[test]
        fn reject_inputs_are_rejected(
            raw in super::rejects::<Status>()
        ) {
            proptest::prop_assert!(parse::<Status>(&raw).is_err());
        }

        /// Same property over the edge-case wires (case-flip
        /// collisions, letterless member): the generator skips
        /// candidates that are themselves members and stays sound.
        #[test]
        fn reject_inputs_for_edge_wires_are_rejected(
            raw in super::rejects::<WireEdge>()
        ) {
            proptest::prop_assert!(parse::<WireEdge>(&raw).is_err());
        }
    }
}
