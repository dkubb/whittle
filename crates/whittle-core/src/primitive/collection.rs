//! Collection primitive rules.
//!
//! Length bounds, per-element refinement, and key-based uniqueness
//! for `Vec<T>`. Other collection shapes (`BTreeSet`, `BTreeMap`,
//! custom ordered sets) land in later commits once a real consumer
//! needs them.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::marker::PhantomData;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::{Refined, Rule};

/// Inclusive bound on the number of items in a `Vec<T>`:
/// `MIN <= len <= MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{CollectionError, LenItems};
///
/// // Admit: length is within `1..=3`.
/// let ok: Refined<Vec<i32>, LenItems<1, 3>>
///     = Refined::try_new(vec![1, 2]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2]);
///
/// // Reject: empty vector falls below `MIN = 1`.
/// let err = Refined::<Vec<i32>, LenItems<1, 3>>::try_new(Vec::new())
///     .unwrap_err();
/// assert_eq!(err, CollectionError::LenOutOfRange { actual: 0 });
/// ```
pub struct LenItems<const MIN: usize, const MAX: usize>;

/// Marker: rules over `Vec<_>` whose admissibility depends only on
/// the vector's *length*, never on the elements themselves.
///
/// Unlocks the infallible [`Refined::map_items`]: an element-wise
/// map yields exactly one output per input, so it cannot change the
/// vector's length — and a length-only rule therefore cannot be
/// invalidated by it. Follows the same capability-marker pattern as
/// [`crate::transform::StableUnderTrim`]; see that trait's docs for
/// the four-step audit recipe.
///
/// # Implementor obligation
///
/// Implement this marker only when BOTH hold for the rule `R`:
///
/// 1. For every element type `T` with `R: Rule<Vec<T>>`, the
///    `refine` impl admits or rejects solely on `Vec::len` — it
///    never inspects, compares, or depends on the elements.
/// 2. The `refine` impl is a pure predicate: it returns the input
///    vector unchanged on success (no canonicalisation). `map_items`
///    bypasses `refine` entirely, so a canonicalising impl would let
///    a non-canonical carrier escape.
///
/// As with the `StableUnder*` markers, the obligation must hold for
/// EVERY admissible vector and every map, not only for the shapes a
/// particular caller produces. For composed rules the marker
/// propagates: `And<A, B>` / `Or<A, B>` are stable under element
/// maps iff both operands are (the kernel provides those impls).
pub trait StableUnderElementMap {}

/// SOUNDNESS: `LenItems::refine` reads only `Vec::len` and returns
/// the input unchanged — both obligations hold for every element
/// type and every length bound.
impl<const MIN: usize, const MAX: usize> StableUnderElementMap for LenItems<MIN, MAX> {}

/// Every item in the collection must satisfy the inner rule `R`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     AllItems, CollectionError, NumericError, Within,
/// };
///
/// // Admit: every item is in `0..=100`.
/// let ok: Refined<Vec<i32>, AllItems<Within<0, 100>>>
///     = Refined::try_new(vec![0, 50, 100]).unwrap();
/// assert_eq!(ok.as_inner(), &[0, 50, 100]);
///
/// // Reject: item at index 2 is out of range. `Within<0, 100>`
/// // is a nominal newtype whose flat `NumericError` surfaces here
/// // as the `source` of the collection's `BadItem` variant.
/// let err = Refined::<Vec<i32>, AllItems<Within<0, 100>>>::try_new(
///     vec![0, 50, 101],
/// ).unwrap_err();
/// assert_eq!(
///     err,
///     CollectionError::BadItem {
///         index: 2,
///         source: NumericError::OutOfRange { value: 101 },
///     },
/// );
/// ```
pub struct AllItems<R>(PhantomData<R>);

/// Duplicate-free `Vec<T>`: shorthand for `UniqueByKey<T,
/// IdentityKey<T>>`.
///
/// The audit of downstream consumers (`symbiote`, `incremental-gate`)
/// surfaced repeated long-form `UniqueByKey<T, IdentityKey<T>>`
/// spellings. `Distinct<T>` is a type alias over the same rule so
/// the same uniqueness invariant reads as a single token at call
/// sites.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{CollectionError, Distinct};
///
/// // Admit: every value is distinct.
/// let ok: Refined<Vec<i32>, Distinct<i32>>
///     = Refined::try_new(vec![1, 2, 3]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2, 3]);
///
/// // Reject: `1` appears again at index 2.
/// let err = Refined::<Vec<i32>, Distinct<i32>>::try_new(
///     vec![1, 2, 1],
/// ).unwrap_err();
/// assert_eq!(err, CollectionError::DuplicateKey { index: 2 });
/// ```
pub type Distinct<T> = UniqueByKey<T, IdentityKey<T>>;

/// `Vec<T>` is sorted ascending by the key derived through
/// `K: KeyOf<T>`.
///
/// Non-strict: equal adjacent keys are admissible (the rule only
/// rejects when a later key is strictly less than its predecessor).
/// Use a separate uniqueness rule (`UniqueByKey` / `Distinct`) when
/// strict ascending order is required.
///
/// The error pinpoints the index of the first out-of-order element
/// (the later element in the offending pair).
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     CollectionError, IdentityKey, Sorted,
/// };
///
/// // Admit: ascending order with a duplicate is allowed.
/// let ok: Refined<Vec<i32>, Sorted<i32, IdentityKey<i32>>>
///     = Refined::try_new(vec![1, 2, 2, 5]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2, 2, 5]);
///
/// // Reject: element at index 2 is less than its predecessor.
/// let err = Refined::<
///     Vec<i32>,
///     Sorted<i32, IdentityKey<i32>>,
/// >::try_new(vec![1, 5, 3]).unwrap_err();
/// assert_eq!(err, CollectionError::NotSorted { index: 2 });
/// ```
pub struct Sorted<T, K>(PhantomData<(T, K)>);

/// Items must be unique under a key derived from each item by the
/// `K: KeyOf<T>` extractor.
///
/// Order is preserved: the first occurrence of each key wins. A
/// second occurrence is reported as
/// `CollectionError::DuplicateKey { index, … }`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     CollectionError, IdentityKey, UniqueByKey,
/// };
///
/// // Admit: every value is distinct.
/// let ok: Refined<Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>>
///     = Refined::try_new(vec![1, 2, 3]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2, 3]);
///
/// // Reject: `1` appears again at index 2.
/// let err = Refined::<
///     Vec<i32>,
///     UniqueByKey<i32, IdentityKey<i32>>,
/// >::try_new(vec![1, 2, 1]).unwrap_err();
/// assert_eq!(err, CollectionError::DuplicateKey { index: 2 });
/// ```
pub struct UniqueByKey<T, K>(PhantomData<(T, K)>);

/// Extracts a comparable, ownable key from a `&T`.
///
/// Used by `UniqueByKey<T, K>` to detect duplicates without
/// requiring `T: Ord + Clone` directly.
pub trait KeyOf<T>: 'static {
    /// The key type. Must be comparable (`Ord`) and ownable
    /// (`Clone`) so the set under the hood is `BTreeSet<Key>`.
    type Key: Ord + Clone;
    /// Extract a key from `value`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::{IdentityKey, KeyOf};
    ///
    /// // Library-supplied `IdentityKey<T>` clones the value as its key.
    /// assert_eq!(<IdentityKey<i32> as KeyOf<i32>>::key_of(&7), 7);
    ///
    /// // Custom extractor: project the second tuple field.
    /// pub struct ByB;
    /// impl KeyOf<(i32, i32)> for ByB {
    ///     type Key = i32;
    ///     fn key_of(value: &(i32, i32)) -> i32 { value.1 }
    /// }
    /// assert_eq!(<ByB as KeyOf<(i32, i32)>>::key_of(&(10, 99)), 99);
    /// ```
    fn key_of(value: &T) -> Self::Key;
}

