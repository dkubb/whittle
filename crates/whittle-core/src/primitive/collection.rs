//! Collection primitive rules.
//!
//! Length bounds, per-element refinement, and key-based uniqueness
//! for `Vec<T>`. Other collection shapes (`BTreeSet`, `BTreeMap`,
//! custom ordered sets) land in later commits once a real consumer
//! needs them.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::rule::Rule;

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
#[non_exhaustive]
pub enum CollectionError<EI = core::convert::Infallible> {
    /// Length not in the admissible range.
    LenOutOfRange { actual: usize },

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

impl<T, const MIN: usize, const MAX: usize> Rule<Vec<T>> for LenItems<MIN, MAX>
where
    T: 'static,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        const { assert!(MIN <= MAX, "LenItems requires MIN <= MAX") };
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

    #[test]
    fn len_items_inclusive_bounds() {
        let one: Refined<Vec<i32>, LenItems<1, 5>> = Refined::try_new(vec![10]).unwrap();
        assert_eq!(one.as_inner(), &[10]);
        let five: Refined<Vec<i32>, LenItems<1, 5>> =
            Refined::try_new(vec![1, 2, 3, 4, 5]).unwrap();
        assert_eq!(five.as_inner(), &[1, 2, 3, 4, 5]);
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
        assert!(bad.is_err());
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
            let result: Result<Refined<Vec<i32>, LenItems<1, 5>>, _>
                = Refined::try_new(v);
            proptest::prop_assert!(result.is_err());
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
            // least one element guarantees rejection.
            let mut v = head;
            v.push(bad);
            v.extend(tail);
            let result: Result<
                Refined<Vec<i32>, AllItems<Within<0, 100>>>,
                _,
            > = Refined::try_new(v);
            proptest::prop_assert!(result.is_err());
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
            head in proptest::collection::vec(0_i32..=100_i32, 1_usize..=5_usize)
        ) {
            // Append the first element again so the result has at
            // least one duplicate.
            let mut v = head.clone();
            v.push(head[0]);
            let result: Result<UniqueI32, _> = Refined::try_new(v);
            proptest::prop_assert!(result.is_err());
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
            let mut v = head;
            v.push(0);
            v.extend(tail);
            let result: Result<Refined<Vec<i32>, NoneOf<IsZero>>, _>
                = Refined::try_new(v);
            proptest::prop_assert!(result.is_err());
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
            proptest::prop_assert!(result.is_err());
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
            head in proptest::collection::vec(0_i32..=100_i32, 1_usize..=5_usize)
        ) {
            // Splice the first element again so the result has at
            // least one duplicate.
            let mut v = head.clone();
            v.push(head[0]);
            let result: Result<Refined<Vec<i32>, Distinct<i32>>, _>
                = Refined::try_new(v);
            proptest::prop_assert!(result.is_err());
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
            proptest::prop_assert!(result.is_err());
        }
    }
}
