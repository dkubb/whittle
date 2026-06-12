//! Schema reflection: a constructive, runtime-introspectable
//! description of a rule's admitted set (IDEA §5.9).
//!
//! [`Rule::refine`] is the *predicative*
//! description of a rule's admitted set — it tests membership.
//! [`Schema`] is the *constructive* counterpart: a first-class value
//! describing the same set, so derived views (generators, boundary
//! matrices, residual-state reports, schema diffs) can read one
//! determinant instead of restating the bounds.
//!
//! # The scalar universe
//!
//! Interval endpoints live in a single scalar universe ([`Scalar`]):
//! integers (and integer-encoded carriers — days from CE, seconds
//! since the Unix epoch, decimal mantissas) widen into `i128`; floats
//! widen into `f64`. [`ScalarKind`] records which carrier domain an
//! interval describes, so a date interval and a plain integer
//! interval never compare equal even when their endpoint numbers
//! coincide.
//!
//! # Canonical form
//!
//! Smart constructors maintain canonical form, so the set of
//! constructor-built values approximates the set of canonical trees:
//!
//! - [`Schema::interval`] requires non-`NaN` endpoints with
//!   `lo <= hi`, normalises `-0.0` to `0.0`, and reduces decimal
//!   intervals to their smallest shared scale;
//! - [`Schema::union`] and [`Schema::intersection`] are flattened,
//!   sorted, and deduplicated, and a single operand collapses to the
//!   operand itself;
//! - [`Schema::intersection`] fuses same-kind intervals into one
//!   interval;
//! - [`Schema::enumerated`] requires a non-empty, duplicate-free
//!   label set.
//!
//! Construction order does not affect the normal form (confluence);
//! the property tests in this module pin that invariant.
//!
//! # Equality, ordering, and rendering are UNSTABLE
//!
//! `Eq`/`Ord` are *structural on the canonical form*. Equality is
//! sound — canonically-equal schemas describe equal admitted sets —
//! but incomplete: semantically equivalent schemas spelled through
//! vocabulary the canonicalizer does not rewrite (equivalent regexes,
//! unfused heterogeneous intersections) compare unequal. The exact
//! canonical form, the `Ord` ordering, and the `Display` rendering
//! are NOT stable across whittle versions; do not persist them or
//! match on their text.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::rule::Rule;

/// An endpoint value in the schema's scalar universe.
///
/// Integer-regime carriers (integers, dates as days from CE,
/// datetimes as seconds since the Unix epoch, decimal mantissas)
/// widen into [`Scalar::Int`]; floats widen into [`Scalar::Float`].
///
/// # Ordering vs membership
///
/// `Eq`/`Ord` are the *structural* total order used for canonical
/// sorting: integers by value, floats by [`f64::total_cmp`], and
/// `Int` before `Float` across variants. Denotational *membership*
/// checks ([`Schema::scalar_membership`]) instead compare floats by
/// IEEE-754 `partial_cmp` — the same comparison `refine` impls use —
/// and never compare across variants.
///
/// # Examples
///
/// ```
/// use whittle_core::schema::Scalar;
///
/// assert!(Scalar::Int(1) < Scalar::Int(2));
/// assert!(Scalar::Float(1.0) < Scalar::Float(f64::INFINITY));
/// // Structural order: Int sorts before Float regardless of value.
/// assert!(Scalar::Int(9) < Scalar::Float(0.0));
/// ```
#[derive(Clone, Copy, Debug)]
pub enum Scalar {
    /// Integer-regime endpoint, widened losslessly into `i128`.
    Int(i128),
    /// Float endpoint, widened losslessly into `f64`.
    Float(f64),
}

impl Scalar {
    /// The integer payload, when this scalar is [`Scalar::Int`].
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Scalar;
    ///
    /// assert_eq!(Scalar::Int(7).as_int(), Some(7));
    /// assert_eq!(Scalar::Float(7.0).as_int(), None);
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_int(&self) -> Option<i128> {
        match *self {
            Self::Int(value) => Some(value),
            Self::Float(_) => None,
        }
    }

    /// The float payload, when this scalar is [`Scalar::Float`].
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Scalar;
    ///
    /// assert_eq!(Scalar::Float(0.5).as_float(), Some(0.5));
    /// assert_eq!(Scalar::Int(1).as_float(), None);
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_float(&self) -> Option<f64> {
        match *self {
            Self::Float(value) => Some(value),
            Self::Int(_) => None,
        }
    }

    /// Denotational comparison: integers by value, floats by IEEE-754
    /// `partial_cmp` (`None` for NaN operands), `None` across
    /// variants. This is the comparison membership uses; the `Ord`
    /// impl is the structural total order for canonical sorting.
    fn denotational_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match (*self, *other) {
            (Self::Int(a), Self::Int(b)) => Some(a.cmp(&b)),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(&b),
            (Self::Int(_), Self::Float(_)) | (Self::Float(_), Self::Int(_)) => None,
        }
    }

    /// `true` iff this is a `NaN` float endpoint.
    const fn is_nan(&self) -> bool {
        match *self {
            Self::Float(value) => value.is_nan(),
            Self::Int(_) => false,
        }
    }

    /// Canonicalise the scalar: `-0.0` becomes `0.0` so the two
    /// IEEE-equal zeros share one structural form.
    const fn canonicalized(self) -> Self {
        if let Self::Float(value) = self
            && value.to_bits() == (-0.0_f64).to_bits()
        {
            return Self::Float(0.0_f64);
        }
        self
    }
}

impl PartialEq for Scalar {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == core::cmp::Ordering::Equal
    }
}

impl Eq for Scalar {}

impl PartialOrd for Scalar {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Scalar {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (*self, *other) {
            (Self::Int(a), Self::Int(b)) => a.cmp(&b),
            (Self::Float(a), Self::Float(b)) => a.total_cmp(&b),
            (Self::Int(_), Self::Float(_)) => core::cmp::Ordering::Less,
            (Self::Float(_), Self::Int(_)) => core::cmp::Ordering::Greater,
        }
    }
}

/// The carrier domain an [`Schema::Interval`] describes.
///
/// The kind disambiguates integer-encoded carriers that share the
/// `i128` widening: a date interval and a plain integer interval with
/// the same endpoint numbers describe different admitted sets and
/// must not compare equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScalarKind {
    /// Plain integers, widened into `i128` (the
    /// [`Numeric`](crate::primitive::Numeric) widening).
    Integer,
    /// IEEE-754 floats, widened into `f64`.
    Float,
    /// Calendar dates, encoded as days from CE
    /// (`chrono::NaiveDate::num_days_from_ce`).
    Date,
    /// UTC datetimes, encoded as seconds since the Unix epoch
    /// (`chrono::DateTime::timestamp`).
    DateTime,
    /// Fixed-point decimals, encoded as `i128` mantissas at the
    /// recorded scale: the denoted value is `mantissa / 10^scale`.
    /// Canonical intervals carry the smallest scale that represents
    /// both endpoints exactly (trailing zeros are stripped jointly).
    Decimal {
        /// Digits after the decimal point shared by both endpoints.
        scale: u8,
    },
}

impl ScalarKind {
    /// `true` iff `scalar`'s variant matches this kind's regime:
    /// [`Scalar::Float`] for [`ScalarKind::Float`], [`Scalar::Int`]
    /// for every other kind.
    const fn admits(self, scalar: Scalar) -> bool {
        match self {
            Self::Float => matches!(scalar, Scalar::Float(_)),
            Self::Integer | Self::Date | Self::DateTime | Self::Decimal { .. } => {
                matches!(scalar, Scalar::Int(_))
            }
        }
    }
}

/// One end of an [`Schema::Interval`].
///
/// Only inclusive finite endpoints exist: every shipped rule's
/// admitted set is closed at its finite ends (open integer bounds
/// normalise to the adjacent inclusive bound; open float bounds have
/// no producer). An exclusive variant would be representable state
/// with no inhabitant — it is added when a producer needs it, as a
/// deliberately loud enum extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bound {
    /// No bound at this end: the admitted set extends to the
    /// carrier's own limit.
    Unbounded,
    /// Inclusive endpoint: the value itself is admitted.
    Inclusive(Scalar),
}

impl Bound {
    /// The inclusive endpoint scalar, when present.
    const fn scalar(&self) -> Option<Scalar> {
        match *self {
            Self::Inclusive(scalar) => Some(scalar),
            Self::Unbounded => None,
        }
    }
}

/// Length unit for [`Schema::Str`] bounds.
///
/// `LenChars` counts Unicode scalar values; `LenBytes` counts UTF-8
/// bytes. The two units admit different sets for the same numeric
/// bounds, so the unit is part of the canonical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LenUnit {
    /// Unicode scalar values (`str::chars().count()`).
    Chars,
    /// UTF-8 bytes (`str::len()`).
    Bytes,
}

/// Closed length range for [`Schema::Str`] and [`Schema::Collection`]
/// nodes: `min <= length <= max`, both ends inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LenBound {
    /// Minimum admitted length (inclusive).
    pub min: u64,
    /// Maximum admitted length (inclusive).
    pub max: u64,
}

impl LenBound {
    /// Build a length bound.
    ///
    /// # Panics
    ///
    /// Panics when `min > max`: an empty length range admits nothing,
    /// and empty admitted sets are unrepresentable by construction
    /// (the same posture as the compile-time `MIN <= MAX` asserts on
    /// the rules themselves).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::LenBound;
    ///
    /// let len = LenBound::new(1, 64);
    /// assert_eq!((len.min, len.max), (1, 64));
    /// ```
    #[inline]
    #[must_use]
    pub const fn new(min: u64, max: u64) -> Self {
        assert!(
            min <= max,
            "LenBound: min must be <= max (an empty length range admits nothing)",
        );
        Self { min, max }
    }
}

/// A set of characters, canonically represented as sorted, disjoint,
/// non-adjacent inclusive ranges. The constructive form of a
/// [`CharPredicate`](crate::primitive::CharPredicate)'s admitted set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CharSet {
    /// Sorted, disjoint, non-adjacent inclusive ranges.
    ranges: Vec<(char, char)>,
}

impl CharSet {
    /// Build a character set from inclusive ranges, normalising to
    /// canonical form: ranges are sorted, and overlapping or adjacent
    /// ranges are merged, so equal sets have equal representations
    /// regardless of construction order.
    ///
    /// # Panics
    ///
    /// Panics when a range is empty (`lo > hi`) or when the resulting
    /// set is empty (no ranges): an empty character set admits
    /// nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::CharSet;
    ///
    /// // Overlapping and adjacent ranges merge; order is irrelevant.
    /// let a = CharSet::from_ranges([('a', 'm'), ('n', 'z')]);
    /// let b = CharSet::from_ranges([('n', 'z'), ('a', 'p')]);
    /// assert_eq!(a, b);
    /// assert_eq!(a.ranges(), &[('a', 'z')]);
    /// ```
    #[must_use]
    pub fn from_ranges<I>(ranges: I) -> Self
    where
        I: IntoIterator<Item = (char, char)>,
    {
        let mut ranges: Vec<(char, char)> = ranges.into_iter().collect();
        for &(lo, hi) in &ranges {
            assert!(
                lo <= hi,
                "CharSet: every range must satisfy lo <= hi (an empty range admits nothing)",
            );
        }
        ranges.sort_unstable();
        let mut merged: Vec<(char, char)> = Vec::with_capacity(ranges.len());
        for (lo, hi) in ranges {
            match merged.last_mut() {
                Some(last) if lo <= char_successor(last.1) => {
                    last.1 = last.1.max(hi);
                }
                _ => merged.push((lo, hi)),
            }
        }
        assert!(
            !merged.is_empty(),
            "CharSet: at least one range is required (an empty set admits nothing)",
        );
        Self { ranges: merged }
    }

    /// The canonical ranges: sorted, disjoint, non-adjacent, each
    /// inclusive at both ends.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::CharSet;
    ///
    /// let digits = CharSet::from_ranges([('0', '9')]);
    /// assert_eq!(digits.ranges(), &[('0', '9')]);
    /// ```
    #[inline]
    #[must_use]
    pub fn ranges(&self) -> &[(char, char)] {
        &self.ranges
    }

    /// Membership test: `true` iff `ch` falls inside one of the
    /// canonical ranges.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::CharSet;
    ///
    /// let ident = CharSet::from_ranges([('a', 'z'), ('_', '_')]);
    /// assert!(ident.contains('q'));
    /// assert!(ident.contains('_'));
    /// assert!(!ident.contains('-'));
    /// ```
    #[must_use]
    pub fn contains(&self, ch: char) -> bool {
        self.ranges.iter().any(|&(lo, hi)| lo <= ch && ch <= hi)
    }

