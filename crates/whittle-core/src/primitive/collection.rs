//! Collection primitive rules.
//!
//! Length bounds, per-element refinement, and key-based uniqueness
//! for `Vec<T>`. Other collection shapes (`BTreeSet`, `BTreeMap`,
//! custom ordered sets) land in later commits once a real consumer
//! needs them.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::marker::PhantomData;

use thiserror::Error;

use crate::rule::Rule;

/// Inclusive bound on the number of items in a `Vec<T>`:
/// `MIN <= len <= MAX`.
pub struct LenItems<const MIN: usize, const MAX: usize>;

/// Every item in the collection must satisfy the inner rule `R`.
pub struct AllItems<R>(PhantomData<R>);

/// Items must be unique under a key derived from each item by the
/// `K: KeyOf<T>` extractor.
///
/// Order is preserved: the first occurrence of each key wins. A
/// second occurrence is reported as
/// `CollectionError::DuplicateKey { index, … }`.
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
    fn key_of(value: &T) -> Self::Key;
}

/// Identity key: `T` is its own ordering key. Requires
/// `T: Ord + Clone + 'static`.
///
/// Useful when the element type is itself an identifier or a
/// fingerprint that should be unique across the collection without
/// projecting a sub-field.
pub struct IdentityKey<T>(PhantomData<T>);

impl<T: 'static + Ord + Clone> KeyOf<T> for IdentityKey<T> {
    type Key = T;
    #[inline]
    fn key_of(value: &T) -> T {
        value.clone()
    }
}

/// Errors common to every collection primitive.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CollectionError<EI = core::convert::Infallible> {
    /// `LenItems<MIN, MAX>` declared with `MIN > MAX`. No collection
    /// is admissible.
    #[error("empty length range")]
    EmptyRange,

    /// Length not in the admissible range.
    #[error("length {actual} not in admissible range")]
    LenOutOfRange { actual: usize },

    /// `AllItems<R>` rejected the item at the given index.
    #[error("item at index {index}: {source}")]
    BadItem {
        /// Position of the rejected item in the original collection.
        index: usize,
        /// The inner rule's error.
        #[source]
        source: EI,
    },

    /// `UniqueByKey<T, K>` saw a duplicate key. The second
    /// occurrence's index is reported; the first wins.
    #[error("duplicate key at index {index}")]
    DuplicateKey {
        /// Position of the duplicate (the second occurrence).
        index: usize,
    },
}

impl<T, const MIN: usize, const MAX: usize> Rule<Vec<T>> for LenItems<MIN, MAX>
where
    T: 'static,
{
    type Error = CollectionError;

    #[inline]
    fn refine(raw: Vec<T>) -> Result<Vec<T>, Self::Error> {
        if MIN > MAX {
            return Err(CollectionError::EmptyRange);
        }
        let actual = raw.len();
        if actual < MIN || actual > MAX {
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::{
        AllItems, CollectionError, IdentityKey, LenItems, UniqueByKey,
    };
    use crate::primitive::{NumericError, Within};
    use crate::rule::Refined;

    #[test]
    fn len_items_inclusive_bounds() {
        let one: Refined<Vec<i32>, LenItems<1, 5>>
            = Refined::try_new(vec![10]).unwrap();
        assert_eq!(one.as_inner(), &[10]);
        let five: Refined<Vec<i32>, LenItems<1, 5>>
            = Refined::try_new(vec![1, 2, 3, 4, 5]).unwrap();
        assert_eq!(five.as_inner(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn len_items_rejects_below_min() {
        let result: Result<Refined<Vec<i32>, LenItems<1, 5>>, _>
            = Refined::try_new(Vec::new());
        assert_eq!(
            result.unwrap_err(),
            CollectionError::LenOutOfRange { actual: 0 },
        );
    }

    #[test]
    fn len_items_rejects_above_max() {
        let result: Result<Refined<Vec<i32>, LenItems<1, 3>>, _>
            = Refined::try_new(vec![1, 2, 3, 4]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::LenOutOfRange { actual: 4 },
        );
    }

    #[test]
    fn len_items_empty_range_rejects_all() {
        let result: Result<Refined<Vec<i32>, LenItems<10, 5>>, _>
            = Refined::try_new(vec![1, 2, 3]);
        assert_eq!(result.unwrap_err(), CollectionError::EmptyRange);
    }

    #[test]
    fn all_items_accepts_uniform_inner() {
        let r: Refined<Vec<i32>, AllItems<Within<0, 100>>>
            = Refined::try_new(vec![0, 50, 100]).unwrap();
        assert_eq!(r.as_inner(), &[0, 50, 100]);
    }

    #[test]
    fn all_items_reports_index_of_first_violation() {
        let result: Result<Refined<Vec<i32>, AllItems<Within<0, 100>>>, _>
            = Refined::try_new(vec![0, 50, 101, 200]);
        assert!(matches!(
            result.unwrap_err(),
            CollectionError::BadItem {
                index: 2,
                source: NumericError::OutOfRange { value: 101 },
            },
        ));
    }

    #[test]
    fn unique_by_key_accepts_distinct_keys() {
        let r: Refined<Vec<i32>, UniqueByKey<i32, IdentityKey<i32>>>
            = Refined::try_new(vec![1, 2, 3, 4]).unwrap();
        assert_eq!(r.as_inner(), &[1, 2, 3, 4]);
    }

    type UniqueI32 = Refined<
        alloc::vec::Vec<i32>,
        UniqueByKey<i32, IdentityKey<i32>>,
    >;

    #[test]
    fn unique_by_key_reports_duplicate_index() {
        let result: Result<UniqueI32, _>
            = Refined::try_new(vec![1, 2, 1, 3]);
        assert_eq!(
            result.unwrap_err(),
            CollectionError::DuplicateKey { index: 2 },
        );
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
    }
}