/// Identity key: `T` is its own ordering key. Requires
/// `T: Ord + Clone + 'static`.
///
/// Useful when the element type is itself an identifier or a
/// fingerprint that should be unique across the collection without
/// projecting a sub-field.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{
///     CollectionError, IdentityKey, UniqueByKey,
/// };
///
/// // Admit: distinct values under identity.
/// let ok: Refined<Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>>
///     = Refined::try_new(vec![1, 2, 3]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2, 3]);
///
/// // Reject: duplicate at index 2.
/// let err = Refined::<
///     Vec<i32>,
///     UniqueByKey<i32, IdentityKey<i32>>,
/// >::try_new(vec![1, 2, 1]).unwrap_err();
/// assert_eq!(err, CollectionError::DuplicateKey { index: 2 });
/// ```
pub struct IdentityKey<T>(PhantomData<T>);

impl<T: 'static + Ord + Clone> KeyOf<T> for IdentityKey<T> {
    type Key = T;
    #[inline]
    fn key_of(value: &T) -> T {
        value.clone()
    }
}

/// Pure boolean test over `&T`.
///
/// Distinct from `Rule<T>` — a `Predicate` only answers yes/no;
/// it neither validates the value nor produces an output. Used
/// by `NoneOf<P>` and `AnyOf<P>` to express collection-level
/// "no item matches" / "some item matches" invariants without
/// consuming items.
///
/// Implementations are zero-sized type markers so they compose
/// cleanly with `NoneOf` / `AnyOf` and the future schema
/// reflection.
pub trait Predicate<T: ?Sized>: 'static {
    /// Return `true` when the predicate accepts `value`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Predicate;
    ///
    /// // Custom predicate: zero detector for `i32`.
    /// pub struct IsZero;
    /// impl Predicate<i32> for IsZero {
    ///     fn test(value: &i32) -> bool { *value == 0 }
    /// }
    ///
    /// assert!(<IsZero as Predicate<i32>>::test(&0));
    /// assert!(!<IsZero as Predicate<i32>>::test(&1));
    /// ```
    fn test(value: &T) -> bool;
}

/// `Predicate<T>` that exposes a `proptest` strategy emitting values
/// admissible under the predicate.
///
/// Used by `AnyOf<P>`'s `ArbitraryRule` impl to seed the generated
/// collection with at least one matching item. The strategy MUST
/// produce only values that satisfy `Predicate::test`.
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait ArbitraryPredicate<T>: Predicate<T>
where
    T: 'static,
{
    /// Strategy type yielding values admissible under the predicate.
    type Strategy: proptest::strategy::Strategy<Value = T>;

    /// Construct the predicate's value-emitting strategy.
    fn arbitrary_matching() -> Self::Strategy;
}

/// Rule: reject if any item in the collection satisfies `P`.
///
/// The error pinpoints the first matching item by index. Used
/// for collection-level "this list contains no _" invariants.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{CollectionError, NoneOf, Predicate};
///
/// // Custom predicate: zero detector.
/// pub struct IsZero;
/// impl Predicate<i32> for IsZero {
///     fn test(value: &i32) -> bool { *value == 0 }
/// }
///
/// // Admit: no zeros in the collection.
/// let ok: Refined<Vec<i32>, NoneOf<IsZero>>
///     = Refined::try_new(vec![1, 2, 3]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 2, 3]);
///
/// // Reject: the first zero is at index 1.
/// let err = Refined::<Vec<i32>, NoneOf<IsZero>>::try_new(vec![1, 0, 2])
///     .unwrap_err();
/// assert_eq!(err, CollectionError::MatchingItem { index: 1 });
/// ```
pub struct NoneOf<P>(PhantomData<P>);

/// Rule: require at least one item to satisfy `P`.
///
/// The error is `NoMatchingItem` when no item matches. Used for
/// collection-level "this list contains at least one _"
/// invariants.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{AnyOf, CollectionError, Predicate};
///
/// // Custom predicate: zero detector.
/// pub struct IsZero;
/// impl Predicate<i32> for IsZero {
///     fn test(value: &i32) -> bool { *value == 0 }
/// }
///
/// // Admit: at least one zero is present.
/// let ok: Refined<Vec<i32>, AnyOf<IsZero>>
///     = Refined::try_new(vec![1, 0, 2]).unwrap();
/// assert_eq!(ok.as_inner(), &[1, 0, 2]);
///
/// // Reject: no item matches.
/// let err = Refined::<Vec<i32>, AnyOf<IsZero>>::try_new(vec![1, 2, 3])
///     .unwrap_err();
/// assert_eq!(err, CollectionError::NoMatchingItem);
/// ```
pub struct AnyOf<P>(PhantomData<P>);

/// Errors common to every collection primitive.
///
/// Invalid rule configurations (e.g. `LenItems<MIN, MAX>` with
/// `MIN > MAX`) are rejected at compile time via `const { assert!
/// (...) }` blocks inside the affected `Rule::refine` impls, so
/// the corresponding error variant is unrepresentable.
#[derive(Debug, PartialEq, Eq)]
pub enum CollectionError<EI = core::convert::Infallible> {
    /// Length not in the admissible range.
    LenOutOfRange {
        /// Observed length of the offending collection.
        actual: usize,
    },

    /// `AllItems<R>` rejected the item at the given index.
    BadItem {
        /// Position of the rejected item in the original collection.
        index: usize,
        /// The inner rule's error.
        source: EI,
    },

    /// `UniqueByKey<T, K>` saw a duplicate key. The second
    /// occurrence's index is reported; the first wins.
    DuplicateKey {
        /// Position of the duplicate (the second occurrence).
        index: usize,
    },

    /// `NoneOf<P>` saw an item that matches the forbidden
    /// predicate.
    MatchingItem {
        /// Position of the first matching item.
        index: usize,
    },

    /// `AnyOf<P>` saw no item that matches the required
    /// predicate.
    NoMatchingItem,

    /// `Sorted<T, K>` saw an element whose key is strictly less
    /// than its immediate predecessor's key. The reported index
    /// is the position of the offending later element.
    NotSorted {
        /// Position of the first out-of-order element.
        index: usize,
    },
}

impl<EI> core::fmt::Display for CollectionError<EI>
where
    EI: core::fmt::Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LenOutOfRange { actual } => {
                write!(f, "length {actual} not in admissible range")
            }
            Self::BadItem { index, source } => {
                write!(f, "item at index {index}: {source}")
            }
            Self::DuplicateKey { index } => {
                write!(f, "duplicate key at index {index}")
            }
            Self::MatchingItem { index } => {
                write!(f, "item at index {index} matches a forbidden predicate")
            }
            Self::NoMatchingItem => f.write_str("no item matches the required predicate"),
            Self::NotSorted { index } => {
                write!(f, "element at index {index} breaks ascending order")
            }
        }
    }
}