    /// The set difference `self \ other`, in canonical form, or
    /// `None` when nothing remains (a `CharSet` cannot be empty —
    /// an empty set admits nothing).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::CharSet;
    ///
    /// let letters = CharSet::from_ranges([('a', 'z')]);
    /// let vowels = CharSet::from_ranges([('a', 'a'), ('e', 'e')]);
    /// let consonants = letters.difference(&vowels).expect("non-empty");
    /// assert_eq!(consonants.ranges(), &[('b', 'd'), ('f', 'z')]);
    ///
    /// // Subtracting a superset leaves nothing.
    /// assert_eq!(vowels.difference(&letters), None);
    /// ```
    #[must_use]
    pub fn difference(&self, other: &Self) -> Option<Self> {
        let mut out: Vec<(char, char)> = Vec::new();
        for &(lo, hi) in &self.ranges {
            let mut cursor = lo;
            let mut consumed = false;
            for &(other_lo, other_hi) in &other.ranges {
                if other_hi < cursor {
                    continue;
                }
                if other_lo > hi {
                    break;
                }
                if other_lo > cursor {
                    out.push((cursor, char_predecessor(other_lo)));
                }
                if other_hi >= hi {
                    consumed = true;
                    break;
                }
                cursor = char_successor(other_hi);
            }
            if !consumed {
                out.push((cursor, hi));
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(Self::from_ranges(out))
        }
    }

    /// The smallest Unicode scalar value OUTSIDE the set, when one
    /// exists (`None` when the set covers every scalar value). The
    /// string boundary fold uses it as the canonical alphabet
    /// near-miss character.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::CharSet;
    ///
    /// let digits = CharSet::from_ranges([('0', '9')]);
    /// assert_eq!(digits.complement_sample(), Some('\0'));
    ///
    /// let low = CharSet::from_ranges([('\0', '9')]);
    /// assert_eq!(low.complement_sample(), Some(':'));
    ///
    /// let everything = CharSet::from_ranges([('\0', char::MAX)]);
    /// assert_eq!(everything.complement_sample(), None);
    /// ```
    #[must_use]
    pub fn complement_sample(&self) -> Option<char> {
        // Non-empty by construction, so the first range exists.
        let (lo, hi) = self.ranges[0];
        if lo > '\0' {
            Some('\0')
        } else if hi == char::MAX {
            // A canonical first range spanning the whole universe is
            // necessarily the only range: full coverage.
            None
        } else {
            // Canonical ranges are non-adjacent: the successor of
            // the first range's end is in the gap before the next.
            Some(char_successor(hi))
        }
    }
}

/// The next Unicode scalar value after `c`, skipping the surrogate
/// gap; saturates at `char::MAX`. Used by [`CharSet`] adjacency
/// merging: ranges `('a', 'm')` and `('n', 'z')` are adjacent because
/// `'n'` is `'m'`'s successor.
const fn char_successor(c: char) -> char {
    if c as u32 == 0xD7FF {
        '\u{E000}'
    } else {
        match char::from_u32(c as u32 + 1) {
            Some(next) => next,
            None => char::MAX,
        }
    }
}

/// The previous Unicode scalar value before `c`, skipping the
/// surrogate gap; saturates at `'\0'`. The mirror of
/// [`char_successor`], used by [`CharSet::difference`] to close a
/// kept run just below a subtracted range.
fn char_predecessor(c: char) -> char {
    if c as u32 == 0xE000 {
        '\u{D7FF}'
    } else {
        // `wrapping_sub` turns the (unreachable) `'\0'` input into
        // u32::MAX, which is not a scalar value, so the fallback
        // saturates at NUL without a branch of its own.
        char::from_u32((c as u32).wrapping_sub(1)).unwrap_or('\0')
    }
}

/// A canonicalisation morphism recorded by [`Schema::Canonicalized`].
///
/// The morphism is the transformation `refine` applies to raw input
/// before the inner rule runs. The carried set is the inner schema's
/// set (the morphism's fixed points within it); the morphism
/// describes which raw inputs reach it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Morphism {
    /// Leading/trailing-whitespace removal
    /// ([`Trim`](crate::transform::Trim)).
    Trim,
    /// ASCII lowercasing
    /// ([`AsciiLowercase`](crate::transform::AsciiLowercase)).
    AsciiLowercase,
    /// ASCII uppercasing
    /// ([`AsciiUppercase`](crate::transform::AsciiUppercase)).
    AsciiUppercase,
}

/// One scalar test point of a derived boundary matrix
/// ([`Schema::scalar_boundaries`]): a value at or adjacent to an
/// interval endpoint, paired with the schema's own membership verdict.
///
/// The verdict is read off the schema itself — the single
/// constructive determinant — so a test consuming the matrix never
/// restates the bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScalarBoundary {
    /// Carrier domain of the test point.
    pub kind: ScalarKind,
    /// The test point in the scalar universe.
    pub value: Scalar,
    /// The schema's membership verdict for the point: `true` means
    /// the admitted set contains it.
    pub admitted: bool,
}

/// One string test point of a derived boundary matrix
/// ([`Schema::string_boundaries`]): a label, near-miss, or
/// length/alphabet edge case, paired with the schema's own
/// membership verdict.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StringBoundary {
    /// The candidate string.
    pub value: String,
    /// The schema's membership verdict for the candidate: `true`
    /// means the admitted set contains it.
    pub admitted: bool,
}

/// Constructive description of an admitted set.
///
/// Build values through the smart constructors ([`Schema::interval`],
/// [`Schema::union`], …), which maintain the canonical form the
/// module docs describe; `Eq`/`Ord`/`Display` are only meaningful on
/// canonically-constructed values. The enum is a deliberately closed
/// sum: a new node kind is a breaking change that every consumer
/// match must acknowledge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Schema {
    /// A scalar interval, closed at each finite end. The admitted set
    /// is the interval intersected with the carrier's own
    /// representable range under the kind's encoding.
    Interval {
        /// Carrier domain of the endpoints.
        kind: ScalarKind,
        /// Lower end (inclusive when finite).
        lo: Bound,
        /// Upper end (inclusive when finite).
        hi: Bound,
    },
    /// A string constrained by length and alphabet.
    Str {
        /// Admitted length range.
        len: LenBound,
        /// Unit the length range counts.
        unit: LenUnit,
        /// Characters admitted at every position.
        alphabet: CharSet,
        /// Stricter set for the first character, when the rule
        /// distinguishes it ([`FirstChar`](crate::primitive::FirstChar)).
        first: Option<CharSet>,
    },
    /// Strings matching a regular expression; the pattern is the
    /// fragment ([`Pattern`](crate::primitive::Pattern)'s const
    /// generic).
    Regex(&'static str),
    /// A closed set of admitted wire strings: the
    /// [`ClosedSet::MEMBERS`](crate::ClosedSet::MEMBERS) labels in
    /// declaration order.
    Enumerated(&'static [&'static str]),
    /// A homogeneous collection constrained by length, element
    /// schema, and ordering/uniqueness invariants.
    Collection {
        /// Admitted item-count range.
        len: LenBound,
        /// Schema every element satisfies.
        element: Box<Self>,
        /// Elements are sorted ascending
        /// ([`Sorted`](crate::primitive::Sorted)).
        sorted: bool,
        /// Elements are pairwise distinct
        /// ([`Distinct`](crate::primitive::Distinct)).
        unique: bool,
    },
    /// The union of the members' sets. Canonical: flattened, sorted,
    /// deduplicated, and never a singleton (a single member collapses
    /// to the member itself), so at least two members are present.
    Union(Vec<Self>),
    /// The intersection of the members' sets — the residual symbolic
    /// form for operands the canonicalizer cannot fuse. Canonical:
    /// flattened, sorted, deduplicated, same-kind intervals fused,
    /// never a singleton.
    Intersection(Vec<Self>),
    /// A canonicalising rule: the carried set is `inner`'s set; the
    /// recorded morphism maps raw input onto it (so the raw-input
    /// preimage is the morphism's preimage of the inner set).
    Canonicalized {
        /// The canonicalisation applied to raw input.
        morphism: Morphism,
        /// Schema of the carried (post-morphism) set.
        inner: Box<Self>,
    },
}

