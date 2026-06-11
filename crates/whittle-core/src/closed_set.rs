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
//!   offending value and a `'static` borrow of the expected table.
//!
//! # Hand-written impl obligations
//!
//! A hand-written [`ClosedSet`] impl carries two obligations:
//!
//! 1. **Wire-string injectivity** — no two table entries share a
//!    wire string.
//! 2. **Variant coverage** — every variant of the enum appears in
//!    `MEMBERS` exactly once. The type system cannot see "every
//!    variant" generically, so this is a documented contract;
//!    [`as_str`] panics loudly when it is violated (see its docs
//!    for why that is a contract-violation diagnostic, not a
//!    fallback).

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
/// wire strings and no duplicate variants (a documented obligation
/// for hand-written impls; see the [module docs](self)). Aliases
/// (many wire strings mapping to one variant) are deliberately not
/// supported.
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
/// let status: ActivityStatus = closed_set::parse("active").unwrap();
/// assert_eq!(status, ActivityStatus::Active);
/// assert_eq!(closed_set::as_str(status), "active");
/// ```
pub trait ClosedSet: Copy + PartialEq + Sized + 'static {
    /// The single determinant: the injective wire-string ↔ variant
    /// table. Order is the declaration order and is what
    /// [`ClosedSetError`]'s `Display` renders.
    const MEMBERS: &'static [(&'static str, Self)];
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
    #[inline]
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The expected set: a `'static` borrow of the rejecting
    /// [`ClosedSet::MEMBERS`] table.
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
/// `as_str` is total over every well-formed [`ClosedSet`] impl: by
/// the variant-coverage obligation (see the [module docs](self))
/// the table has one row per variant, so every value has a
/// witness. No
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
    for &(wire, variant) in E::MEMBERS {
        if variant == value {
            return wire;
        }
    }
    panic!(
        "ClosedSet contract violated: every enum variant must appear in MEMBERS \
         (a hand-written impl omitted this value's row)"
    );
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString as _;

    use super::{ClosedSet, as_str, parse};

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
}