impl<EI> core::error::Error for CollectionError<EI>
where
    EI: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::BadItem { source, .. } => Some(source),
            Self::LenOutOfRange { .. }
            | Self::DuplicateKey { .. }
            | Self::MatchingItem { .. }
            | Self::NoMatchingItem
            | Self::NotSorted { .. } => None,
        }
    }
}

/// Typed overflow rejection for checked mutation of `LenItems`-ruled
/// collections.
///
/// Carries the rejected payload back to the caller — the single item
/// for [`Refined::try_push`] (`CapacityFull<T>`), the whole
/// all-or-nothing batch for [`Refined::try_extend`]
/// (`CapacityFull<Vec<T>>`). The collection is untouched on the
/// `Err` path, so callers keep both the collection and the payload
/// and can implement eviction or reporting on top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityFull<T> {
    /// The rejected item(s); ownership returns to the caller.
    pub rejected: T,
}

impl<T> core::fmt::Display for CapacityFull<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The payload is deliberately not printed: `T` need not be
        // `Display`, and the typed field already hands it back.
        f.write_str("collection is at capacity; rejected input returned to caller")
    }
}

impl<T> core::error::Error for CapacityFull<T> where T: core::fmt::Debug {}

impl<const MIN: usize, const MAX: usize> LenItems<MIN, MAX> {
    /// Single source of the bound invariant: `MIN <= MAX`. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }`.
    const VALID: () = assert!(MIN <= MAX, "LenItems requires MIN <= MAX");
}

impl<T, const MIN: usize, const MAX: usize> Refined<Vec<T>, LenItems<MIN, MAX>> {
    /// Append `item`, keeping the length proof, or hand it back when
    /// the collection is at `MAX` capacity.
    ///
    /// On `Err` the collection is untouched and still usable, and
    /// the rejected item rides back inside [`CapacityFull`], so
    /// callers can branch on a typed overflow outcome (eviction,
    /// reporting, back-pressure) without losing either side.
    ///
    /// # Soundness
    ///
    /// The mutation commits without re-running `refine` because both
    /// bounds are re-established by construction:
    ///
    /// - `MIN`: the existence of `self` proves `len >= MIN`, and a
    ///   push can only grow the length, so the lower bound cannot be
    ///   violated.
    /// - `MAX`: the push commits only on the `len < MAX` branch, so
    ///   the new length is at most `MAX`.
    ///
    /// `LenItems` admits on length alone and never canonicalises,
    /// so no other invariant exists to re-check.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityFull`] carrying `item` back when the
    /// collection already holds `MAX` items.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::{CapacityFull, LenItems};
    ///
    /// let mut buffer: Refined<Vec<i32>, LenItems<1, 2>> =
    ///     Refined::try_new(vec![10]).unwrap();
    ///
    /// // Admit: capacity remains, the proof is maintained.
    /// assert_eq!(buffer.try_push(20), Ok(()));
    /// assert_eq!(buffer.as_inner(), &[10, 20]);
    ///
    /// // Reject: at capacity the item comes back typed, and the
    /// // collection is untouched.
    /// assert_eq!(buffer.try_push(30), Err(CapacityFull { rejected: 30 }));
    /// assert_eq!(buffer.as_inner(), &[10, 20]);
    /// ```
    #[inline]
    pub fn try_push(&mut self, item: T) -> Result<(), CapacityFull<T>> {
        const { LenItems::<MIN, MAX>::VALID };
        if self.as_inner().len() < MAX {
            // SOUNDNESS: `len < MAX` was just checked, so the new
            // length is `<= MAX`; growth cannot violate `MIN`.
            self.as_inner_mut().push(item);
            Ok(())
        } else {
            Err(CapacityFull { rejected: item })
        }
    }

    /// Append every item in `items`, all-or-nothing: when the whole
    /// batch fits under `MAX` the proof is maintained; otherwise
    /// nothing is appended and the collected batch rides back.
    ///
    /// # Soundness
    ///
    /// Same argument as [`Refined::try_push`]: extension only grows
    /// the length (so `MIN` holds by construction proof), and the
    /// batch commits only when `batch.len()` fits in the remaining
    /// `MAX - len` capacity, so the new length is at most `MAX`.
    /// The subtraction cannot underflow because the construction
    /// proof guarantees `len <= MAX`.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityFull`] carrying the entire collected batch
    /// back when appending it would exceed `MAX`. The collection is
    /// untouched — no prefix is committed.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::{CapacityFull, LenItems};
    ///
    /// let mut paths: Refined<Vec<i32>, LenItems<1, 4>> =
    ///     Refined::try_new(vec![1]).unwrap();
    ///
    /// // Admit: the whole batch fits.
    /// assert_eq!(paths.try_extend([2, 3]), Ok(()));
    /// assert_eq!(paths.as_inner(), &[1, 2, 3]);
    ///
    /// // Reject all-or-nothing: a 2-item batch does not fit in the
    /// // 1 remaining slot, so nothing is appended and the whole
    /// // batch comes back.
    /// assert_eq!(
    ///     paths.try_extend([4, 5]),
    ///     Err(CapacityFull { rejected: vec![4, 5] }),
    /// );
    /// assert_eq!(paths.as_inner(), &[1, 2, 3]);
    /// ```
    #[inline]
    pub fn try_extend<I>(&mut self, items: I) -> Result<(), CapacityFull<Vec<T>>>
    where
        I: IntoIterator<Item = T>,
    {
        const { LenItems::<MIN, MAX>::VALID };
        let batch: Vec<T> = items.into_iter().collect();
        // SOUNDNESS: the construction proof guarantees
        // `len <= MAX`, so `MAX - len` cannot underflow.
        let remaining = MAX - self.as_inner().len();
        if batch.len() <= remaining {
            // SOUNDNESS: the batch fits in the remaining capacity,
            // so the new length is `<= MAX`; growth cannot violate
            // `MIN`.
            self.as_inner_mut().extend(batch);
            Ok(())
        } else {
            Err(CapacityFull { rejected: batch })
        }
    }
}

impl<T, R> Refined<Vec<T>, R>
where
    T: 'static,
{
    /// Map each element through `f`, keeping the rule's proof
    /// without re-running `refine`.
    ///
    /// Available only for rules marked [`StableUnderElementMap`]
    /// (length-only rules such as [`LenItems`]). For arbitrary rule
    /// pairs — or element maps that must re-establish a per-element
    /// invariant — use the re-validating [`Refined::try_map`]
    /// instead, which routes through `try_new`.
    ///
    /// # Soundness
    ///
    /// The existence of `self` proves the input vector was admissible
    /// under `R`. `Iterator::map` over `into_iter` yields exactly one
    /// output per input, so the mapped vector's length equals the
    /// input's. `R: StableUnderElementMap` obligates `R`'s
    /// admissibility (for every element type) to depend only on that
    /// unchanged length and its `refine` to be a pure predicate, so
    /// the mapped vector is admissible — and already canonical —
    /// without re-running `refine`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::LenItems;
    ///
    /// let files: Refined<Vec<i32>, LenItems<1, 3>> =
    ///     Refined::try_new(vec![10, 20]).unwrap();
    ///
    /// // Infallible: the length proof transfers to the mapped
    /// // vector — no `Result`, no re-validation.
    /// let paths: Refined<Vec<String>, LenItems<1, 3>> =
    ///     files.map_items(|n| n.to_string());
    /// assert_eq!(paths.as_inner(), &["10".to_string(), "20".to_string()]);
    /// ```
    #[must_use]
    pub fn map_items<U, F>(self, f: F) -> Refined<Vec<U>, R>
    where
        U: 'static,
        R: Rule<Vec<T>> + Rule<Vec<U>> + StableUnderElementMap,
        F: FnMut(T) -> U,
    {
        let mapped: Vec<U> = self.into_inner().into_iter().map(f).collect();
        // SOUNDNESS: `map` preserves length and `R:
        // StableUnderElementMap` certifies `R` admits on length
        // alone, so the construction-time proof carries over.
        Refined::from_inner(mapped)
    }
}