impl Schema {
    /// Build a scalar interval, canonicalising the endpoints.
    ///
    /// Canonicalisation: float endpoints normalise `-0.0` to `0.0`;
    /// decimal intervals reduce to the smallest scale representing
    /// both endpoints exactly (trailing zeros stripped jointly, so
    /// the same value set has one representation regardless of the
    /// declared scale).
    ///
    /// # Panics
    ///
    /// Panics when an endpoint's scalar variant does not match the
    /// kind's regime, when an endpoint is `NaN`, or when both ends
    /// are finite with `lo > hi` (an empty interval admits nothing;
    /// empty admitted sets are unrepresentable by construction).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
    ///
    /// // `Within<0, 100>`'s admitted set.
    /// let percent = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    ///
    /// // Decimal scales reduce: 0.00..=100.00 at scale 2 is the same
    /// // value set as 0..=100 at scale 0.
    /// let wide = Schema::interval(
    ///     ScalarKind::Decimal { scale: 2 },
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(10_000)),
    /// );
    /// let narrow = Schema::interval(
    ///     ScalarKind::Decimal { scale: 0 },
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    /// assert_eq!(wide, narrow);
    /// assert_ne!(percent, narrow); // kinds differ
    /// ```
    #[must_use]
    pub fn interval(kind: ScalarKind, lo: Bound, hi: Bound) -> Self {
        let lo = canonical_bound(lo);
        let hi = canonical_bound(hi);
        for bound in [&lo, &hi] {
            if let Some(scalar) = bound.scalar() {
                assert!(
                    kind.admits(scalar),
                    "Schema::interval: endpoint scalar variant must match the kind's regime",
                );
                assert!(
                    !scalar.is_nan(),
                    "Schema::interval: NaN is not an admissible endpoint",
                );
            }
        }
        if let (Some(lo_scalar), Some(hi_scalar)) = (lo.scalar(), hi.scalar()) {
            assert!(
                lo_scalar
                    .denotational_cmp(&hi_scalar)
                    .is_some_and(core::cmp::Ordering::is_le),
                "Schema::interval: lo must be <= hi (an empty interval admits nothing)",
            );
        }
        let (kind, lo, hi) = reduce_decimal_scale(kind, lo, hi);
        Self::Interval { kind, lo, hi }
    }

    /// Build a string schema from its length bound, length unit,
    /// alphabet, and optional first-character set.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{CharSet, LenBound, LenUnit, Schema};
    ///
    /// let ident = Schema::string(
    ///     LenBound::new(1, 64),
    ///     LenUnit::Chars,
    ///     CharSet::from_ranges([('a', 'z'), ('0', '9'), ('_', '_')]),
    ///     Some(CharSet::from_ranges([('a', 'z'), ('_', '_')])),
    /// );
    /// assert_eq!(ident, ident.clone());
    /// ```
    #[inline]
    #[must_use]
    pub const fn string(
        len: LenBound,
        unit: LenUnit,
        alphabet: CharSet,
        first: Option<CharSet>,
    ) -> Self {
        Self::Str {
            len,
            unit,
            alphabet,
            first,
        }
    }

    /// Build a regex schema; the pattern string is the fragment.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Schema;
    ///
    /// let name = Schema::regex(r"^[A-Z][a-z]*$");
    /// assert_eq!(name, Schema::Regex(r"^[A-Z][a-z]*$"));
    /// ```
    #[inline]
    #[must_use]
    pub const fn regex(pattern: &'static str) -> Self {
        Self::Regex(pattern)
    }

    /// Build an enumerated schema from a closed set's labels, in
    /// declaration order.
    ///
    /// # Panics
    ///
    /// Panics when `labels` is empty (an empty closed set admits
    /// nothing) or contains a duplicate (the table injectivity that
    /// [`ClosedSet::VALID`](crate::ClosedSet::VALID) enforces at
    /// compile time).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Schema;
    ///
    /// let status = Schema::enumerated(&["active", "inactive"]);
    /// assert_eq!(status.as_enumerated(), Some(&["active", "inactive"][..]));
    /// ```
    #[must_use]
    pub fn enumerated(labels: &'static [&'static str]) -> Self {
        assert!(
            !labels.is_empty(),
            "Schema::enumerated: at least one label is required (an empty set admits nothing)",
        );
        for (index, label) in labels.iter().enumerate() {
            assert!(
                !labels[..index].contains(label),
                "Schema::enumerated: labels must be duplicate-free",
            );
        }
        Self::Enumerated(labels)
    }

    /// Build a collection schema from its length bound, element
    /// schema, and ordering/uniqueness invariants.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, LenBound, Scalar, ScalarKind, Schema};
    ///
    /// let bytes = Schema::collection(
    ///     LenBound::new(1, 32),
    ///     Schema::interval(
    ///         ScalarKind::Integer,
    ///         Bound::Inclusive(Scalar::Int(0)),
    ///         Bound::Inclusive(Scalar::Int(255)),
    ///     ),
    ///     true,
    ///     true,
    /// );
    /// assert_eq!(bytes, bytes.clone());
    /// ```
    #[inline]
    #[must_use]
    pub fn collection(len: LenBound, element: Self, sorted: bool, unique: bool) -> Self {
        Self::Collection {
            len,
            element: Box::new(element),
            sorted,
            unique,
        }
    }

    /// Build the union of `members`' sets, canonicalising: nested
    /// unions flatten, members sort and deduplicate, and a single
    /// remaining member collapses to the member itself.
    ///
    /// # Panics
    ///
    /// Panics when `members` is empty: an empty union admits nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
    ///
    /// let below = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Unbounded,
    ///     Bound::Inclusive(Scalar::Int(-1)),
    /// );
    /// let above = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(1)),
    ///     Bound::Unbounded,
    /// );
    ///
    /// // `NonZero`'s admitted set; member order is irrelevant.
    /// let non_zero = Schema::union([below.clone(), above.clone()].into());
    /// assert_eq!(non_zero, Schema::union([above, below.clone()].into()));
    ///
    /// // A singleton union collapses to its member.
    /// assert_eq!(Schema::union([below.clone()].into()), below);
    /// ```
    #[must_use]
    pub fn union(members: Vec<Self>) -> Self {
        assert!(
            !members.is_empty(),
            "Schema::union: at least one member is required (an empty union admits nothing)",
        );
        let mut flat: Vec<Self> = Vec::with_capacity(members.len());
        for member in members {
            if let Self::Union(inner) = member {
                flat.extend(inner);
            } else {
                flat.push(member);
            }
        }
        flat.sort_unstable();
        flat.dedup();
        collapse_singleton(flat, Self::Union)
    }

    /// Build the intersection of `members`' sets, canonicalising:
    /// nested intersections flatten, same-kind intervals fuse into
    /// one interval, members sort and deduplicate, and a single
    /// remaining member collapses to the member itself.
    ///
    /// # Panics
    ///
    /// Panics when `members` is empty, or when fusing same-kind
    /// intervals produces an empty interval (the intersection admits
    /// nothing — the schema analogue of the compile-time
    /// `MIN <= MAX` asserts on the rules themselves).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
    ///
    /// let at_least = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Unbounded,
    /// );
    /// let at_most = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Unbounded,
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    ///
    /// // Same-kind intervals fuse: the result IS `Within<0, 100>`'s
    /// // interval, not a symbolic intersection.
    /// let within = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    /// assert_eq!(Schema::intersection([at_least, at_most].into()), within);
    /// ```
    #[must_use]
    pub fn intersection(members: Vec<Self>) -> Self {
        assert!(
            !members.is_empty(),
            "Schema::intersection: at least one member is required",
        );
        let mut intervals: Vec<(ScalarKind, Bound, Bound)> = Vec::new();
        let mut others: Vec<Self> = Vec::new();
        let mut queue: Vec<Self> = members;
        while let Some(member) = queue.pop() {
            if let Self::Intersection(inner) = member {
                queue.extend(inner);
            } else if let Self::Interval { kind, lo, hi } = member {
                match intervals.iter_mut().find(|(k, _, _)| *k == kind) {
                    Some((_, fused_lo, fused_hi)) => {
                        *fused_lo = fuse_lo(*fused_lo, lo);
                        *fused_hi = fuse_hi(*fused_hi, hi);
                    }
                    None => intervals.push((kind, lo, hi)),
                }
            } else {
                others.push(member);
            }
        }
        let mut flat = others;
        for (kind, lo, hi) in intervals {
            // Re-canonicalise: fusion may expose a reducible decimal
            // scale, and an empty fusion panics here with the
            // non-empty contract named.
            flat.push(Self::interval(kind, lo, hi));
        }
        flat.sort_unstable();
        flat.dedup();
        collapse_singleton(flat, Self::Intersection)
    }

    /// Build a canonicalised schema: the carried set is `inner`'s
    /// set; `morphism` records the transformation applied to raw
    /// input.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Morphism, Schema};
    ///
    /// let trimmed = Schema::canonicalized(
    ///     Morphism::Trim,
    ///     Schema::enumerated(&["on", "off"]),
    /// );
    /// assert_eq!(trimmed, trimmed.clone());
    /// ```
    #[inline]
    #[must_use]
    pub fn canonicalized(morphism: Morphism, inner: Self) -> Self {
        Self::Canonicalized {
            morphism,
            inner: Box::new(inner),
        }
    }

    /// The enumerated labels, when this schema is
    /// [`Schema::Enumerated`].
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Schema;
    ///
    /// let status = Schema::enumerated(&["on", "off"]);
    /// assert_eq!(status.as_enumerated(), Some(&["on", "off"][..]));
    /// assert_eq!(Schema::regex("a").as_enumerated(), None);
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_enumerated(&self) -> Option<&'static [&'static str]> {
        if let Self::Enumerated(labels) = *self {
            Some(labels)
        } else {
            None
        }
    }

    /// Every finite interval endpoint in the tree, paired with its
    /// kind: the boundary values a derived test matrix samples.
    /// Recurses through unions, intersections, collections (element
    /// endpoints), and canonicalised inners; non-interval leaves
    /// contribute nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
    ///
    /// let percent = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    /// assert_eq!(
    ///     percent.interval_endpoints(),
    ///     [
    ///         (ScalarKind::Integer, Scalar::Int(0)),
    ///         (ScalarKind::Integer, Scalar::Int(100)),
    ///     ],
    /// );
    /// ```
    #[must_use]
    pub fn interval_endpoints(&self) -> Vec<(ScalarKind, Scalar)> {
        let mut endpoints = Vec::new();
        self.collect_interval_endpoints(&mut endpoints);
        endpoints
    }

    fn collect_interval_endpoints(&self, endpoints: &mut Vec<(ScalarKind, Scalar)>) {
        match self {
            Self::Interval { kind, lo, hi } => {
                for bound in [lo, hi] {
                    if let Some(scalar) = bound.scalar() {
                        endpoints.push((*kind, scalar));
                    }
                }
            }
            Self::Union(members) | Self::Intersection(members) => {
                for member in members {
                    member.collect_interval_endpoints(endpoints);
                }
            }
            Self::Collection { element, .. } => {
                element.collect_interval_endpoints(endpoints);
            }
            Self::Canonicalized { inner, .. } => {
                inner.collect_interval_endpoints(endpoints);
            }
            Self::Str { .. } | Self::Regex(_) | Self::Enumerated(_) => {}
        }
    }

    /// Decide membership of a scalar of carrier domain `kind` in this
    /// schema's denoted set, where the vocabulary is scalar-decidable.
    ///
    /// Returns `Some(true)`/`Some(false)` when every node consulted
    /// can decide, and `None` when the answer depends on a node
    /// outside the scalar fragment (strings, regexes, enumerations,
    /// collections) or on an interval of a different kind. Float
    /// intervals compare by IEEE-754 semantics — the same comparison
    /// `refine` impls use — so `NaN` is a member of no interval.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
    ///
    /// let percent = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    ///
    /// // Decided: in and out of range.
    /// assert_eq!(
    ///     percent.scalar_membership(ScalarKind::Integer, &Scalar::Int(50)),
    ///     Some(true),
    /// );
    /// assert_eq!(
    ///     percent.scalar_membership(ScalarKind::Integer, &Scalar::Int(101)),
    ///     Some(false),
    /// );
    ///
    /// // Undecidable: wrong carrier domain.
    /// assert_eq!(
    ///     percent.scalar_membership(ScalarKind::Date, &Scalar::Int(50)),
    ///     None,
    /// );
    /// ```
    #[must_use]
    pub fn scalar_membership(&self, kind: ScalarKind, value: &Scalar) -> Option<bool> {
        match self {
            Self::Interval {
                kind: interval_kind,
                lo,
                hi,
            } => {
                if *interval_kind != kind {
                    return None;
                }
                Some(bound_admits_below(*lo, value) && bound_admits_above(*hi, value))
            }
            Self::Union(members) => {
                let answers: Vec<Option<bool>> = members
                    .iter()
                    .map(|member| member.scalar_membership(kind, value))
                    .collect();
                combine_membership(&answers, |decided| decided.contains(&true))
            }
            Self::Intersection(members) => {
                let answers: Vec<Option<bool>> = members
                    .iter()
                    .map(|member| member.scalar_membership(kind, value))
                    .collect();
                combine_membership(&answers, |decided| !decided.contains(&false))
            }
            Self::Canonicalized { inner, .. } => inner.scalar_membership(kind, value),
            Self::Str { .. } | Self::Regex(_) | Self::Enumerated(_) | Self::Collection { .. } => {
                None
            }
        }
    }

    /// Decide membership of a string in this schema's denoted set,
    /// where the vocabulary is string-decidable.
    ///
    /// Returns `Some(true)`/`Some(false)` when every node consulted
    /// can decide — [`Schema::Str`] nodes by length, alphabet, and
    /// first-character checks, [`Schema::Enumerated`] nodes by label
    /// lookup — and `None` when the answer depends on a node outside
    /// the string fragment. [`Schema::Regex`] is `None` by design:
    /// deciding it needs a regex engine, which the `no_std` kernel
    /// does not carry. [`Schema::Canonicalized`] decides membership
    /// of the CARRIED set (its inner schema), matching the
    /// [`SchemaRule`] denotation.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::Schema;
    ///
    /// let toggle = Schema::enumerated(&["on", "off"]);
    /// assert_eq!(toggle.string_membership("on"), Some(true));
    /// assert_eq!(toggle.string_membership("ON"), Some(false));
    ///
    /// // Regexes are undecidable without an engine.
    /// assert_eq!(Schema::regex("^on$").string_membership("on"), None);
    /// ```
    #[must_use]
    pub fn string_membership(&self, value: &str) -> Option<bool> {
        match self {
            Self::Str {
                len,
                unit,
                alphabet,
                first,
            } => Some(str_node_admits(
                *len,
                *unit,
                alphabet,
                first.as_ref(),
                value,
            )),
            Self::Enumerated(labels) => Some(labels.contains(&value)),
            Self::Union(members) => {
                let answers: Vec<Option<bool>> = members
                    .iter()
                    .map(|member| member.string_membership(value))
                    .collect();
                combine_membership(&answers, |decided| decided.contains(&true))
            }
            Self::Intersection(members) => {
                let answers: Vec<Option<bool>> = members
                    .iter()
                    .map(|member| member.string_membership(value))
                    .collect();
                combine_membership(&answers, |decided| !decided.contains(&false))
            }
            Self::Canonicalized { inner, .. } => inner.string_membership(value),
            Self::Interval { .. } | Self::Regex(_) | Self::Collection { .. } => None,
        }
    }

    /// The derived scalar boundary matrix: every finite interval
    /// endpoint together with its adjacent representable neighbours
    /// (`MIN−1`/`MIN`/`MIN+1`, `MAX−1`/`MAX`/`MAX+1`), each
    /// classified by the schema's own membership verdict.
    ///
    /// Neighbours respect the endpoint's regime: integer-kind values
    /// step by one and stop at the `i128` extremes (no candidate is
    /// emitted past them); float endpoints step to the next
    /// representable `f64` via [`f64::next_up`]/[`f64::next_down`]
    /// (at an infinity the step is the identity and the duplicate is
    /// removed). Candidates whose membership the schema cannot decide
    /// ([`Schema::scalar_membership`] returns `None`) are omitted —
    /// absence over a guessed verdict. The result is sorted and
    /// deduplicated.
    ///
    /// Float precision: neighbours are `f64`-ULP steps. A carrier
    /// narrower than `f64` (an `f32` rule) may not represent the
    /// neighbour exactly; consumers must skip candidates their
    /// carrier cannot embed losslessly (see
    /// `assert_schema_boundary_matrix` in `whittle_core::testing`).
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{
    ///     Bound, Scalar, ScalarBoundary, ScalarKind, Schema,
    /// };
    ///
    /// let percent = Schema::interval(
    ///     ScalarKind::Integer,
    ///     Bound::Inclusive(Scalar::Int(0)),
    ///     Bound::Inclusive(Scalar::Int(100)),
    /// );
    /// let boundary = |value: i128, admitted: bool| ScalarBoundary {
    ///     kind: ScalarKind::Integer,
    ///     value: Scalar::Int(value),
    ///     admitted,
    /// };
    /// assert_eq!(
    ///     percent.scalar_boundaries(),
    ///     [
    ///         boundary(-1, false),
    ///         boundary(0, true),
    ///         boundary(1, true),
    ///         boundary(99, true),
    ///         boundary(100, true),
    ///         boundary(101, false),
    ///     ],
    /// );
    /// ```
    #[must_use]
    pub fn scalar_boundaries(&self) -> Vec<ScalarBoundary> {
        let mut candidates: Vec<(ScalarKind, Scalar)> = Vec::new();
        for &(kind, scalar) in &self.interval_endpoints() {
            for candidate in [scalar_pred(scalar), Some(scalar), scalar_succ(scalar)]
                .into_iter()
                .flatten()
            {
                candidates.push((kind, candidate));
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .filter_map(|(kind, value)| {
                let admitted = self.scalar_membership(kind, &value)?;
                Some(ScalarBoundary {
                    kind,
                    value,
                    admitted,
                })
            })
            .collect()
    }

    /// The derived string boundary matrix, classified by the
    /// schema's own membership verdict:
    ///
    /// - [`Schema::Enumerated`] nodes contribute their labels plus
    ///   derived near-misses (case-flips, truncations, one-character
    ///   extensions, the empty string — the same derivation
    ///   [`crate::closed_set::rejects`] uses);
    /// - [`Schema::Str`] nodes contribute the empty string, length
    ///   edges (`MIN−1`/`MIN`/`MIN+1`/`MAX`/`MAX+1`, capped at
    ///   [`STR_BOUNDARY_LEN_CAP`] units — an uncapped `u64::MAX`
    ///   bound yields no candidate), an alphabet near-miss (an
    ///   in-bounds string whose last character falls outside the
    ///   alphabet, when an outsider exists), and a first-character
    ///   near-miss (an alphabet-admissible head outside the
    ///   first-character set, when the node carries one).
    ///
    /// Candidates whose membership the schema cannot decide
    /// ([`Schema::string_membership`] returns `None`) are omitted.
    /// [`Schema::Collection`] elements contribute nothing: their
    /// carrier is not a string, so element-level candidates have no
    /// string embedding at the root. The result is sorted and
    /// deduplicated.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::schema::{Schema, StringBoundary};
    ///
    /// let toggle = Schema::enumerated(&["on"]);
    /// let boundary = |value: &str, admitted: bool| StringBoundary {
    ///     value: value.into(),
    ///     admitted,
    /// };
    /// assert_eq!(
    ///     toggle.string_boundaries(),
    ///     [
    ///         boundary("", false),
    ///         boundary("ON", false),
    ///         boundary("o", false),
    ///         boundary("on", true),
    ///         boundary("onx", false),
    ///     ],
    /// );
    /// ```
    #[must_use]
    pub fn string_boundaries(&self) -> Vec<StringBoundary> {
        let mut candidates: Vec<String> = Vec::new();
        self.collect_string_candidates(&mut candidates);
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .filter_map(|value| {
                let admitted = self.string_membership(&value)?;
                Some(StringBoundary { value, admitted })
            })
            .collect()
    }

    /// Collect raw (unclassified) string boundary candidates from
    /// every node; [`Schema::string_boundaries`] classifies them at
    /// the root.
    fn collect_string_candidates(&self, out: &mut Vec<String>) {
        match self {
            Self::Enumerated(labels) => {
                out.extend(labels.iter().map(|label| String::from(*label)));
                out.extend(crate::closed_set::near_miss_candidates(
                    labels.iter().copied(),
                ));
            }
            Self::Union(members) | Self::Intersection(members) => {
                for member in members {
                    member.collect_string_candidates(out);
                }
            }
            Self::Canonicalized { inner, .. } => inner.collect_string_candidates(out),
            Self::Str {
                len,
                alphabet,
                first,
                ..
            } => collect_str_candidates(*len, alphabet, first.as_ref(), out),
            // `Collection` elements have no string embedding at the
            // root (see the method docs); intervals and regexes
            // contribute nothing.
            Self::Regex(_) | Self::Interval { .. } | Self::Collection { .. } => {}
        }
    }
}

/// Length cap for generated [`Schema::Str`] boundary candidates:
/// length edges above it yield no candidate.
///
/// The unconstrained `u64::MAX` bound is the motivating case — a
/// megabyte-scale test string probes nothing a 4096-unit one does
/// not — while every practical fixed length (hex digests up to
/// SHA-512's 128 chars, bounded lines and labels) stays comfortably
/// inside.
pub const STR_BOUNDARY_LEN_CAP: u64 = 4096;

/// Collect one [`Schema::Str`] node's raw boundary candidates; the
/// root classifies them (see [`Schema::string_boundaries`]).
fn collect_str_candidates(
    len: LenBound,
    alphabet: &CharSet,
    first: Option<&CharSet>,
    out: &mut Vec<String>,
) {
    out.push(String::new());
    // Sorted canonical ranges: the first range's start is the set's
    // smallest member. When the unit counts bytes it is also ASCII
    // whenever ANY member is (smaller code point, fewer UTF-8
    // bytes), so length-targeted candidates land on the byte
    // boundary whenever the alphabet allows it; otherwise the probe
    // is off-target but still honestly classified.
    let filler = alphabet.ranges()[0].0;
    let head = first.map_or(filler, |set| set.ranges()[0].0);
    // Length edges, capped: an absent edge (MIN = 0's MIN−1, an
    // uncapped MAX's MAX+1) is skipped, not wrapped.
    let edges = [
        len.min.checked_sub(1),
        Some(len.min),
        len.min.checked_add(1),
        Some(len.max),
        len.max.checked_add(1),
    ];
    for target in edges.into_iter().flatten() {
        if target > STR_BOUNDARY_LEN_CAP {
            continue;
        }
        out.push(unit_candidate(head, filler, target));
    }
    // Near-misses ride on the shortest in-bounds (non-empty) length.
    let miss_len = len.min.clamp(1, STR_BOUNDARY_LEN_CAP);
    if len.min <= STR_BOUNDARY_LEN_CAP {
        // Alphabet near-miss: the LAST character falls outside the
        // alphabet, when an outsider exists.
        if let Some(outsider) = alphabet.complement_sample() {
            let mut candidate = unit_candidate(head, filler, miss_len);
            candidate.pop();
            candidate.push(outsider);
            out.push(candidate);
        }
        // First-character near-miss: a head inside the alphabet but
        // outside the first-character set, when one exists.
        if let Some(first_set) = first
            && let Some(outside_first) = alphabet.difference(first_set)
        {
            out.push(unit_candidate(
                outside_first.ranges()[0].0,
                filler,
                miss_len,
            ));
        }
    }
}

/// Build a candidate of `target` length units: `head` first, then
/// `filler` repeated. `target` is pre-capped by the caller, so the
/// usize conversion cannot fail on supported targets.
fn unit_candidate(head: char, filler: char, target: u64) -> String {
    let target = usize::try_from(target).expect("capped candidate lengths fit usize");
    let mut out = String::with_capacity(target);
    for index in 0..target {
        out.push(if index == 0 { head } else { filler });
    }
    out
}

/// The previous representable scalar in the endpoint's regime:
/// integer-regime values step down by one (`None` at `i128::MIN` —
/// the universe has nothing below it); floats step to the next
/// representable `f64` via [`f64::next_down`] (the identity at
/// negative infinity, deduplicated by the caller).
fn scalar_pred(scalar: Scalar) -> Option<Scalar> {
    match scalar {
        Scalar::Int(value) => value.checked_sub(1).map(Scalar::Int),
        Scalar::Float(value) => Some(Scalar::Float(value.next_down())),
    }
}

/// The next representable scalar in the endpoint's regime; the
/// mirror of [`scalar_pred`] (`None` at `i128::MAX`, identity at
/// positive infinity).
fn scalar_succ(scalar: Scalar) -> Option<Scalar> {
    match scalar {
        Scalar::Int(value) => value.checked_add(1).map(Scalar::Int),
        Scalar::Float(value) => Some(Scalar::Float(value.next_up())),
    }
}

/// Decide a [`Schema::Str`] node's membership for one string: the
/// length (measured in the node's unit) must fall in the bound,
/// every character must be in the alphabet, and the first character
/// (when one exists) must be in the first-character set (when the
/// node carries one).
fn str_node_admits(
    len: LenBound,
    unit: LenUnit,
    alphabet: &CharSet,
    first: Option<&CharSet>,
    value: &str,
) -> bool {
    let measured = match unit {
        LenUnit::Chars => value.chars().count(),
        LenUnit::Bytes => value.len(),
    };
    let measured = u64::try_from(measured).expect("string lengths fit u64 on supported targets");
    if measured < len.min || measured > len.max {
        return false;
    }
    if !value.chars().all(|ch| alphabet.contains(ch)) {
        return false;
    }
    match (first, value.chars().next()) {
        (Some(set), Some(head)) => set.contains(head),
        (Some(_) | None, _) => true,
    }
}

/// `true` iff `value` satisfies the lower bound `lo` (IEEE semantics
/// for floats: `NaN` satisfies no finite bound).
fn bound_admits_below(lo: Bound, value: &Scalar) -> bool {
    lo.scalar().is_none_or(|scalar| {
        scalar
            .denotational_cmp(value)
            .is_some_and(core::cmp::Ordering::is_le)
    })
}

/// `true` iff `value` satisfies the upper bound `hi`.
fn bound_admits_above(hi: Bound, value: &Scalar) -> bool {
    hi.scalar().is_none_or(|scalar| {
        scalar
            .denotational_cmp(value)
            .is_some_and(core::cmp::Ordering::is_ge)
    })
}

/// Combine member membership answers (scalar or string regime):
/// the outcome is decided by `decide` over the decided answers; any
/// undecided member that could change the outcome makes the whole
/// answer undecided.
fn combine_membership(answers: &[Option<bool>], decide: fn(&[bool]) -> bool) -> Option<bool> {
    let decided: Vec<bool> = answers.iter().copied().flatten().collect();
    let any_undecided = answers.iter().any(Option::is_none);
    let outcome = decide(&decided);
    // For a union, a decided `true` wins regardless of undecided
    // members; for an intersection, a decided `false` wins. In both
    // cases `decide` returns the dominating answer; otherwise an
    // undecided member leaves the question open.
    let dominated = outcome != decide(&[]);
    if any_undecided && !dominated {
        return None;
    }
    Some(outcome)
}

/// Canonicalise one bound's scalar (`-0.0` to `0.0`).
const fn canonical_bound(bound: Bound) -> Bound {
    match bound {
        Bound::Inclusive(scalar) => Bound::Inclusive(scalar.canonicalized()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

/// Reduce a decimal interval to the smallest scale that represents
/// both endpoints exactly: trailing zeros are stripped jointly from
/// every finite mantissa while the scale is positive. Non-decimal
/// kinds pass through unchanged.
#[expect(
    clippy::integer_division_remainder_used,
    reason = "scale reduction strips exact factors of ten: the divisibility check \
              precedes every division, so no remainder is ever discarded"
)]
fn reduce_decimal_scale(kind: ScalarKind, lo: Bound, hi: Bound) -> (ScalarKind, Bound, Bound) {
    let ScalarKind::Decimal { mut scale } = kind else {
        return (kind, lo, hi);
    };
    let mut mantissas: Vec<i128> = [lo, hi]
        .iter()
        .filter_map(|bound| {
            let scalar = bound.scalar()?;
            scalar.as_int()
        })
        .collect();
    while scale > 0 && mantissas.iter().all(|mantissa| mantissa % 10 == 0) {
        for mantissa in &mut mantissas {
            *mantissa /= 10;
        }
        scale -= 1;
    }
    let mut reduced = mantissas.into_iter();
    let rebuild = |bound: Bound, reduced: &mut alloc::vec::IntoIter<i128>| match bound {
        Bound::Inclusive(_) => Bound::Inclusive(Scalar::Int(
            reduced.next().expect("one mantissa per finite bound"),
        )),
        Bound::Unbounded => Bound::Unbounded,
    };
    let lo = rebuild(lo, &mut reduced);
    let hi = rebuild(hi, &mut reduced);
    (ScalarKind::Decimal { scale }, lo, hi)
}

/// Fuse two lower bounds: the larger wins (`Unbounded` is the
/// identity, negative infinity).
fn fuse_lo(a: Bound, b: Bound) -> Bound {
    match (a.scalar(), b.scalar()) {
        (Some(sa), Some(sb)) => {
            if sa.cmp(&sb).is_ge() {
                a
            } else {
                b
            }
        }
        (Some(_), None) => a,
        (None, _) => b,
    }
}

/// Fuse two upper bounds: the smaller wins (`Unbounded` is the
/// identity, positive infinity).
fn fuse_hi(a: Bound, b: Bound) -> Bound {
    match (a.scalar(), b.scalar()) {
        (Some(sa), Some(sb)) => {
            if sa.cmp(&sb).is_le() {
                a
            } else {
                b
            }
        }
        (Some(_), None) => a,
        (None, _) => b,
    }
}

/// A canonical n-ary node never holds a single member: collapse to
/// the member itself, otherwise wrap with `node`.
fn collapse_singleton(mut members: Vec<Schema>, node: fn(Vec<Schema>) -> Schema) -> Schema {
    if members.len() == 1 {
        members.remove(0)
    } else {
        node(members)
    }
}

// ─── Display rendering (the R-S2 carrier). ─────────────────────────
//
// One line per node level: leaf nodes render on their own line;
// composite nodes render a header line followed by each child
// indented two spaces deeper. The exact text is UNSTABLE across
// whittle versions (module docs) — render for humans, never parse.

/// Renders the schema tree, one line per node level, children
/// indented two spaces per depth. UNSTABLE: human-readable output
/// only, not a serialized form.
///
/// # Examples
///
/// ```
/// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema};
///
/// let below = Schema::interval(
///     ScalarKind::Integer,
///     Bound::Unbounded,
///     Bound::Inclusive(Scalar::Int(-1)),
/// );
/// let above = Schema::interval(
///     ScalarKind::Integer,
///     Bound::Inclusive(Scalar::Int(1)),
///     Bound::Unbounded,
/// );
/// let non_zero = Schema::union([below, above].into());
///
/// assert_eq!(
///     non_zero.to_string(),
///     "any of\n  int in ..=-1\n  int in 1..",
/// );
/// ```
impl core::fmt::Display for Schema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fmt_schema_at(self, f, 0)
    }
}

/// Render one node at `depth`, recursing into children one level
/// deeper.
fn fmt_schema_at(
    schema: &Schema,
    f: &mut core::fmt::Formatter<'_>,
    depth: usize,
) -> core::fmt::Result {
    for _ in 0..depth {
        f.write_str("  ")?;
    }
    match schema {
        Schema::Interval { kind, lo, hi } => fmt_interval(f, *kind, *lo, *hi),
        Schema::Str {
            len,
            unit,
            alphabet,
            first,
        } => {
            write!(
                f,
                "string(len {}..={} {unit}, chars {alphabet}",
                len.min, len.max,
            )?;
            if let Some(first) = first {
                write!(f, ", first {first}")?;
            }
            f.write_str(")")
        }
        Schema::Regex(pattern) => write!(f, "regex /{pattern}/"),
        Schema::Enumerated(labels) => {
            f.write_str("one of ")?;
            for (index, label) in labels.iter().enumerate() {
                if index > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "\"{}\"", label.escape_debug())?;
            }
            Ok(())
        }
        Schema::Collection {
            len,
            element,
            sorted,
            unique,
        } => {
            write!(f, "collection(len {}..={}", len.min, len.max)?;
            if *sorted {
                f.write_str(", sorted")?;
            }
            if *unique {
                f.write_str(", unique")?;
            }
            f.write_str(")\n")?;
            fmt_schema_at(element, f, depth + 1)
        }
        Schema::Union(members) => fmt_members(f, "any of", members, depth),
        Schema::Intersection(members) => fmt_members(f, "all of", members, depth),
        Schema::Canonicalized { morphism, inner } => {
            writeln!(f, "canonicalized by {morphism}")?;
            fmt_schema_at(inner, f, depth + 1)
        }
    }
}

