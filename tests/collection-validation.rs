//! Collection primitives: length, items, uniqueness, ordering.
//!
//! Covers `LenItems`, `AllItems`, `Distinct`, `UniqueByKey`,
//! `Sorted`, `NoneOf`, `AnyOf`, plus how to plug a custom
//! `KeyOf` extractor and a custom `Predicate`. The final section
//! shows the load-bearing pattern from the SKILL: a domain
//! newtype that wraps a *composed* collection rule and exposes a
//! flat domain error — the same shape used for scalar newtypes.
//!
//! Use this when a field is a `Vec<T>` whose admissibility depends
//! on its shape (length, ordering, duplicate freedom) rather than
//! the per-item content alone. `AllItems<R>` is the bridge: lift
//! any item-level `Rule<T>` into a `Rule<Vec<T>>`.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::missing_errors_doc,
    missing_docs,
    reason = "integration test: unwrap keeps the focus on the API; pedagogical try_new omits doc"
)]

use whittle::primitive::{
    AllItems, AnyOf, CollectionError, Distinct, IdentityKey, KeyOf, LenItems, NoneOf, NumericError,
    Predicate, Sorted, UniqueByKey, Within,
};
use whittle::{And, Refined};

/// Custom `KeyOf<(i32, i32)>`: project the second tuple field.
enum BySecond {}
impl KeyOf<(i32, i32)> for BySecond {
    type Key = i32;
    fn key_of(value: &(i32, i32)) -> i32 {
        value.1
    }
}

/// Custom `Predicate<i32>`: detect zero.
enum IsZero {}
impl Predicate<i32> for IsZero {
    fn test(value: &i32) -> bool {
        *value == 0
    }
}

/// Opaque order-line identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemId(pub u32);

/// Composed collection rule:
/// - `LenItems<1, 100>` — non-empty, at most 100 items.
/// - `Distinct<ItemId>` — no duplicate IDs.
/// - `Sorted<ItemId, IdentityKey<ItemId>>` — strictly ascending
///   when combined with the distinctness check.
///
/// The order is deliberate: length first, distinctness next,
/// ordering last. Each step assumes the previous step's invariant.
type OrderItemListRule =
    And<LenItems<1, 100>, And<Distinct<ItemId>, Sorted<ItemId, IdentityKey<ItemId>>>>;

/// Nominal domain newtype. The inner `Refined<...>` is private,
/// so the only construction path is `try_new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderItemList(Refined<Vec<ItemId>, OrderItemListRule>);

/// Flat domain error. Each variant names one externally observable
/// failure mode. The rule composition's shared `CollectionError` is
/// mapped into the domain enum inside `try_new`.
///
/// `thiserror` is one option for the `Display` + `Error` impls;
/// whittle does not require any specific derive macro — hand-rolled
/// `impl Display + impl Error` works too.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum OrderItemListError {
    /// Length is outside the admissible range (`1..=100`).
    #[error("order item list length {actual} not in 1..=100")]
    Length { actual: usize },
    /// The item at `index` duplicates an earlier item.
    #[error("order item list has a duplicate at index {index}")]
    Duplicate { index: usize },
    /// The item at `index` breaks ascending order.
    #[error("order item list is out of order at index {index}")]
    OutOfOrder { index: usize },
}

impl OrderItemList {
    /// Validate `raw` and wrap. Every rule in the composition
    /// produces `CollectionError`, so the match is a flat 1:1
    /// mapping into the domain enum — no positional indirection.
    pub fn try_new(raw: Vec<ItemId>) -> Result<Self, OrderItemListError> {
        Refined::try_new(raw).map(Self).map_err(|err: CollectionError<_>| match err {
            CollectionError::LenOutOfRange { actual } => {
                OrderItemListError::Length { actual }
            }
            CollectionError::DuplicateKey { index } => {
                OrderItemListError::Duplicate { index }
            }
            CollectionError::NotSorted { index } => {
                OrderItemListError::OutOfOrder { index }
            }
            // `CollectionError` is `#[non_exhaustive]`, so the
            // catch-all is required even though the composition
            // above can only emit the three variants we just named.
            other => unreachable!("unexpected inner CollectionError variant: {other:?}"),
        })
    }

    /// Borrow the inner vector.
    #[must_use]
    pub fn as_inner(&self) -> &[ItemId] {
        self.0.as_inner()
    }
}