impl<T, const MAX: usize> Refined<Vec<T>, LenItems<1, MAX>> {
    /// Borrow the first item of a statically non-empty refined
    /// vector.
    ///
    /// `LenItems<1, MAX>` rejects the empty vector at construction
    /// time, so callers do not need to handle the `None` branch that
    /// `Vec::first` exposes for unrefined vectors.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::Refined;
    /// use whittle_core::primitive::LenItems;
    ///
    /// let items: Refined<Vec<i32>, LenItems<1, 3>>
    ///     = Refined::try_new(vec![10, 20]).unwrap();
    /// assert_eq!(*items.first(), 10);
    /// ```
    #[inline]
    #[must_use]
    pub fn first(&self) -> &T {
        &self.as_inner()[0]
    }
}

impl<T, const MIN: usize, const MAX: usize> Rule<Vec<T>> for LenItems<MIN, MAX>
where
    T: 'static,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        const { Self::VALID };
        let actual = raw.len();
        if !(MIN..=MAX).contains(&actual) {
            return Err(CollectionError::LenOutOfRange { actual });
        }
        Ok(raw)
    }
}

impl<T, R> Rule<Vec<T>> for AllItems<R>
where
    T: 'static,
    R: Rule<T>,
{
    type Error = CollectionError<R::Error>;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        // Refine each item in place; collect the failing index +
        // inner error if any item rejects.
        let mut out: Vec<T> = Vec::with_capacity(raw.len());
        for (index, item) in raw.into_iter().enumerate() {
            match R::refine(item) {
                Ok(refined) => out.push(refined),
                Err(source) => {
                    return Err(CollectionError::BadItem { index, source });
                }
            }
        }
        Ok(out)
    }
}

impl<T, K> Rule<Vec<T>> for UniqueByKey<T, K>
where
    T: 'static,
    K: KeyOf<T>,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        let mut seen: BTreeSet<K::Key> = BTreeSet::new();
        for (index, item) in raw.iter().enumerate() {
            if !seen.insert(K::key_of(item)) {
                return Err(CollectionError::DuplicateKey { index });
            }
        }
        Ok(raw)
    }
}

impl<T, K> Rule<Vec<T>> for Sorted<T, K>
where
    T: 'static,
    K: KeyOf<T>,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        // Walk adjacent pairs; reject on the first decrease.
        // Equal adjacent keys are admissible (non-strict ascending).
        for index in 1..raw.len() {
            if K::key_of(&raw[index]) < K::key_of(&raw[index - 1]) {
                return Err(CollectionError::NotSorted { index });
            }
        }
        Ok(raw)
    }
}

impl<T, P> Rule<Vec<T>> for NoneOf<P>
where
    T: 'static,
    P: Predicate<T>,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        for (index, item) in raw.iter().enumerate() {
            if P::test(item) {
                return Err(CollectionError::MatchingItem { index });
            }
        }
        Ok(raw)
    }
}

impl<T, P> Rule<Vec<T>> for AnyOf<P>
where
    T: 'static,
    P: Predicate<T>,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        if raw.iter().any(|item| P::test(item)) {
            Ok(raw)
        } else {
            Err(CollectionError::NoMatchingItem)
        }
    }
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// Length-bounded vectors draw from `T`'s `Arbitrary` strategy.
// `AllItems<R>` draws each element from `R`'s `ArbitraryRule`
// strategy so every element is admissible by construction.
// `Distinct` and `UniqueByKey` use a `BTreeSet`-based collector to
// guarantee uniqueness at construction time without rejection
// sampling. `Sorted` post-`sort_by_key`s. `NoneOf` filters the
// element strategy; `AnyOf` seeds the collection with a guaranteed
// match supplied by `ArbitraryPredicate`.

/// Cap on the number of items generated when an admissible-length
/// upper bound is unbounded.
#[cfg(feature = "proptest")]
const COLLECTION_ARBITRARY_MAX_LEN: usize = 32;

#[cfg(feature = "proptest")]
impl<T, const MIN: usize, const MAX: usize> ArbitraryRule<Vec<T>> for LenItems<MIN, MAX>
where
    T: proptest::arbitrary::Arbitrary + core::fmt::Debug + 'static,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        proptest::collection::vec(proptest::arbitrary::any::<T>(), MIN..=MAX).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, R> ArbitraryRule<Vec<T>> for AllItems<R>
where
    T: core::fmt::Debug + 'static,
    R: ArbitraryRule<T>,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::collection::vec(
            R::arbitrary_strategy(),
            0_usize..=COLLECTION_ARBITRARY_MAX_LEN,
        )
        .boxed()
    }
}

#[cfg(feature = "proptest")]
fn dedup_by_key<T, K>(raw: Vec<T>) -> Vec<T>
where
    T: 'static,
    K: KeyOf<T>,
{
    // Order of first occurrence is preserved (mirrors
    // `UniqueByKey::refine`'s semantics). `seen.insert(key)`
    // returns `false` on the second sighting of a key, so the
    // duplicate is dropped.
    let mut seen: BTreeSet<<K as KeyOf<T>>::Key> = BTreeSet::new();
    let mut out: Vec<T> = Vec::with_capacity(raw.len());
    for item in raw {
        let key = K::key_of(&item);
        if seen.insert(key) {
            out.push(item);
        }
    }
    out
}

#[cfg(feature = "proptest")]
impl<T, K> ArbitraryRule<Vec<T>> for UniqueByKey<T, K>
where
    T: proptest::arbitrary::Arbitrary + core::fmt::Debug + 'static,
    K: KeyOf<T>,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::collection::vec(
            proptest::arbitrary::any::<T>(),
            0_usize..=COLLECTION_ARBITRARY_MAX_LEN,
        )
        .prop_map(dedup_by_key::<T, K>)
        .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, K> ArbitraryRule<Vec<T>> for Sorted<T, K>
where
    T: proptest::arbitrary::Arbitrary + core::fmt::Debug + Clone + 'static,
    K: KeyOf<T>,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::collection::vec(
            proptest::arbitrary::any::<T>(),
            0_usize..=COLLECTION_ARBITRARY_MAX_LEN,
        )
        .prop_map(|mut raw| {
            raw.sort_by(|a, b| K::key_of(a).cmp(&K::key_of(b)));
            raw
        })
        .boxed()
    }
}

#[cfg(feature = "proptest")]
fn predicate_does_not_match<T, P>(value: &T) -> bool
where
    T: 'static,
    P: Predicate<T>,
{
    !P::test(value)
}

#[cfg(feature = "proptest")]
impl<T, P> ArbitraryRule<Vec<T>> for NoneOf<P>
where
    T: proptest::arbitrary::Arbitrary + core::fmt::Debug + 'static,
    P: Predicate<T>,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let pred: fn(&T) -> bool = predicate_does_not_match::<T, P>;
        let filtered =
            proptest::arbitrary::any::<T>().prop_filter("NoneOf: predicate matched", pred);
        proptest::collection::vec(filtered, 0_usize..=COLLECTION_ARBITRARY_MAX_LEN).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, P> ArbitraryRule<Vec<T>> for AnyOf<P>