/// Render a composite node: the header line, then each member on its
/// own line one level deeper.
fn fmt_members(
    f: &mut core::fmt::Formatter<'_>,
    header: &str,
    members: &[Schema],
    depth: usize,
) -> core::fmt::Result {
    f.write_str(header)?;
    for member in members {
        f.write_str("\n")?;
        fmt_schema_at(member, f, depth + 1)?;
    }
    Ok(())
}

/// Render an interval line: the kind label, then the endpoint range
/// in Rust range syntax (`0..=100`, `0..`, `..=100`, `..`).
fn fmt_interval(
    f: &mut core::fmt::Formatter<'_>,
    kind: ScalarKind,
    lo: Bound,
    hi: Bound,
) -> core::fmt::Result {
    match kind {
        ScalarKind::Integer => f.write_str("int")?,
        ScalarKind::Float => f.write_str("float")?,
        ScalarKind::Date => f.write_str("date(days from CE)")?,
        ScalarKind::DateTime => f.write_str("datetime(unix seconds)")?,
        ScalarKind::Decimal { .. } => f.write_str("decimal")?,
    }
    f.write_str(" in ")?;
    if let Bound::Inclusive(scalar) = lo {
        fmt_endpoint(f, kind, scalar)?;
    }
    f.write_str("..")?;
    if let Bound::Inclusive(scalar) = hi {
        f.write_str("=")?;
        fmt_endpoint(f, kind, scalar)?;
    }
    Ok(())
}

/// Render one endpoint: decimal mantissas render as the scaled value
/// (`5` at scale 1 renders `0.5`); everything else renders the
/// scalar's own number.
fn fmt_endpoint(
    f: &mut core::fmt::Formatter<'_>,
    kind: ScalarKind,
    scalar: Scalar,
) -> core::fmt::Result {
    match scalar {
        Scalar::Int(mantissa) => {
            if let ScalarKind::Decimal { scale } = kind {
                fmt_scaled_decimal(f, mantissa, scale)
            } else {
                write!(f, "{mantissa}")
            }
        }
        Scalar::Float(value) => write!(f, "{value}"),
    }
}