#[test]
fn len_items_bounds_collection_length_inclusively() {
    // `LenItems<MIN, MAX>` bounds the length inclusively.
    let bounded: Refined<Vec<i32>, LenItems<1, 3>> = Refined::try_new(vec![1, 2]).unwrap();
    assert_eq!(bounded.as_inner(), &[1, 2]);
}

#[test]
fn all_items_lifts_item_rule_and_carries_index_with_source_error() {
    // `AllItems<R>` lifts an item-level rule to the collection.
    // `Within<0, 100>` is nominal, so its flat `NumericError` is
    // what the collection's `BadItem` carries as `source`.
    let bad =
        Refined::<Vec<i32>, AllItems<Within<0, 100>>>::try_new(vec![0, 50, 101]).unwrap_err();
    assert_eq!(
        bad,
        CollectionError::BadItem {
            index: 2,
            source: NumericError::OutOfRange { value: 101 },
        },
    );
}

#[test]
fn distinct_admits_unique_items() {
    // `Distinct<T>` is the identity-keyed shorthand. Equivalent
    // to `UniqueByKey<T, IdentityKey<T>>`.
    let distinct: Refined<Vec<i32>, Distinct<i32>> = Refined::try_new(vec![1, 2, 3]).unwrap();
    assert_eq!(distinct.as_inner(), &[1, 2, 3]);
}

#[test]
fn unique_by_key_admits_unique_by_custom_projection() {
    // `UniqueByKey<T, K>` with a custom extractor: deduplicate
    // by the second tuple field. Use a type alias to keep the
    // declaration readable.
    type UniqueBySecond = Refined<Vec<(i32, i32)>, UniqueByKey<(i32, i32), BySecond>>;
    let by_key: UniqueBySecond = Refined::try_new(vec![(1, 10), (2, 20), (3, 30)]).unwrap();
    assert_eq!(by_key.as_inner().len(), 3);
}

#[test]
fn sorted_admits_ascending_order_non_strict() {
    // `Sorted<T, K>` enforces ascending order (non-strict).
    let sorted: Refined<Vec<i32>, Sorted<i32, IdentityKey<i32>>> =
        Refined::try_new(vec![1, 2, 2, 5]).unwrap();
    assert_eq!(sorted.as_inner(), &[1, 2, 2, 5]);
}

#[test]
fn none_of_admits_when_no_item_matches_predicate() {
    // `NoneOf<P>`: forbid any item matching the predicate.
    let no_zeros: Refined<Vec<i32>, NoneOf<IsZero>> = Refined::try_new(vec![1, 2, 3]).unwrap();
    assert_eq!(no_zeros.as_inner(), &[1, 2, 3]);
}

#[test]
fn any_of_admits_when_at_least_one_item_matches_predicate() {
    // `AnyOf<P>`: require at least one item to match.
    let has_zero: Refined<Vec<i32>, AnyOf<IsZero>> = Refined::try_new(vec![1, 0, 2]).unwrap();
    assert_eq!(has_zero.as_inner(), &[1, 0, 2]);
}

#[test]
fn order_item_list_newtype_flattens_composed_collection_error() {
    // ─── Domain newtype around a composed collection rule.  ─────
    //
    // The pattern below is the load-bearing one: a *collection*
    // invariant (bounded length, distinct, sorted) gets the same
    // nominal-newtype-plus-flat-error treatment as a scalar
    // invariant. The composed rule's shared `CollectionError` is
    // an implementation detail; `OrderItemListError` is the public
    // surface.

    let ok = OrderItemList::try_new(vec![ItemId(1), ItemId(2), ItemId(5)]).unwrap();
    assert_eq!(ok.as_inner(), &[ItemId(1), ItemId(2), ItemId(5)]);

    let too_short = OrderItemList::try_new(vec![]).unwrap_err();
    assert_eq!(too_short, OrderItemListError::Length { actual: 0 });

    let duplicate =
        OrderItemList::try_new(vec![ItemId(1), ItemId(2), ItemId(2)]).unwrap_err();
    assert_eq!(duplicate, OrderItemListError::Duplicate { index: 2 });

    let out_of_order =
        OrderItemList::try_new(vec![ItemId(1), ItemId(5), ItemId(2)]).unwrap_err();
    assert_eq!(out_of_order, OrderItemListError::OutOfOrder { index: 2 });
}