where
    T: proptest::arbitrary::Arbitrary + core::fmt::Debug + 'static,
    P: ArbitraryPredicate<T>,
{
    type Strategy = proptest::strategy::BoxedStrategy<Vec<T>>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // Generate a guaranteed-matching seed and an arbitrary
        // tail; concat with the seed at a random index so the
        // match is not always at the head.
        (
            P::arbitrary_matching(),
            proptest::collection::vec(
                proptest::arbitrary::any::<T>(),
                0_usize..=COLLECTION_ARBITRARY_MAX_LEN,
            ),
        )
            .prop_map(|(seed, mut rest)| {
                rest.push(seed);
                rest
            })
            .boxed()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::{
        AllItems, AnyOf, CollectionError, Distinct, IdentityKey, LenItems, NoneOf, Predicate,
        Sorted, UniqueByKey,
    };
    use crate::primitive::{NumericError, Within};
    use crate::rule::Refined;

    refinement! {
        /// Macro-generated newtype for testing: `Vec<i32>` of unique
        /// values with 1..=5 items. Exercises `refinement!` from the
        /// collection primitive test module.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub TestUniqueShort:
            Vec<i32>,
            crate::composition::And<
                LenItems<1, 5>,
                UniqueByKey<i32, IdentityKey<i32>>,
            >;
    }

    /// Test predicate: `true` iff the integer is zero.
    enum IsZero {}
    impl Predicate<i32> for IsZero {
        fn test(value: &i32) -> bool {
            *value == 0
        }
    }

    #[cfg(feature = "proptest")]
    impl super::ArbitraryPredicate<i32> for IsZero {
        type Strategy = proptest::strategy::BoxedStrategy<i32>;
        fn arbitrary_matching() -> Self::Strategy {
            use proptest::strategy::Strategy as _;
            proptest::strategy::Just(0_i32).boxed()
        }
    }

    #[test]
    fn len_items_inclusive_bounds() {
        let one: Refined<Vec<i32>, LenItems<1, 5>> = Refined::try_new(vec![10]).unwrap();
        assert_eq!(one.as_inner(), &[10]);
        let five: Refined<Vec<i32>, LenItems<1, 5>> =
            Refined::try_new(vec![1, 2, 3, 4, 5]).unwrap();
        assert_eq!(five.as_inner(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn len_items_non_empty_first_returns_head_without_option() {
        let items: Refined<Vec<i32>, LenItems<1, 5>> = Refined::try_new(vec![10, 20, 30]).unwrap();
        assert_eq!(*items.first(), 10);
    }

    #[test]
    fn len_items_rejects_below_min() {
        let result: Result<Refined<Vec<i32>, LenItems<1, 5>>, _> = Refined::try_new(Vec::new());
        assert_eq!(
            result.unwrap_err(),
            CollectionError::LenOutOfRange { actual: 0 },
        );
    }

    #[test]
    fn len_items_rejects_above_max() {
        let result: Result<Refined<Vec<i32>, LenItems<1, 3>>, _> =
            Refined::try_new(vec![1, 2, 3, 4]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::LenOutOfRange { actual: 4 },
        );
    }

    // LenItems<MIN, MAX> with MIN > MAX is rejected at compile
    // time via `const { assert!(MIN <= MAX) }`; the previous
    // runtime test that exercised that branch is no longer needed
    // because the offending monomorphization is unrepresentable.

    // ─── try_push / try_extend (checked mutation). ───────────────

    #[test]
    fn try_push_appends_below_capacity() {
        let mut buffer: Refined<Vec<i32>, LenItems<1, 3>> = Refined::try_new(vec![10]).unwrap();
        assert_eq!(buffer.try_push(20), Ok(()));
        assert_eq!(buffer.as_inner(), &[10, 20]);
    }

    #[test]
    fn try_push_at_capacity_returns_item_and_leaves_collection_usable() {
        let mut buffer: Refined<Vec<i32>, LenItems<1, 2>> = Refined::try_new(vec![10, 20]).unwrap();
        assert_eq!(
            buffer.try_push(30),
            Err(super::CapacityFull { rejected: 30 }),
        );
        // The Err path left the collection untouched and usable.
        assert_eq!(buffer.as_inner(), &[10, 20]);
        assert_eq!(
            buffer.try_push(40),
            Err(super::CapacityFull { rejected: 40 }),
        );
    }

    #[test]
    fn try_extend_appends_when_whole_batch_fits() {
        let mut buffer: Refined<Vec<i32>, LenItems<1, 4>> = Refined::try_new(vec![1]).unwrap();
        assert_eq!(buffer.try_extend([2, 3]), Ok(()));
        assert_eq!(buffer.as_inner(), &[1, 2, 3]);
    }

    #[test]
    fn try_extend_exact_fit_fills_to_max() {
        // Boundary: batch length equals the remaining capacity.
        let mut buffer: Refined<Vec<i32>, LenItems<1, 4>> = Refined::try_new(vec![1, 2]).unwrap();
        assert_eq!(buffer.try_extend([3, 4]), Ok(()));
        assert_eq!(buffer.as_inner(), &[1, 2, 3, 4]);
    }

    #[test]
    fn try_extend_empty_batch_is_ok_even_at_capacity() {
        // Boundary: zero remaining capacity admits the empty batch.
        let mut buffer: Refined<Vec<i32>, LenItems<1, 2>> = Refined::try_new(vec![1, 2]).unwrap();
        assert_eq!(buffer.try_extend([]), Ok(()));
        assert_eq!(buffer.as_inner(), &[1, 2]);
    }

    #[test]
    fn try_extend_rejects_whole_batch_on_overflow() {
        let mut buffer: Refined<Vec<i32>, LenItems<1, 3>> = Refined::try_new(vec![1, 2]).unwrap();
        // All-or-nothing: 2 items do not fit in the 1 remaining
        // slot; nothing is committed, the whole batch comes back.
        assert_eq!(
            buffer.try_extend([3, 4]),
            Err(super::CapacityFull {
                rejected: vec![3, 4],
            }),
        );
        assert_eq!(buffer.as_inner(), &[1, 2]);
    }

    #[test]
    fn capacity_full_display_and_error_surface() {
        let err = super::CapacityFull { rejected: 7_i32 };
        assert_eq!(
            err.to_string(),
            "collection is at capacity; rejected input returned to caller",
        );
        // No inner error to chain.
        let dyn_err: &dyn core::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    // ─── map_items (StableUnderElementMap). ──────────────────────

    #[test]
    fn map_items_transfers_length_proof_across_element_types() {
        let files: Refined<Vec<i32>, LenItems<1, 3>> = Refined::try_new(vec![10, 20]).unwrap();
        let labels: Refined<Vec<alloc::string::String>, LenItems<1, 3>> =
            files.map_items(|n| n.to_string());
        assert_eq!(labels.as_inner(), &["10".to_string(), "20".to_string()],);
    }

    #[test]
    fn map_items_through_and_composition() {
        // `And<A, B>: StableUnderElementMap` when both operands are
        // — the composition marker impl is exercised here.
        let r: Refined<Vec<i32>, crate::composition::And<LenItems<1, 5>, LenItems<0, 9>>> =
            Refined::try_new(vec![1, 2, 3]).unwrap();
        let doubled = r.map_items(|n| i64::from(n) * 2);
        assert_eq!(doubled.as_inner(), &[2_i64, 4, 6]);
    }

    #[test]
    fn all_items_accepts_uniform_inner() {
        let r: Refined<Vec<i32>, AllItems<Within<0, 100>>> =
            Refined::try_new(vec![0, 50, 100]).unwrap();
        assert_eq!(r.as_inner(), &[0, 50, 100]);
    }

    #[test]
    fn all_items_reports_index_of_first_violation() {
        // `Within<0, 100>` is a nominal newtype with a flat
        // `NumericError`, so the inner error surfaces unwrapped in
        // the `BadItem` source.
        let result: Result<Refined<Vec<i32>, AllItems<Within<0, 100>>>, _> =
            Refined::try_new(vec![0, 50, 101, 200]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::BadItem {
                index: 2,
                source: NumericError::OutOfRange { value: 101 },
            },
        );
    }

    #[test]
    fn unique_by_key_accepts_distinct_keys() {
        let r: Refined<Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>> =
            Refined::try_new(vec![1, 2, 3, 4]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 3, 4]);
    }

    type UniqueI32 = Refined<alloc::vec::Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>>;

    #[test]
    fn unique_by_key_reports_duplicate_index() {
        let result: Result<UniqueI32, _> = Refined::try_new(vec![1, 2, 1, 3]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::DuplicateKey { index: 2 },
        );
    }

    // ─── Distinct (alias over UniqueByKey<T, IdentityKey<T>>). ───

    #[test]
    fn distinct_admits_unique_values() {
        let r: Refined<Vec<i32>, Distinct<i32>> = Refined::try_new(vec![1, 2, 3]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 3]);
    }

    #[test]
    fn distinct_admits_empty_collection() {
        // No duplicates in an empty collection.
        let r: Refined<Vec<i32>, Distinct<i32>> = Refined::try_new(Vec::new()).unwrap();
        assert!(r.as_inner().is_empty());
    }

    #[test]
    fn distinct_rejects_duplicate() {
        let result: Result<Refined<Vec<i32>, Distinct<i32>>, _> = Refined::try_new(vec![1, 2, 1]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::DuplicateKey { index: 2 },
        );
    }

    // ─── Sorted. ─────────────────────────────────────────────────

    type SortedIdent = Refined<Vec<i32>, Sorted<i32, IdentityKey<i32>>>;

    #[test]
    fn sorted_admits_ascending() {
        let r: SortedIdent = Refined::try_new(vec![1, 2, 3, 5]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 3, 5]);
    }

    #[test]
    fn sorted_admits_equal_adjacent_keys() {
        // Non-strict: equal adjacent keys are admissible.
        let r: SortedIdent = Refined::try_new(vec![1, 2, 2, 5]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 2, 5]);
    }

    #[test]
    fn sorted_admits_empty_collection() {
        // No adjacent pairs to compare; the empty collection is
        // vacuously sorted.
        let r: SortedIdent = Refined::try_new(Vec::new()).unwrap();
        assert!(r.as_inner().is_empty());
    }

    #[test]
    fn sorted_admits_singleton() {
        // No adjacent pairs to compare.
        let r: SortedIdent = Refined::try_new(vec![42]).unwrap();
        assert_eq!(r.as_inner(), &[42]);
    }

    #[test]
    fn sorted_reports_first_inversion_at_index_one() {
        let result: Result<SortedIdent, _> = Refined::try_new(vec![5, 1, 2]);
        assert_eq!(result.unwrap_err(), CollectionError::NotSorted { index: 1 });
    }

    #[test]
    fn sorted_reports_inversion_deep_in_collection() {
        // Six elements: 0..=4 are ascending; index 5 (value 3) is
        // strictly less than index 4 (value 4). The reject points
        // at the later element, index 5.
        let result: Result<SortedIdent, _> = Refined::try_new(vec![0, 1, 2, 3, 4, 3]);
        assert_eq!(result.unwrap_err(), CollectionError::NotSorted { index: 5 });
    }

    // Per-monomorphization Ok-path: a second `KeyOf` extractor
    // exercises a distinct `Sorted<T, K>` monomorphization.
    enum ByTupleSecond {}
    impl super::KeyOf<(i32, i32)> for ByTupleSecond {
        type Key = i32;
        fn key_of(value: &(i32, i32)) -> i32 {
            value.1
        }
    }

    type SortedByTupleSecond = Refined<Vec<(i32, i32)>, Sorted<(i32, i32), ByTupleSecond>>;

    #[test]
    fn sorted_admits_via_custom_key_extractor() {
        // Tuples sorted ascending by the second field. The first
        // field is arbitrary.
        let r: SortedByTupleSecond =
            Refined::try_new(vec![(9, 1), (-3, 2), (0, 2), (1, 5)]).unwrap();
        assert_eq!(r.as_inner(), &[(9, 1), (-3, 2), (0, 2), (1, 5)]);
    }

    #[test]
    fn sorted_rejects_via_custom_key_extractor() {
        // The second-field key inverts at index 2.
        let result: Result<SortedByTupleSecond, _> = Refined::try_new(vec![(0, 1), (0, 5), (0, 3)]);
        assert_eq!(result.unwrap_err(), CollectionError::NotSorted { index: 2 });
    }

    // ─── NoneOf. ─────────────────────────────────────────────────

    #[test]
    fn none_of_admits_when_no_item_matches() {
        let r: Refined<Vec<i32>, NoneOf<IsZero>> = Refined::try_new(vec![1, 2, 3]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 3]);
    }

    #[test]
    fn none_of_admits_empty_collection() {
        // No item is a matching item.
        let r: Refined<Vec<i32>, NoneOf<IsZero>> = Refined::try_new(Vec::new()).unwrap();
        assert!(r.as_inner().is_empty());
    }

    #[test]
    fn none_of_rejects_first_matching_item() {
        let result: Result<Refined<Vec<i32>, NoneOf<IsZero>>, _> =
            Refined::try_new(vec![1, 0, 2, 0]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::MatchingItem { index: 1 },
        );
    }

    // ─── AnyOf. ──────────────────────────────────────────────────

    #[test]
    fn any_of_admits_when_at_least_one_matches() {
        let r: Refined<Vec<i32>, AnyOf<IsZero>> = Refined::try_new(vec![1, 0, 3]).unwrap();
        assert_eq!(r.as_inner(), &[1, 0, 3]);
    }

    #[test]
    fn any_of_rejects_when_no_item_matches() {
        let result: Result<Refined<Vec<i32>, AnyOf<IsZero>>, _> = Refined::try_new(vec![1, 2, 3]);
        assert_eq!(result.unwrap_err(), CollectionError::NoMatchingItem,);
    }

    #[test]
    fn any_of_rejects_empty_collection() {
        let result: Result<Refined<Vec<i32>, AnyOf<IsZero>>, _> = Refined::try_new(Vec::new());
        assert_eq!(result.unwrap_err(), CollectionError::NoMatchingItem,);
    }

    #[test]
    fn display_formats_every_variant() {
        // Hand-rolled `Display` arms — one assertion per variant
        // (the default `EI = Infallible` is used for the non-source
        // variants). The `BadItem` variant carries an inner error
        // and chains it via `Error::source`; the other variants
        // expose no source.
        assert_eq!(
            CollectionError::<NumericError>::LenOutOfRange { actual: 4 }.to_string(),
            "length 4 not in admissible range",
        );
        assert_eq!(
            CollectionError::<NumericError>::DuplicateKey { index: 2 }.to_string(),
            "duplicate key at index 2",
        );
        assert_eq!(
            CollectionError::<NumericError>::MatchingItem { index: 3 }.to_string(),
            "item at index 3 matches a forbidden predicate",
        );
        assert_eq!(
            CollectionError::<NumericError>::NoMatchingItem.to_string(),
            "no item matches the required predicate",
        );
        assert_eq!(
            CollectionError::<NumericError>::NotSorted { index: 1 }.to_string(),
            "element at index 1 breaks ascending order",
        );
        let bad_item = CollectionError::BadItem {
            index: 0,
            source: NumericError::OutOfRange { value: 42_i128 },
        };
        assert_eq!(
            bad_item.to_string(),
            "item at index 0: value 42 not in admissible range",
        );
        // `BadItem` chains the inner error via `Error::source`.
        let dyn_err: &dyn core::error::Error = &bad_item;
        assert!(dyn_err.source().is_some());
        // Other variants do not chain a source — call `source()`
        // on each so every `None`-returning arm of the source-match
        // is exercised.
        let no_source_variants: [CollectionError<NumericError>; 5] = [
            CollectionError::LenOutOfRange { actual: 0 },
            CollectionError::DuplicateKey { index: 0 },
            CollectionError::MatchingItem { index: 0 },
            CollectionError::NoMatchingItem,
            CollectionError::NotSorted { index: 0 },
        ];
        for err in &no_source_variants {
            let dyn_err: &dyn core::error::Error = err;
            assert!(dyn_err.source().is_none());
        }
    }

    #[test]
    fn refinement_macro_unique_short_admits_and_rejects() {
        // Macro-generated newtype: admit a 3-element unique vec,
        // reject a vec with a duplicate.
        let ok = TestUniqueShort::try_new(vec![1, 2, 3]).unwrap();
        assert_eq!(ok.as_inner(), &[1, 2, 3]);
        let owned: Vec<i32> = ok.into_inner();
        assert_eq!(owned, vec![1, 2, 3]);
        let bad = TestUniqueShort::try_new(vec![1, 1, 2]);
        bad.unwrap_err();
    }

    proptest::proptest! {
        #[test]
        fn len_items_round_trips_in_range(
            v in proptest::collection::vec(0_i32..=100_i32, 1_usize..=5_usize)
        ) {
            let r: Refined<Vec<i32>, LenItems<1, 5>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        // ─── LenItems reject: vectors above the cap. ─────────

        #[test]
        fn len_items_rejects_too_long(
            v in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                6_usize..=10_usize,
            )
        ) {
            let actual = v.len();
            let result: Result<Refined<Vec<i32>, LenItems<1, 5>>, _>
                = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::LenOutOfRange { actual },
            );
        }

        // ─── AllItems<Within<0, 100>>. ───────────────────────

        #[test]
        fn all_items_admits_when_every_item_in_range(
            v in proptest::collection::vec(0_i32..=100_i32, 0_usize..=10_usize)
        ) {
            let r: Refined<Vec<i32>, AllItems<Within<0, 100>>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn all_items_rejects_when_any_item_out_of_range(
            head in proptest::collection::vec(0_i32..=100_i32, 0_usize..=5_usize),
            bad in 101_i32..=i32::MAX,
            tail in proptest::collection::vec(0_i32..=100_i32, 0_usize..=5_usize),
        ) {
            // Splice an out-of-range item into the middle so at
            // least one element guarantees rejection; the bad item
            // sits at index head.len().
            let index = head.len();
            let mut v = head;
            v.push(bad);
            v.extend(tail);
            let result: Result<
                Refined<Vec<i32>, AllItems<Within<0, 100>>>,
                _,
            > = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::BadItem {
                    index,
                    source: NumericError::OutOfRange { value: i128::from(bad) },
                },
            );
        }

        // ─── UniqueByKey. ────────────────────────────────────

        #[test]
        fn unique_by_key_admits_distinct_values(
            // Use a small alphabet but enforce uniqueness via
            // BTreeSet collection so duplicates are impossible.
            keys in proptest::collection::btree_set(
                0_i32..=100_i32,
                1_usize..=10_usize,
            )
        ) {
            let v: Vec<i32> = keys.into_iter().collect();
            let r: UniqueI32 = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn unique_by_key_rejects_when_duplicate_present(
            // BTreeSet seed guarantees head is already distinct, so
            // the only duplicate is the appended head[0] at index
            // head.len().
            head in proptest::collection::btree_set(
                0_i32..=100_i32,
                1_usize..=5_usize,
            )
        ) {
            let head: Vec<i32> = head.into_iter().collect();
            let index = head.len();
            let mut v = head.clone();
            v.push(head[0]);
            let result: Result<UniqueI32, _> = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::DuplicateKey { index },
            );
        }

        // ─── NoneOf<IsZero>. ─────────────────────────────────

        #[test]
        fn none_of_admits_when_no_zero_present(
            v in proptest::collection::vec(
                proptest::prop_oneof![1_i32..=100_i32, -100_i32..=-1_i32],
                0_usize..=10_usize,
            )
        ) {
            let r: Refined<Vec<i32>, NoneOf<IsZero>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn none_of_rejects_when_zero_present(
            head in proptest::collection::vec(1_i32..=100_i32, 0_usize..=5_usize),
            tail in proptest::collection::vec(1_i32..=100_i32, 0_usize..=5_usize),
        ) {
            // Head is non-zero by strategy, so the first zero sits
            // at index head.len().
            let index = head.len();
            let mut v = head;
            v.push(0);
            v.extend(tail);
            let result: Result<Refined<Vec<i32>, NoneOf<IsZero>>, _>
                = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::MatchingItem { index },
            );
        }

        // ─── AnyOf<IsZero>. ──────────────────────────────────

        #[test]
        fn any_of_admits_when_zero_present(
            head in proptest::collection::vec(1_i32..=100_i32, 0_usize..=5_usize),
            tail in proptest::collection::vec(1_i32..=100_i32, 0_usize..=5_usize),
        ) {
            let mut v = head;
            v.push(0);
            v.extend(tail);
            let r: Refined<Vec<i32>, AnyOf<IsZero>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn any_of_rejects_when_no_zero_present(
            v in proptest::collection::vec(
                proptest::prop_oneof![1_i32..=100_i32, -100_i32..=-1_i32],
                0_usize..=10_usize,
            )
        ) {
            let result: Result<Refined<Vec<i32>, AnyOf<IsZero>>, _>
                = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::NoMatchingItem,
            );
        }

        // ─── Distinct. ───────────────────────────────────────

        #[test]
        fn distinct_admits_btreeset_values(
            keys in proptest::collection::btree_set(
                0_i32..=100_i32,
                0_usize..=10_usize,
            )
        ) {
            // BTreeSet collection guarantees distinct values.
            let v: Vec<i32> = keys.into_iter().collect();
            let r: Refined<Vec<i32>, Distinct<i32>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn distinct_rejects_when_duplicate_present(
            // BTreeSet seed guarantees head is distinct, so the
            // only duplicate is the appended head[0] at index
            // head.len().
            head in proptest::collection::btree_set(
                0_i32..=100_i32,
                1_usize..=5_usize,
            )
        ) {
            let head: Vec<i32> = head.into_iter().collect();
            let index = head.len();
            let mut v = head.clone();
            v.push(head[0]);
            let result: Result<Refined<Vec<i32>, Distinct<i32>>, _>
                = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::DuplicateKey { index },
            );
        }

        // ─── Sorted. ─────────────────────────────────────────

        #[test]
        fn sorted_admits_after_sort(
            mut v in proptest::collection::vec(0_i32..=100_i32, 0_usize..=10_usize)
        ) {
            v.sort_unstable();
            let r: SortedIdent
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(r.as_inner(), &v);
        }

        #[test]
        fn sorted_rejects_strict_descending(
            head in 1_i32..=100_i32,
            extra in proptest::collection::vec(0_i32..=100_i32, 0_usize..=5_usize),
        ) {
            // Build a vec whose first pair is strictly decreasing
            // (head, 0). The strict drop forces rejection at index 1.
            let mut v = alloc::vec![head, 0];
            v.extend(extra);
            let result: Result<SortedIdent, _> = Refined::try_new(v);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                CollectionError::NotSorted { index: 1 },
            );
        }

        // ─── `ArbitraryRule` for every collection primitive. Each
        //     rule's strategy emits admissible-by-construction
        //     vectors; the carrier is generated through `Refined`'s
        //     blanket `Arbitrary` impl.

        #[test]
        fn arbitrary_len_items_in_range(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, LenItems<1, 5>>>()
        ) {
            proptest::prop_assert!((1..=5).contains(&r.as_inner().len()));
        }

        /// `try_push` happy path: the valid grammar emits vectors
        /// strictly below `MAX`, so every push commits, grows the
        /// length by one, and the result still re-validates.
        #[test]
        fn try_push_below_capacity_always_commits(
            v in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                1_usize..=4_usize,
            ),
            item in proptest::arbitrary::any::<i32>(),
        ) {
            let len = v.len();
            let mut buffer: Refined<Vec<i32>, LenItems<1, 5>>
                = Refined::try_new(v).unwrap();
            proptest::prop_assert_eq!(buffer.try_push(item), Ok(()));
            proptest::prop_assert_eq!(buffer.as_inner().len(), len + 1);
            let revalidated =
                <LenItems<1, 5> as crate::Rule<Vec<i32>>>::refine(buffer.into_inner());
            proptest::prop_assert!(revalidated.is_ok());
        }

        /// `try_push` rejection path: the invalid grammar emits
        /// vectors at exactly `MAX`, so every push is rejected with
        /// the item handed back and the collection untouched.
        #[test]
        fn try_push_at_capacity_always_rejects(
            v in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                5_usize..=5_usize,
            ),
            item in proptest::arbitrary::any::<i32>(),
        ) {
            let mut buffer: Refined<Vec<i32>, LenItems<1, 5>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(
                buffer.try_push(item),
                Err(super::CapacityFull { rejected: item }),
            );
            proptest::prop_assert_eq!(buffer.as_inner(), &v);
        }

        /// `try_extend` happy path: batches sized within the
        /// remaining capacity always commit atomically.
        #[test]
        fn try_extend_within_remaining_capacity_commits(
            v in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                1_usize..=3_usize,
            ),
            batch in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                0_usize..=2_usize,
            ),
        ) {
            let len = v.len();
            let mut buffer: Refined<Vec<i32>, LenItems<1, 5>>
                = Refined::try_new(v).unwrap();
            proptest::prop_assert_eq!(buffer.try_extend(batch.clone()), Ok(()));
            proptest::prop_assert_eq!(buffer.as_inner().len(), len + batch.len());
        }

        /// `try_extend` rejection path: batches strictly larger than
        /// the remaining capacity are rejected whole, with the
        /// collection untouched.
        #[test]
        fn try_extend_over_remaining_capacity_rejects_whole_batch(
            v in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                1_usize..=5_usize,
            ),
            extra in proptest::collection::vec(
                proptest::arbitrary::any::<i32>(),
                0_usize..=3_usize,
            ),
        ) {
            // Fabricate a batch one item longer than the remaining
            // capacity, so rejection is guaranteed by construction.
            let remaining = 5 - v.len();
            let mut batch = extra;
            batch.resize(remaining + 1, 0_i32);
            let mut buffer: Refined<Vec<i32>, LenItems<1, 5>>
                = Refined::try_new(v.clone()).unwrap();
            proptest::prop_assert_eq!(
                buffer.try_extend(batch.clone()),
                Err(super::CapacityFull { rejected: batch }),
            );
            proptest::prop_assert_eq!(buffer.as_inner(), &v);
        }

        /// `map_items` soundness audit: for every admissible source
        /// vector, the mapped vector re-validates under the same
        /// length-only rule — the proof `map_items` transfers
        /// without re-running `refine` is one `refine` would
        /// re-establish.
        #[test]
        fn map_items_output_revalidates_under_rule(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, LenItems<1, 5>>>()
        ) {
            let mapped = r.map_items(i64::from);
            let raw = mapped.into_inner();
            let revalidated =
                <LenItems<1, 5> as crate::Rule<Vec<i64>>>::refine(raw);
            proptest::prop_assert!(revalidated.is_ok());
        }

        #[test]
        fn arbitrary_all_items_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, AllItems<Within<0, 100>>>>()
        ) {
            proptest::prop_assert!(r.as_inner().iter().all(|x| (0..=100).contains(x)));
        }

        #[test]
        fn arbitrary_distinct_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, Distinct<i32>>>()
        ) {
            let v = r.as_inner();
            let mut sorted = v.clone();
            sorted.sort_unstable();
            sorted.dedup();
            proptest::prop_assert_eq!(sorted.len(), v.len());
        }

        #[test]
        fn arbitrary_unique_by_key_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>>>()
        ) {
            let v = r.as_inner();
            let mut sorted = v.clone();
            sorted.sort_unstable();
            sorted.dedup();
            proptest::prop_assert_eq!(sorted.len(), v.len());
        }

        #[test]
        fn arbitrary_sorted_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, Sorted<i32, IdentityKey<i32>>>>()
        ) {
            let v = r.as_inner();
            let in_order = v.iter().zip(v.iter().skip(1)).all(|(a, b)| a <= b);
            proptest::prop_assert!(in_order);
        }

        #[test]
        fn arbitrary_none_of_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, NoneOf<IsZero>>>()
        ) {
            proptest::prop_assert!(!r.as_inner().contains(&0));
        }

        #[test]
        fn arbitrary_any_of_admissible(
            r in proptest::arbitrary::any::<Refined<Vec<i32>, AnyOf<IsZero>>>()
        ) {
            proptest::prop_assert!(r.as_inner().contains(&0));
        }
    }

    // The `UniqueByKey<T, K>` strategy's dedup closure has a False
    // arm (`seen.insert` returns `false` on duplicate keys) that
    // the proptest-driven property tests on `i32` rarely reach —
    // duplicates are vanishingly rare in `any::<i32>()` samples.
    // Call the extracted helper directly with a vec that contains
    // duplicates so both branches are exercised.
    #[cfg(feature = "proptest")]
    #[test]
    fn dedup_by_key_drops_duplicates_preserving_first_occurrence() {
        let deduped = super::dedup_by_key::<i32, IdentityKey<i32>>(vec![1_i32, 2, 1, 3, 2]);
        assert_eq!(deduped, vec![1, 2, 3]);
    }
}