/// Render `mantissa / 10^scale` as a plain decimal numeral by
/// inserting the point into the digit string (no arithmetic that
/// could overflow for large scales).
fn fmt_scaled_decimal(
    f: &mut core::fmt::Formatter<'_>,
    mantissa: i128,
    scale: u8,
) -> core::fmt::Result {
    if scale == 0 {
        return write!(f, "{mantissa}");
    }
    if mantissa < 0 {
        f.write_str("-")?;
    }
    let digits = alloc::format!("{}", mantissa.unsigned_abs());
    let scale = usize::from(scale);
    if digits.len() <= scale {
        write!(f, "0.{digits:0>scale$}")
    } else {
        let (int_part, frac_part) = digits.split_at(digits.len() - scale);
        write!(f, "{int_part}.{frac_part}")
    }
}

impl core::fmt::Display for LenUnit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Chars => f.write_str("chars"),
            Self::Bytes => f.write_str("bytes"),
        }
    }
}

impl core::fmt::Display for Morphism {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Trim => f.write_str("trim"),
            Self::AsciiLowercase => f.write_str("ascii-lowercase"),
            Self::AsciiUppercase => f.write_str("ascii-uppercase"),
        }
    }
}

/// Renders the canonical ranges as a bracketed list: singleton ranges
/// as one character, wider ranges as `'lo'-'hi'`, characters escaped
/// for printability.
///
/// # Examples
///
/// ```
/// use whittle_core::schema::CharSet;
///
/// let ident = CharSet::from_ranges([('a', 'z'), ('_', '_')]);
/// assert_eq!(ident.to_string(), "['_', 'a'-'z']");
/// ```
impl core::fmt::Display for CharSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("[")?;
        for (index, &(lo, hi)) in self.ranges.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "'{}'", lo.escape_debug())?;
            if lo != hi {
                write!(f, "-'{}'", hi.escape_debug())?;
            }
        }
        f.write_str("]")
    }
}

/// A rule whose admitted set has a constructive [`Schema`]
/// description.
///
/// # Soundness obligation
///
/// `⟦Self::schema()⟧ = range(Self::refine)` — the schema denotes the
/// post-canonicalisation CARRIED set (the values a
/// [`Refined`](crate::Refined) can hold), not the accepted raw-input
/// preimage. The two readings coincide for pure predicates, whose
/// `refine` is the identity on admissible input (IDEA §5.12
/// idempotence); for canonicalising rules the
/// [`Schema::Canonicalized`] node's inner schema denotes the carried
/// set and the recorded [`Morphism`] describes how raw input reaches
/// it (the accepted preimage is the morphism's preimage of the inner
/// set).
///
/// The schema is interpreted *within the carrier's embedding* into
/// the scalar universe: `⟦schema()⟧ ∩ ⟦T⟧ = range(refine)`. A bound
/// wider than `T`'s own range (an `AtMost<300>` carried by `u8`)
/// still describes the admitted set exactly, because the values
/// outside `T` are outside the embedding.
///
/// Like [`Rule::refine`]'s own soundness obligation, implementers
/// discharge this by reading the SAME const generics `refine` reads;
/// the cross-check helpers in [`crate::testing`] are the mechanical
/// oracle.
///
/// # Absence is meaningful
///
/// A rule without a `SchemaRule` impl has no schema — there is no
/// `Opaque` node. Hand-written `refine` logic stays visibly distinct
/// (IDEA §5.10), and composite rules can only have schemas when every
/// operand does.
///
/// # Examples
///
/// ```
/// use whittle_core::Rule;
/// use whittle_core::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};
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
/// impl SchemaRule<i32> for NonNeg {
///     fn schema() -> Schema {
///         Schema::interval(
///             ScalarKind::Integer,
///             Bound::Inclusive(Scalar::Int(0)),
///             Bound::Unbounded,
///         )
///     }
/// }
///
/// // The schema decides membership the same way `refine` does.
/// let schema = <NonNeg as SchemaRule<i32>>::schema();
/// assert_eq!(
///     schema.scalar_membership(ScalarKind::Integer, &Scalar::Int(7)),
///     Some(true),
/// );
/// assert_eq!(
///     schema.scalar_membership(ScalarKind::Integer, &Scalar::Int(-1)),
///     Some(false),
/// );
/// ```
pub trait SchemaRule<T>: Rule<T>
where
    T: 'static,
{
    /// The constructive description of this rule's admitted set.
    ///
    /// See the trait docs for the soundness obligation relating the
    /// returned schema to [`Rule::refine`].
    fn schema() -> Schema;
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::ToString as _;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::{Bound, CharSet, LenBound, LenUnit, Morphism, Scalar, ScalarKind, Schema};

    fn int_interval(lo: i128, hi: i128) -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(lo)),
            Bound::Inclusive(Scalar::Int(hi)),
        )
    }

    // ─── Scalar: structural order vs denotational comparison. ─────

    #[test]
    fn scalar_orders_ints_by_value_and_floats_by_total_order() {
        assert!(Scalar::Int(1) < Scalar::Int(2));
        assert!(Scalar::Float(-0.5) < Scalar::Float(0.5));
        assert!(Scalar::Float(f64::NEG_INFINITY) < Scalar::Float(f64::MIN));
        assert!(Scalar::Float(f64::MAX) < Scalar::Float(f64::INFINITY));
        // NaN sits above +inf in the total order.
        assert!(Scalar::Float(f64::INFINITY) < Scalar::Float(f64::NAN));
    }

    #[test]
    fn scalar_orders_across_variants_structurally() {
        assert!(Scalar::Int(i128::MAX) < Scalar::Float(f64::NEG_INFINITY));
        assert!(Scalar::Float(0.0) > Scalar::Int(0));
        assert_ne!(Scalar::Int(0), Scalar::Float(0.0));
    }

    #[test]
    fn scalar_eq_follows_the_total_order() {
        assert_eq!(Scalar::Int(7), Scalar::Int(7));
        assert_eq!(Scalar::Float(0.5), Scalar::Float(0.5));
        // total_cmp equality: NaN equals NaN structurally.
        assert_eq!(Scalar::Float(f64::NAN), Scalar::Float(f64::NAN));
        // partial_cmp (PartialOrd) routes through the total order.
        assert_eq!(
            Scalar::Int(1).partial_cmp(&Scalar::Int(2)),
            Some(core::cmp::Ordering::Less),
        );
    }

    #[test]
    fn scalar_accessors_select_the_matching_variant() {
        assert_eq!(Scalar::Int(3).as_int(), Some(3));
        assert_eq!(Scalar::Float(3.0).as_int(), None);
        assert_eq!(Scalar::Float(3.0).as_float(), Some(3.0));
        assert_eq!(Scalar::Int(3).as_float(), None);
    }

    // ─── Interval canonical invariants. ───────────────────────────

    #[test]
    fn interval_accepts_degenerate_singleton() {
        let point = int_interval(42, 42);
        assert_eq!(
            point,
            Schema::Interval {
                kind: ScalarKind::Integer,
                lo: Bound::Inclusive(Scalar::Int(42)),
                hi: Bound::Inclusive(Scalar::Int(42)),
            },
        );
    }

    #[test]
    fn interval_accepts_unbounded_ends() {
        let everything = Schema::interval(ScalarKind::Integer, Bound::Unbounded, Bound::Unbounded);
        assert_eq!(
            everything.scalar_membership(ScalarKind::Integer, &Scalar::Int(i128::MIN)),
            Some(true),
        );
    }

    #[test]
    #[should_panic(expected = "lo must be <= hi")]
    fn interval_rejects_empty_range() {
        let _schema = int_interval(1, 0);
    }

    #[test]
    #[should_panic(expected = "endpoint scalar variant must match")]
    fn interval_rejects_regime_mismatch() {
        let _schema = Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Unbounded,
        );
    }

    #[test]
    #[should_panic(expected = "endpoint scalar variant must match")]
    fn interval_rejects_int_endpoint_for_float_kind() {
        let _schema = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Unbounded,
        );
    }

    #[test]
    #[should_panic(expected = "NaN is not an admissible endpoint")]
    fn interval_rejects_nan_endpoint() {
        let _schema = Schema::interval(
            ScalarKind::Float,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Float(f64::NAN)),
        );
    }

    #[test]
    fn interval_normalizes_negative_zero_endpoint() {
        let negative = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(-0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        let positive = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        assert_eq!(negative, positive);
    }

    #[test]
    fn interval_reduces_decimal_scale_jointly() {
        let wide = Schema::interval(
            ScalarKind::Decimal { scale: 2 },
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Inclusive(Scalar::Int(10_000)),
        );
        let narrow = Schema::interval(
            ScalarKind::Decimal { scale: 0 },
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Inclusive(Scalar::Int(100)),
        );
        assert_eq!(wide, narrow);
    }

    #[test]
    fn interval_keeps_irreducible_decimal_scale() {
        // 0.5..=2.5 at scale 1: 5 and 25 are not both divisible by
        // 10, so the scale stays.
        let interval = Schema::interval(
            ScalarKind::Decimal { scale: 1 },
            Bound::Inclusive(Scalar::Int(5)),
            Bound::Inclusive(Scalar::Int(25)),
        );
        assert_eq!(
            interval,
            Schema::Interval {
                kind: ScalarKind::Decimal { scale: 1 },
                lo: Bound::Inclusive(Scalar::Int(5)),
                hi: Bound::Inclusive(Scalar::Int(25)),
            },
        );
    }

    #[test]
    fn interval_reduces_decimal_scale_with_unbounded_end() {
        let half_open = Schema::interval(
            ScalarKind::Decimal { scale: 2 },
            Bound::Inclusive(Scalar::Int(100)),
            Bound::Unbounded,
        );
        let reduced = Schema::interval(
            ScalarKind::Decimal { scale: 0 },
            Bound::Inclusive(Scalar::Int(1)),
            Bound::Unbounded,
        );
        assert_eq!(half_open, reduced);
    }

    #[test]
    fn interval_reduces_fully_unbounded_decimal_to_scale_zero() {
        let any_scale = Schema::interval(
            ScalarKind::Decimal { scale: 7 },
            Bound::Unbounded,
            Bound::Unbounded,
        );
        let zero_scale = Schema::interval(
            ScalarKind::Decimal { scale: 0 },
            Bound::Unbounded,
            Bound::Unbounded,
        );
        assert_eq!(any_scale, zero_scale);
    }

    // ─── LenBound / CharSet / leaf constructors. ───────────────────

    #[test]
    fn len_bound_accepts_degenerate_and_ordered_ranges() {
        assert_eq!(LenBound::new(0, 0), LenBound { min: 0, max: 0 });
        assert_eq!(LenBound::new(1, 64), LenBound { min: 1, max: 64 });
    }

    #[test]
    #[should_panic(expected = "min must be <= max")]
    fn len_bound_rejects_inverted_range() {
        let _len = LenBound::new(2, 1);
    }

    #[test]
    fn char_set_merges_overlapping_and_adjacent_ranges() {
        let merged = CharSet::from_ranges([('n', 'z'), ('a', 'p'), ('0', '4'), ('5', '9')]);
        assert_eq!(merged.ranges(), &[('0', '9'), ('a', 'z')]);
    }

    #[test]
    fn char_set_keeps_disjoint_ranges_sorted() {
        let set = CharSet::from_ranges([('x', 'z'), ('a', 'c')]);
        assert_eq!(set.ranges(), &[('a', 'c'), ('x', 'z')]);
    }

    #[test]
    fn char_set_merges_across_the_surrogate_gap() {
        // U+D7FF's successor is U+E000: the two ranges are adjacent.
        let set = CharSet::from_ranges([('\u{0}', '\u{D7FF}'), ('\u{E000}', char::MAX)]);
        assert_eq!(set.ranges(), &[('\u{0}', char::MAX)]);
    }

    #[test]
    fn char_set_saturates_at_char_max() {
        // char::MAX's successor saturates, so a range ending at
        // char::MAX merges with anything starting at or below it.
        let set = CharSet::from_ranges([('\u{E000}', char::MAX), (char::MAX, char::MAX)]);
        assert_eq!(set.ranges(), &[('\u{E000}', char::MAX)]);
    }

    #[test]
    #[should_panic(expected = "every range must satisfy lo <= hi")]
    fn char_set_rejects_inverted_range() {
        let _set = CharSet::from_ranges([('z', 'a')]);
    }

    #[test]
    #[should_panic(expected = "at least one range is required")]
    fn char_set_rejects_empty_set() {
        let _set = CharSet::from_ranges([]);
    }

    #[test]
    fn enumerated_keeps_declaration_order() {
        let schema = Schema::enumerated(&["zulu", "alpha"]);
        assert_eq!(schema.as_enumerated(), Some(&["zulu", "alpha"][..]));
    }

    #[test]
    #[should_panic(expected = "at least one label is required")]
    fn enumerated_rejects_empty_label_set() {
        let _schema = Schema::enumerated(&[]);
    }

    #[test]
    #[should_panic(expected = "labels must be duplicate-free")]
    fn enumerated_rejects_duplicate_labels() {
        let _schema = Schema::enumerated(&["same", "same"]);
    }

    #[test]
    fn string_and_collection_and_canonicalized_round_trip_structurally() {
        let ident = Schema::string(
            LenBound::new(1, 64),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'z')]),
            Some(CharSet::from_ranges([('_', '_')])),
        );
        assert_eq!(
            ident,
            Schema::Str {
                len: LenBound::new(1, 64),
                unit: LenUnit::Chars,
                alphabet: CharSet::from_ranges([('a', 'z')]),
                first: Some(CharSet::from_ranges([('_', '_')])),
            },
        );
        let list = Schema::collection(LenBound::new(0, 8), ident.clone(), false, true);
        assert_eq!(
            list,
            Schema::Collection {
                len: LenBound::new(0, 8),
                element: Box::new(ident),
                sorted: false,
                unique: true,
            },
        );
        let trimmed = Schema::canonicalized(Morphism::Trim, list.clone());
        assert_eq!(
            trimmed,
            Schema::Canonicalized {
                morphism: Morphism::Trim,
                inner: Box::new(list),
            },
        );
        // Support types are ordered for canonical sorting.
        assert!(LenUnit::Chars < LenUnit::Bytes);
        assert!(Morphism::Trim < Morphism::AsciiLowercase);
        assert_eq!(Schema::regex("^a$"), Schema::Regex("^a$"));
    }

    // ─── Union canonical invariants. ───────────────────────────────

    #[test]
    fn union_flattens_sorts_and_dedupes() {
        let a = int_interval(0, 1);
        let b = int_interval(5, 9);
        let c = int_interval(20, 30);
        let nested = Schema::union(vec![
            Schema::union(vec![b.clone(), a.clone()]),
            c.clone(),
            a.clone(),
        ]);
        let flat = Schema::union(vec![a, b, c]);
        assert_eq!(nested, flat);
        // The canonical form is the sorted, deduplicated member list.
        assert_eq!(
            flat,
            Schema::Union(vec![
                int_interval(0, 1),
                int_interval(5, 9),
                int_interval(20, 30),
            ]),
        );
    }

    #[test]
    fn union_collapses_singleton_to_member() {
        let only = int_interval(0, 9);
        assert_eq!(Schema::union(vec![only.clone()]), only);
    }

    #[test]
    fn union_collapses_duplicates_to_member() {
        let only = int_interval(0, 9);
        assert_eq!(Schema::union(vec![only.clone(), only.clone()]), only);
    }

    #[test]
    #[should_panic(expected = "at least one member is required")]
    fn union_rejects_empty_member_list() {
        let _schema = Schema::union(Vec::new());
    }

    // ─── Intersection canonical invariants. ────────────────────────

    #[test]
    fn intersection_fuses_same_kind_intervals() {
        let at_least = Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Unbounded,
        );
        let at_most = Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(100)),
        );
        assert_eq!(
            Schema::intersection(vec![at_least, at_most]),
            int_interval(0, 100),
        );
    }

    #[test]
    fn intersection_fuses_same_kind_intervals_in_either_member_order() {
        // Covers all four fusion arms: a finite bound meeting an
        // unbounded one in both argument positions.
        let at_least = Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Unbounded,
        );
        let at_most = Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(100)),
        );
        assert_eq!(
            Schema::intersection(vec![at_most.clone(), at_least.clone()]),
            int_interval(0, 100),
        );
        assert_eq!(
            Schema::intersection(vec![at_least, at_most]),
            int_interval(0, 100),
        );
    }

    #[test]
    fn intersection_fusion_picks_tightest_bounds() {
        let fused = Schema::intersection(vec![int_interval(0, 100), int_interval(10, 200)]);
        assert_eq!(fused, int_interval(10, 100));
    }

    #[test]
    fn intersection_flattens_nested_symbolic_intersections() {
        // A mixed-kind intersection stays symbolic, so nesting it
        // inside another intersection exercises the flattening path.
        let int = int_interval(0, 100);
        let float = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        let date = Schema::interval(
            ScalarKind::Date,
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Inclusive(Scalar::Int(9)),
        );
        let nested = Schema::intersection(vec![
            Schema::intersection(vec![int.clone(), float.clone()]),
            date.clone(),
        ]);
        assert_eq!(nested, Schema::intersection(vec![int, float, date]));
    }

    #[test]
    fn intersection_keeps_different_kind_intervals_symbolic() {
        let int = int_interval(0, 100);
        let float = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        let mixed = Schema::intersection(vec![int.clone(), float.clone()]);
        // The canonical residual form is the sorted symbolic pair.
        assert_eq!(
            mixed,
            Schema::Intersection(vec![int.clone(), float.clone()]),
        );
        // Construction order is irrelevant.
        assert_eq!(mixed, Schema::intersection(vec![float, int]));
    }

    #[test]
    fn intersection_keeps_non_interval_members_symbolic() {
        let interval = int_interval(0, 9);
        let regex = Schema::regex("^[0-9]$");
        let mixed = Schema::intersection(vec![regex.clone(), interval.clone()]);
        assert_eq!(mixed, Schema::intersection(vec![interval, regex]));
    }

    #[test]
    fn intersection_collapses_singleton_to_member() {
        let only = int_interval(0, 9);
        assert_eq!(Schema::intersection(vec![only.clone()]), only);
    }

    #[test]
    fn intersection_fuses_decimal_intervals_and_re_reduces_scale() {
        // [1.0, 99.9] ∩ [0.3, 2.0] at scale 1 → [1.0, 2.0] → scale 0.
        let a = Schema::interval(
            ScalarKind::Decimal { scale: 1 },
            Bound::Inclusive(Scalar::Int(10)),
            Bound::Inclusive(Scalar::Int(999)),
        );
        let b = Schema::interval(
            ScalarKind::Decimal { scale: 1 },
            Bound::Inclusive(Scalar::Int(3)),
            Bound::Inclusive(Scalar::Int(20)),
        );
        let fused = Schema::intersection(vec![a, b]);
        assert_eq!(
            fused,
            Schema::interval(
                ScalarKind::Decimal { scale: 0 },
                Bound::Inclusive(Scalar::Int(1)),
                Bound::Inclusive(Scalar::Int(2)),
            ),
        );
    }

    #[test]
    #[should_panic(expected = "lo must be <= hi")]
    fn intersection_rejects_empty_fusion() {
        let _schema = Schema::intersection(vec![int_interval(5, 9), int_interval(0, 3)]);
    }

    #[test]
    #[should_panic(expected = "at least one member is required")]
    fn intersection_rejects_empty_member_list() {
        let _schema = Schema::intersection(Vec::new());
    }

    // ─── Confluence: normal form independent of construction order.

    proptest::proptest! {
        /// Union normal form is independent of member permutation and
        /// nesting split point.
        #[test]
        fn union_is_confluent(
            mut bounds in proptest::collection::vec((-100_i128..=100, 0_i128..=100), 2..6),
            split in 1_usize..5,
            permute in proptest::bool::ANY,
        ) {
            let members: Vec<Schema> = bounds
                .iter()
                .map(|&(lo, span)| int_interval(lo, lo + span))
                .collect();
            let flat = Schema::union(members.clone());

            // Nest at an arbitrary split point.
            let split = split.min(members.len() - 1);
            let (left, right) = members.split_at(split);
            let nested = Schema::union(vec![
                Schema::union(left.to_vec()),
                Schema::union(right.to_vec()),
            ]);
            proptest::prop_assert_eq!(&nested, &flat);

            // Permute (reverse is enough to change every position).
            if permute {
                bounds.reverse();
            }
            let permuted = Schema::union(
                bounds.iter().map(|&(lo, span)| int_interval(lo, lo + span)).collect(),
            );
            proptest::prop_assert_eq!(&permuted, &flat);
        }

        /// Intersection normal form is independent of member
        /// permutation and nesting split point. Every generated
        /// interval contains zero, so fusion never empties.
        #[test]
        fn intersection_is_confluent(
            bounds in proptest::collection::vec((-100_i128..=0, 0_i128..=100), 2..6),
            split in 1_usize..5,
        ) {
            let members: Vec<Schema> = bounds
                .iter()
                .map(|&(lo, hi)| int_interval(lo, hi))
                .collect();
            let flat = Schema::intersection(members.clone());

            let split = split.min(members.len() - 1);
            let (left, right) = members.split_at(split);
            let nested = Schema::intersection(vec![
                Schema::intersection(left.to_vec()),
                Schema::intersection(right.to_vec()),
            ]);
            proptest::prop_assert_eq!(&nested, &flat);

            let mut reversed = members;
            reversed.reverse();
            proptest::prop_assert_eq!(&Schema::intersection(reversed), &flat);
        }

        /// Fused intersections agree with the directly-constructed
        /// interval: `And<AtLeast<MIN>, AtMost<MAX>>` ≡
        /// `Within<MIN, MAX>` at the schema level.
        #[test]
        fn intersection_fusion_matches_direct_interval(
            lo in -100_i128..=0,
            hi in 0_i128..=100,
        ) {
            let at_least = Schema::interval(
                ScalarKind::Integer,
                Bound::Inclusive(Scalar::Int(lo)),
                Bound::Unbounded,
            );
            let at_most = Schema::interval(
                ScalarKind::Integer,
                Bound::Unbounded,
                Bound::Inclusive(Scalar::Int(hi)),
            );
            proptest::prop_assert_eq!(
                Schema::intersection(vec![at_least, at_most]),
                int_interval(lo, hi),
            );
        }

        /// CharSet normal form is independent of range order.
        #[test]
        fn char_set_is_confluent(
            mut offsets in proptest::collection::vec((0_u8..=12, 0_u8..=13), 1..5),
        ) {
            let to_range = |&(lo_offset, span): &(u8, u8)| {
                let lo_code = u32::from(b'a') + u32::from(lo_offset);
                let hi_code = (lo_code + u32::from(span)).min(u32::from(b'z'));
                (
                    char::from_u32(lo_code).unwrap(),
                    char::from_u32(hi_code).unwrap(),
                )
            };
            let forward = CharSet::from_ranges(offsets.iter().map(to_range));
            offsets.reverse();
            let backward = CharSet::from_ranges(offsets.iter().map(to_range));
            proptest::prop_assert_eq!(forward, backward);
        }
    }

    // ─── Membership and endpoint queries. ──────────────────────────

    #[test]
    fn membership_decides_int_interval_inclusively() {
        let percent = int_interval(0, 100);
        let kind = ScalarKind::Integer;
        assert_eq!(percent.scalar_membership(kind, &Scalar::Int(0)), Some(true));
        assert_eq!(
            percent.scalar_membership(kind, &Scalar::Int(100)),
            Some(true),
        );
        assert_eq!(
            percent.scalar_membership(kind, &Scalar::Int(-1)),
            Some(false),
        );
        assert_eq!(
            percent.scalar_membership(kind, &Scalar::Int(101)),
            Some(false),
        );
    }

    #[test]
    fn membership_uses_ieee_semantics_for_floats() {
        let unit = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        let kind = ScalarKind::Float;
        // -0.0 is IEEE-equal to the 0.0 endpoint: a member, exactly
        // as `refine`'s `(lo..=hi).contains` sees it.
        assert_eq!(
            unit.scalar_membership(kind, &Scalar::Float(-0.0)),
            Some(true),
        );
        // NaN is a member of no interval.
        assert_eq!(
            unit.scalar_membership(kind, &Scalar::Float(f64::NAN)),
            Some(false),
        );
        assert_eq!(
            unit.scalar_membership(kind, &Scalar::Float(1.5)),
            Some(false),
        );
    }

    #[test]
    fn membership_decides_false_for_regime_mismatched_value() {
        // The query kind matches the interval, but the value's scalar
        // variant is from the other regime: denotationally not a
        // member (the comparison itself is undefined, so no bound
        // admits it).
        let percent = int_interval(0, 100);
        assert_eq!(
            percent.scalar_membership(ScalarKind::Integer, &Scalar::Float(50.0)),
            Some(false),
        );
    }

    #[test]
    fn as_enumerated_is_none_for_other_nodes() {
        assert_eq!(int_interval(0, 1).as_enumerated(), None);
    }

    #[test]
    fn membership_is_undecided_for_kind_mismatch_and_non_scalar_nodes() {
        let percent = int_interval(0, 100);
        assert_eq!(
            percent.scalar_membership(ScalarKind::Date, &Scalar::Int(50)),
            None,
        );
        let regex = Schema::regex("^a$");
        assert_eq!(
            regex.scalar_membership(ScalarKind::Integer, &Scalar::Int(0)),
            None,
        );
        let string = Schema::string(
            LenBound::new(0, 9),
            LenUnit::Bytes,
            CharSet::from_ranges([('a', 'z')]),
            None,
        );
        assert_eq!(
            string.scalar_membership(ScalarKind::Integer, &Scalar::Int(0)),
            None,
        );
        let enumerated = Schema::enumerated(&["on"]);
        assert_eq!(
            enumerated.scalar_membership(ScalarKind::Integer, &Scalar::Int(0)),
            None,
        );
        let collection = Schema::collection(LenBound::new(0, 1), percent, false, false);
        assert_eq!(
            collection.scalar_membership(ScalarKind::Integer, &Scalar::Int(50)),
            None,
        );
    }

    #[test]
    fn membership_folds_unions_with_dominating_true() {
        let kind = ScalarKind::Integer;
        let union = Schema::union(vec![int_interval(0, 9), int_interval(20, 30)]);
        assert_eq!(union.scalar_membership(kind, &Scalar::Int(5)), Some(true));
        assert_eq!(union.scalar_membership(kind, &Scalar::Int(15)), Some(false),);

        // A decided `true` dominates an undecided sibling; a decided
        // `false` does not.
        let with_regex = Schema::union(vec![int_interval(0, 9), Schema::regex("^a$")]);
        assert_eq!(
            with_regex.scalar_membership(kind, &Scalar::Int(5)),
            Some(true),
        );
        assert_eq!(with_regex.scalar_membership(kind, &Scalar::Int(15)), None);
    }

    #[test]
    fn membership_folds_intersections_with_dominating_false() {
        let kind = ScalarKind::Integer;
        // Same-kind intervals fuse, so mix kinds to keep a symbolic
        // intersection with decidable integer members.
        let date = Schema::interval(
            ScalarKind::Date,
            Bound::Inclusive(Scalar::Int(0)),
            Bound::Inclusive(Scalar::Int(9)),
        );
        let mixed = Schema::intersection(vec![int_interval(0, 9), date]);
        // The integer member decides false: dominating.
        assert_eq!(mixed.scalar_membership(kind, &Scalar::Int(15)), Some(false),);
        // The integer member decides true, the date member is
        // undecided for an Integer query: open.
        assert_eq!(mixed.scalar_membership(kind, &Scalar::Int(5)), None);
    }

    #[test]
    fn membership_recurses_through_canonicalized() {
        let kind = ScalarKind::Integer;
        let trimmed = Schema::canonicalized(Morphism::Trim, int_interval(0, 9));
        assert_eq!(trimmed.scalar_membership(kind, &Scalar::Int(5)), Some(true));
        assert_eq!(
            trimmed.scalar_membership(kind, &Scalar::Int(10)),
            Some(false),
        );
    }

    #[test]
    fn interval_endpoints_collects_finite_ends_recursively() {
        let lo_only = Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(1)),
            Bound::Unbounded,
        );
        let union = Schema::union(vec![lo_only, int_interval(5, 9)]);
        let date = Schema::interval(
            ScalarKind::Date,
            Bound::Inclusive(Scalar::Int(700_000)),
            Bound::Inclusive(Scalar::Int(800_000)),
        );
        let tree = Schema::canonicalized(
            Morphism::AsciiLowercase,
            Schema::intersection(vec![
                union,
                date,
                Schema::collection(LenBound::new(0, 3), int_interval(0, 255), false, false),
                Schema::regex("^x$"),
            ]),
        );
        let mut endpoints = tree.interval_endpoints();
        endpoints.sort_unstable();
        assert_eq!(
            endpoints,
            [
                (ScalarKind::Integer, Scalar::Int(0)),
                (ScalarKind::Integer, Scalar::Int(1)),
                (ScalarKind::Integer, Scalar::Int(5)),
                (ScalarKind::Integer, Scalar::Int(9)),
                (ScalarKind::Integer, Scalar::Int(255)),
                (ScalarKind::Date, Scalar::Int(700_000)),
                (ScalarKind::Date, Scalar::Int(800_000)),
            ],
        );
        // Non-interval leaves contribute nothing.
        assert!(Schema::enumerated(&["on"]).interval_endpoints().is_empty());
        assert!(
            Schema::string(
                LenBound::new(0, 1),
                LenUnit::Chars,
                CharSet::from_ranges([('a', 'a')]),
                None,
            )
            .interval_endpoints()
            .is_empty()
        );
    }

    // ─── Boundary folds: derived test matrices. ────────────────────

    use super::{ScalarBoundary, StringBoundary};
    use alloc::string::String;

    fn int_boundary(value: i128, admitted: bool) -> ScalarBoundary {
        ScalarBoundary {
            kind: ScalarKind::Integer,
            value: Scalar::Int(value),
            admitted,
        }
    }

    fn string_boundary(value: &str, admitted: bool) -> StringBoundary {
        StringBoundary {
            value: String::from(value),
            admitted,
        }
    }

    #[test]
    fn scalar_boundaries_classify_endpoints_and_neighbours() {
        let percent = int_interval(0, 100);
        assert_eq!(
            percent.scalar_boundaries(),
            [
                int_boundary(-1, false),
                int_boundary(0, true),
                int_boundary(1, true),
                int_boundary(99, true),
                int_boundary(100, true),
                int_boundary(101, false),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_stop_at_the_i128_extremes() {
        // No candidate exists below i128::MIN or above i128::MAX:
        // the predecessor/successor folds skip rather than wrap.
        let everything = int_interval(i128::MIN, i128::MAX);
        assert_eq!(
            everything.scalar_boundaries(),
            [
                int_boundary(i128::MIN, true),
                int_boundary(i128::MIN + 1, true),
                int_boundary(i128::MAX - 1, true),
                int_boundary(i128::MAX, true),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_step_floats_by_one_ulp() {
        let unit = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(0.0)),
            Bound::Inclusive(Scalar::Float(1.0)),
        );
        let float_boundary = |value: f64, admitted: bool| ScalarBoundary {
            kind: ScalarKind::Float,
            value: Scalar::Float(value),
            admitted,
        };
        assert_eq!(
            unit.scalar_boundaries(),
            [
                float_boundary(0.0_f64.next_down(), false),
                float_boundary(0.0, true),
                float_boundary(0.0_f64.next_up(), true),
                float_boundary(1.0_f64.next_down(), true),
                float_boundary(1.0, true),
                float_boundary(1.0_f64.next_up(), false),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_dedup_the_infinite_float_ends() {
        // next_down(-inf) and next_up(+inf) are the identities: the
        // duplicates collapse, leaving the infinities and their
        // finite neighbours.
        let not_nan = Schema::interval(
            ScalarKind::Float,
            Bound::Inclusive(Scalar::Float(f64::NEG_INFINITY)),
            Bound::Inclusive(Scalar::Float(f64::INFINITY)),
        );
        let float_boundary = |value: f64, admitted: bool| ScalarBoundary {
            kind: ScalarKind::Float,
            value: Scalar::Float(value),
            admitted,
        };
        assert_eq!(
            not_nan.scalar_boundaries(),
            [
                float_boundary(f64::NEG_INFINITY, true),
                float_boundary(f64::MIN, true),
                float_boundary(f64::MAX, true),
                float_boundary(f64::INFINITY, true),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_classify_the_union_gap() {
        // NonZero's shape: the gap point 0 is derived from both
        // endpoints (successor of -1, predecessor of 1) and
        // classified as a reject.
        let below = Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(-1)),
        );
        let above = Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(1)),
            Bound::Unbounded,
        );
        let non_zero = Schema::union(vec![below, above]);
        assert_eq!(
            non_zero.scalar_boundaries(),
            [
                int_boundary(-2, true),
                int_boundary(-1, true),
                int_boundary(0, false),
                int_boundary(1, true),
                int_boundary(2, true),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_skip_union_candidates_only_a_true_can_decide() {
        // Mixed-kind union: a reject verdict would need every member
        // decided, but the Date member cannot decide Integer
        // candidates (and vice versa), so only the dominating
        // accepts survive the fold.
        let mixed = Schema::union(vec![
            int_interval(0, 10),
            Schema::interval(
                ScalarKind::Date,
                Bound::Inclusive(Scalar::Int(20)),
                Bound::Inclusive(Scalar::Int(30)),
            ),
        ]);
        let date_boundary = |value: i128, admitted: bool| ScalarBoundary {
            kind: ScalarKind::Date,
            value: Scalar::Int(value),
            admitted,
        };
        assert_eq!(
            mixed.scalar_boundaries(),
            [
                int_boundary(0, true),
                int_boundary(1, true),
                int_boundary(9, true),
                int_boundary(10, true),
                date_boundary(20, true),
                date_boundary(21, true),
                date_boundary(29, true),
                date_boundary(30, true),
            ],
        );
    }

    #[test]
    fn scalar_boundaries_skip_intersection_candidates_only_a_false_can_decide() {
        // The intersection mirror: accepts would need every member
        // decided, so only the dominating rejects survive.
        let mixed = Schema::intersection(vec![
            int_interval(0, 10),
            Schema::interval(
                ScalarKind::Date,
                Bound::Inclusive(Scalar::Int(20)),
                Bound::Inclusive(Scalar::Int(30)),
            ),
        ]);
        let date_boundary = |value: i128, admitted: bool| ScalarBoundary {
            kind: ScalarKind::Date,
            value: Scalar::Int(value),
            admitted,
        };
        assert_eq!(
            mixed.scalar_boundaries(),
            [
                int_boundary(-1, false),
                int_boundary(11, false),
                date_boundary(19, false),
                date_boundary(31, false),
            ],
        );
    }

    #[test]
    fn char_set_contains_decides_by_canonical_ranges() {
        let ident = CharSet::from_ranges([('a', 'z'), ('_', '_')]);
        assert!(ident.contains('a'));
        assert!(ident.contains('z'));
        assert!(ident.contains('_'));
        assert!(!ident.contains('A'));
        assert!(!ident.contains('-'));
    }

    #[test]
    fn char_set_difference_cuts_holes_and_trims_edges() {
        let letters = CharSet::from_ranges([('a', 'z')]);
        // Interior holes: each subtracted point splits the run.
        let holes = CharSet::from_ranges([('e', 'e'), ('p', 'q')]);
        assert_eq!(
            letters.difference(&holes).expect("non-empty"),
            CharSet::from_ranges([('a', 'd'), ('f', 'o'), ('r', 'z')]),
        );
        // Edge trims: overlapping the run's ends shrinks it.
        let edges = CharSet::from_ranges([('W', 'c'), ('x', '~')]);
        assert_eq!(
            letters.difference(&edges).expect("non-empty"),
            CharSet::from_ranges([('d', 'w')]),
        );
        // Disjoint subtraction is the identity.
        let digits = CharSet::from_ranges([('0', '9')]);
        assert_eq!(letters.difference(&digits), Some(letters.clone()));
        // A covering subtraction leaves nothing.
        assert_eq!(
            letters.difference(&CharSet::from_ranges([('A', 'z')])),
            None
        );
        // Self-subtraction leaves nothing.
        assert_eq!(letters.difference(&letters), None);
    }

    #[test]
    fn char_set_difference_skips_the_surrogate_gap() {
        // Cutting a hole right at the gap edges exercises the
        // predecessor/successor mirrors across U+D7FF / U+E000.
        let wide = CharSet::from_ranges([('\u{D000}', '\u{F000}')]);
        let hole = CharSet::from_ranges([('\u{E000}', '\u{E010}')]);
        assert_eq!(
            wide.difference(&hole).expect("non-empty"),
            CharSet::from_ranges([('\u{D000}', '\u{D7FF}'), ('\u{E011}', '\u{F000}')]),
        );
        // The mirrored cut: subtracting up to the gap's low edge.
        let low_cut = CharSet::from_ranges([('\u{D000}', '\u{D7FF}')]);
        assert_eq!(
            wide.difference(&low_cut).expect("non-empty"),
            CharSet::from_ranges([('\u{E000}', '\u{F000}')]),
        );
    }

    #[test]
    fn char_set_difference_with_multiple_kept_ranges() {
        let split = CharSet::from_ranges([('a', 'f'), ('m', 'r')]);
        let cut = CharSet::from_ranges([('e', 'n')]);
        assert_eq!(
            split.difference(&cut).expect("non-empty"),
            CharSet::from_ranges([('a', 'd'), ('o', 'r')]),
        );
        // A subtrahend entirely above every kept range never engages.
        let high = CharSet::from_ranges([('x', 'z')]);
        assert_eq!(split.difference(&high), Some(split));
    }

    proptest::proptest! {
        /// Difference agrees with membership pointwise:
        /// `(a \ b).contains(c) == a.contains(c) && !b.contains(c)`
        /// for every probed character, including when the difference
        /// is empty (`None`).
        #[test]
        fn char_set_difference_matches_pointwise_membership(
            a_ranges in proptest::collection::vec(ascii_range(), 1..4),
            b_ranges in proptest::collection::vec(ascii_range(), 1..4),
        ) {
            let a = CharSet::from_ranges(a_ranges);
            let b = CharSet::from_ranges(b_ranges);
            let difference = a.difference(&b);
            for code in 0_u32..=0x7F {
                let ch = char::from_u32(code).expect("ASCII is valid");
                let expected = a.contains(ch) && !b.contains(ch);
                let actual = difference
                    .as_ref()
                    .is_some_and(|set| set.contains(ch));
                proptest::prop_assert_eq!(
                    actual,
                    expected,
                    "difference disagrees at {:?}",
                    ch,
                );
            }
        }
    }

    /// Strategy: one inclusive ASCII range `(lo, hi)` with
    /// `lo <= hi`.
    fn ascii_range() -> impl proptest::strategy::Strategy<Value = (char, char)> {
        use proptest::strategy::Strategy as _;
        (0_u32..=0x7F, 0_u32..=0x7F).prop_map(|(a, b)| {
            let lo = a.min(b);
            let hi = a.max(b);
            (
                char::from_u32(lo).expect("ASCII is valid"),
                char::from_u32(hi).expect("ASCII is valid"),
            )
        })
    }

    #[test]
    fn char_set_complement_sample_finds_the_smallest_outsider() {
        // First range starts above NUL: NUL is the sample.
        assert_eq!(
            CharSet::from_ranges([('0', '9')]).complement_sample(),
            Some('\0'),
        );
        // First range starts at NUL: the gap after it is sampled.
        assert_eq!(
            CharSet::from_ranges([('\0', '9'), ('a', 'z')]).complement_sample(),
            Some(':'),
        );
        // Full coverage: no outsider exists.
        assert_eq!(
            CharSet::from_ranges([('\0', char::MAX)]).complement_sample(),
            None,
        );
    }

    #[test]
    fn string_membership_decides_str_nodes_by_len_alphabet_and_first() {
        let node = Schema::string(
            LenBound::new(1, 3),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'z')]),
            Some(CharSet::from_ranges([('a', 'm')])),
        );
        assert_eq!(node.string_membership("abc"), Some(true));
        assert_eq!(node.string_membership("m"), Some(true));
        // Length misses on both sides.
        assert_eq!(node.string_membership(""), Some(false));
        assert_eq!(node.string_membership("abcd"), Some(false));
        // Alphabet miss.
        assert_eq!(node.string_membership("a9"), Some(false));
        // First-character miss: in the alphabet, outside the head set.
        assert_eq!(node.string_membership("za"), Some(false));
    }

    #[test]
    fn string_membership_measures_bytes_when_the_unit_is_bytes() {
        let node = Schema::string(
            LenBound::new(2, 2),
            LenUnit::Bytes,
            CharSet::from_ranges([('a', 'z'), ('é', 'é')]),
            None,
        );
        // "é" is one char but two UTF-8 bytes: admitted by bytes.
        assert_eq!(node.string_membership("é"), Some(true));
        assert_eq!(node.string_membership("ab"), Some(true));
        assert_eq!(node.string_membership("a"), Some(false));
    }

    #[test]
    fn string_membership_first_check_is_vacuous_without_a_head() {
        let node = Schema::string(
            LenBound::new(0, 3),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'z')]),
            Some(CharSet::from_ranges([('a', 'm')])),
        );
        // The empty string has no head to reject.
        assert_eq!(node.string_membership(""), Some(true));
    }

    #[test]
    fn string_membership_decides_enumerated_and_recurses_canonicalized() {
        let toggle = Schema::enumerated(&["on", "off"]);
        assert_eq!(toggle.string_membership("off"), Some(true));
        assert_eq!(toggle.string_membership("OFF"), Some(false));
        let trimmed = Schema::canonicalized(Morphism::Trim, toggle);
        assert_eq!(trimmed.string_membership("on"), Some(true));
        assert_eq!(trimmed.string_membership(" on"), Some(false));
    }

    #[test]
    fn string_membership_is_undecided_outside_the_string_fragment() {
        assert_eq!(int_interval(0, 1).string_membership("0"), None);
        assert_eq!(Schema::regex("^a$").string_membership("a"), None);
        assert_eq!(
            Schema::collection(
                LenBound::new(0, 1),
                Schema::enumerated(&["on"]),
                false,
                false
            )
            .string_membership("on"),
            None,
        );
    }

    #[test]
    fn string_membership_folds_unions_and_intersections_with_dominance() {
        let with_regex_union =
            Schema::union(vec![Schema::enumerated(&["on"]), Schema::regex("^x$")]);
        // A decided `true` dominates the undecided regex member.
        assert_eq!(with_regex_union.string_membership("on"), Some(true));
        assert_eq!(with_regex_union.string_membership("off"), None);

        let with_regex_intersection =
            Schema::intersection(vec![Schema::enumerated(&["on"]), Schema::regex("^x$")]);
        // A decided `false` dominates; a decided `true` does not.
        assert_eq!(
            with_regex_intersection.string_membership("off"),
            Some(false),
        );
        assert_eq!(with_regex_intersection.string_membership("on"), None);
    }

    #[test]
    fn string_boundaries_derive_labels_and_near_misses() {
        let toggle = Schema::enumerated(&["on", "off"]);
        assert_eq!(
            toggle.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("OFF", false),
                string_boundary("ON", false),
                string_boundary("o", false),
                string_boundary("of", false),
                string_boundary("off", true),
                string_boundary("offx", false),
                string_boundary("on", true),
                string_boundary("onx", false),
            ],
        );
    }

    #[test]
    fn string_boundaries_classify_a_near_miss_that_is_a_member() {
        // Truncating "ab" yields the member "a": the candidate is
        // classified (accept), not filtered — the membership verdict
        // is the schema's, never the derivation's.
        let nested = Schema::enumerated(&["a", "ab"]);
        assert_eq!(
            nested.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("A", false),
                string_boundary("AB", false),
                string_boundary("a", true),
                string_boundary("ab", true),
                string_boundary("abx", false),
                string_boundary("ax", false),
            ],
        );
    }

    #[test]
    fn string_boundaries_skip_candidates_a_regex_member_leaves_undecided() {
        // In a union with a regex, only the dominating accepts are
        // decidable; every near-miss is skipped rather than guessed.
        let with_regex = Schema::union(vec![Schema::enumerated(&["on"]), Schema::regex("^x$")]);
        assert_eq!(
            with_regex.string_boundaries(),
            [string_boundary("on", true)],
        );
    }

    #[test]
    fn string_boundaries_derive_str_length_edges_and_alphabet_near_miss() {
        let digits = Schema::string(
            LenBound::new(1, 3),
            LenUnit::Chars,
            CharSet::from_ranges([('0', '9')]),
            None,
        );
        assert_eq!(
            digits.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("\0", false),
                string_boundary("0", true),
                string_boundary("00", true),
                string_boundary("000", true),
                string_boundary("0000", false),
            ],
        );
    }

    #[test]
    fn string_boundaries_derive_a_first_character_near_miss() {
        let headed = Schema::string(
            LenBound::new(0, u64::MAX),
            LenUnit::Chars,
            CharSet::from_ranges([('\0', char::MAX)]),
            Some(CharSet::from_ranges([('a', 'm')])),
        );
        assert_eq!(
            headed.string_boundaries(),
            [
                // No head to reject: vacuously admitted.
                string_boundary("", true),
                // Alphabet-admissible head outside the first set.
                string_boundary("\0", false),
                // MIN+1 with an admissible head.
                string_boundary("a", true),
            ],
        );
    }

    #[test]
    fn string_boundaries_skip_a_first_set_covering_the_alphabet() {
        // first ⊇ alphabet: no head can violate it, so no
        // first-character near-miss is derivable.
        let saturated = Schema::string(
            LenBound::new(1, 2),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'z')]),
            Some(CharSet::from_ranges([('a', 'z')])),
        );
        assert_eq!(
            saturated.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("\0", false),
                string_boundary("a", true),
                string_boundary("aa", true),
                string_boundary("aaa", false),
            ],
        );
    }

    #[test]
    fn string_boundaries_measure_byte_unit_edges_with_ascii_fillers() {
        let two_bytes = Schema::string(
            LenBound::new(2, 2),
            LenUnit::Bytes,
            CharSet::from_ranges([('a', 'z')]),
            None,
        );
        assert_eq!(
            two_bytes.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("a", false),
                string_boundary("a\0", false),
                string_boundary("aa", true),
                string_boundary("aaa", false),
            ],
        );
    }

    #[test]
    fn string_boundaries_cap_unreachable_length_edges() {
        // Every length edge exceeds the cap: only the empty string
        // remains, and the near-misses are skipped with it.
        let huge = Schema::string(
            LenBound::new(10_000, 20_000),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'a')]),
            None,
        );
        assert_eq!(huge.string_boundaries(), [string_boundary("", false)]);
    }

    #[test]
    fn string_boundaries_recurse_composites_and_skip_non_string_trees() {
        let trimmed = Schema::canonicalized(Morphism::Trim, Schema::enumerated(&["on"]));
        assert_eq!(
            trimmed.string_boundaries(),
            [
                string_boundary("", false),
                string_boundary("ON", false),
                string_boundary("o", false),
                string_boundary("on", true),
                string_boundary("onx", false),
            ],
        );
        // Non-string vocabulary yields no candidates, and Collection
        // elements have no string embedding at the root.
        assert_eq!(int_interval(0, 1).string_boundaries(), []);
        assert_eq!(
            Schema::collection(
                LenBound::new(0, 1),
                Schema::enumerated(&["on"]),
                false,
                false
            )
            .string_boundaries(),
            [],
        );
    }

    // ─── Display rendering. ────────────────────────────────────────

    #[test]
    fn display_renders_interval_bound_combinations() {
        assert_eq!(int_interval(0, 100).to_string(), "int in 0..=100");
        assert_eq!(
            Schema::interval(
                ScalarKind::Integer,
                Bound::Inclusive(Scalar::Int(1)),
                Bound::Unbounded,
            )
            .to_string(),
            "int in 1..",
        );
        assert_eq!(
            Schema::interval(
                ScalarKind::Integer,
                Bound::Unbounded,
                Bound::Inclusive(Scalar::Int(-1)),
            )
            .to_string(),
            "int in ..=-1",
        );
        assert_eq!(
            Schema::interval(ScalarKind::Integer, Bound::Unbounded, Bound::Unbounded).to_string(),
            "int in ..",
        );
    }

    #[test]
    fn display_renders_every_scalar_kind_label() {
        assert_eq!(
            Schema::interval(
                ScalarKind::Float,
                Bound::Inclusive(Scalar::Float(0.0)),
                Bound::Inclusive(Scalar::Float(1.0)),
            )
            .to_string(),
            "float in 0..=1",
        );
        assert_eq!(
            Schema::interval(
                ScalarKind::Float,
                Bound::Inclusive(Scalar::Float(f64::NEG_INFINITY)),
                Bound::Inclusive(Scalar::Float(f64::INFINITY)),
            )
            .to_string(),
            "float in -inf..=inf",
        );
        assert_eq!(
            Schema::interval(
                ScalarKind::Date,
                Bound::Inclusive(Scalar::Int(730_120)),
                Bound::Inclusive(Scalar::Int(767_009)),
            )
            .to_string(),
            "date(days from CE) in 730120..=767009",
        );
        assert_eq!(
            Schema::interval(
                ScalarKind::DateTime,
                Bound::Inclusive(Scalar::Int(0)),
                Bound::Inclusive(Scalar::Int(1_893_456_000)),
            )
            .to_string(),
            "datetime(unix seconds) in 0..=1893456000",
        );
    }

    #[test]
    fn display_renders_decimal_endpoints_as_scaled_values() {
        // Scale 0 (after joint reduction): plain integers.
        assert_eq!(
            Schema::interval(
                ScalarKind::Decimal { scale: 2 },
                Bound::Inclusive(Scalar::Int(0)),
                Bound::Inclusive(Scalar::Int(10_000)),
            )
            .to_string(),
            "decimal in 0..=100",
        );
        // Irreducible scale: the point is inserted into the digits.
        assert_eq!(
            Schema::interval(
                ScalarKind::Decimal { scale: 2 },
                Bound::Inclusive(Scalar::Int(-12_345)),
                Bound::Inclusive(Scalar::Int(101)),
            )
            .to_string(),
            "decimal in -123.45..=1.01",
        );
        // More scale digits than mantissa digits: zero-padded.
        assert_eq!(
            Schema::interval(
                ScalarKind::Decimal { scale: 3 },
                Bound::Inclusive(Scalar::Int(5)),
                Bound::Unbounded,
            )
            .to_string(),
            "decimal in 0.005..",
        );
    }

    #[test]
    fn display_renders_float_endpoint_under_decimal_kind_literal() {
        // Only reachable through a literal (non-canonical) value: the
        // constructors reject regime mismatches. The renderer stays
        // total and falls back to the scalar's own number.
        let literal = Schema::Interval {
            kind: ScalarKind::Decimal { scale: 2 },
            lo: Bound::Inclusive(Scalar::Float(0.5)),
            hi: Bound::Unbounded,
        };
        assert_eq!(literal.to_string(), "decimal in 0.5..");
    }

    #[test]
    fn display_renders_string_nodes_with_and_without_first_set() {
        let with_first = Schema::string(
            LenBound::new(1, 64),
            LenUnit::Chars,
            CharSet::from_ranges([('a', 'z'), ('0', '9'), ('_', '_')]),
            Some(CharSet::from_ranges([('a', 'z'), ('_', '_')])),
        );
        assert_eq!(
            with_first.to_string(),
            "string(len 1..=64 chars, chars ['0'-'9', '_', 'a'-'z'], first ['_', 'a'-'z'])",
        );
        let bytes_only = Schema::string(
            LenBound::new(0, 16),
            LenUnit::Bytes,
            CharSet::from_ranges([('\n', '\n')]),
            None,
        );
        assert_eq!(
            bytes_only.to_string(),
            "string(len 0..=16 bytes, chars ['\\n'])",
        );
    }

    #[test]
    fn display_renders_regex_and_enumerated_leaves() {
        assert_eq!(Schema::regex("^[A-Z]$").to_string(), "regex /^[A-Z]$/");
        assert_eq!(
            Schema::enumerated(&["active", "in\"active"]).to_string(),
            "one of \"active\", \"in\\\"active\"",
        );
    }

    #[test]
    fn display_renders_collection_flags_and_indented_element() {
        let plain = Schema::collection(LenBound::new(0, 5), int_interval(0, 9), false, false);
        assert_eq!(plain.to_string(), "collection(len 0..=5)\n  int in 0..=9");
        let sorted = Schema::collection(LenBound::new(1, 5), int_interval(0, 9), true, false);
        assert_eq!(
            sorted.to_string(),
            "collection(len 1..=5, sorted)\n  int in 0..=9",
        );
        let unique = Schema::collection(LenBound::new(1, 5), int_interval(0, 9), false, true);
        assert_eq!(
            unique.to_string(),
            "collection(len 1..=5, unique)\n  int in 0..=9",
        );
        let both = Schema::collection(LenBound::new(1, 5), int_interval(0, 9), true, true);
        assert_eq!(
            both.to_string(),
            "collection(len 1..=5, sorted, unique)\n  int in 0..=9",
        );
    }

    #[test]
    fn display_renders_nested_composites_one_line_per_level() {
        let union = Schema::union(vec![int_interval(0, 9), int_interval(20, 30)]);
        let date = Schema::interval(
            ScalarKind::Date,
            Bound::Inclusive(Scalar::Int(700_000)),
            Bound::Unbounded,
        );
        let tree = Schema::canonicalized(
            Morphism::AsciiLowercase,
            Schema::intersection(vec![union, date]),
        );
        assert_eq!(
            tree.to_string(),
            "canonicalized by ascii-lowercase\n  \
             all of\n    \
             date(days from CE) in 700000..\n    \
             any of\n      \
             int in 0..=9\n      \
             int in 20..=30",
        );
    }

    #[test]
    fn display_renders_every_morphism_label() {
        assert_eq!(Morphism::Trim.to_string(), "trim");
        assert_eq!(Morphism::AsciiLowercase.to_string(), "ascii-lowercase");
        assert_eq!(Morphism::AsciiUppercase.to_string(), "ascii-uppercase");
        assert_eq!(LenUnit::Chars.to_string(), "chars");
        assert_eq!(LenUnit::Bytes.to_string(), "bytes");
    }

    /// Writer with a budget of successful writes, mirroring the
    /// closed-set Display sweep: rendering must propagate a formatter
    /// failure at every write boundary and eventually succeed.
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

    #[test]
    fn display_propagates_formatter_errors_at_every_write() {
        let tree = Schema::canonicalized(
            Morphism::Trim,
            Schema::intersection(vec![
                Schema::union(vec![
                    int_interval(0, 9),
                    Schema::interval(
                        ScalarKind::Decimal { scale: 1 },
                        Bound::Inclusive(Scalar::Int(-5)),
                        Bound::Inclusive(Scalar::Int(5)),
                    ),
                    Schema::interval(
                        ScalarKind::Float,
                        Bound::Inclusive(Scalar::Float(0.0)),
                        Bound::Inclusive(Scalar::Float(1.0)),
                    ),
                    Schema::interval(
                        ScalarKind::Date,
                        Bound::Inclusive(Scalar::Int(0)),
                        Bound::Unbounded,
                    ),
                    Schema::interval(
                        ScalarKind::DateTime,
                        Bound::Unbounded,
                        Bound::Inclusive(Scalar::Int(0)),
                    ),
                ]),
                Schema::string(
                    LenBound::new(1, 8),
                    LenUnit::Chars,
                    CharSet::from_ranges([('a', 'z'), ('_', '_')]),
                    Some(CharSet::from_ranges([('a', 'a')])),
                ),
                Schema::collection(LenBound::new(0, 3), Schema::regex("^x$"), true, true),
                Schema::enumerated(&["on", "off"]),
            ]),
        );
        let succeeded = (0..512).any(|budget| {
            let mut sink = FailAfter { remaining: budget };
            core::fmt::write(&mut sink, format_args!("{tree}")).is_ok()
        });
        assert!(
            succeeded,
            "rendering did not complete within the write budget",
        );
    }
}
