//! Declarative macros that drop the newtype-with-`Refined` boilerplate.

/// Define a refined newtype whose only construction path is
/// `try_new` and whose inner field is private.
///
/// The macro expands to a tuple struct wrapping
/// `Refined<Inner, Rule>` and three inherent methods:
///
/// - `try_new(raw: Inner) -> Result<Self, <Rule as Rule<Inner>>::Error>`
/// - `as_inner(&self) -> &Inner`
/// - `into_inner(self) -> Inner`
///
/// Standard trait impls — `Debug`, `Clone`, `Hash`, `PartialEq`,
/// `Eq`, `PartialOrd`, `Ord`, and `Copy` — are forwarded from
/// `Refined` and selected by the user-supplied `#[derive(...)]`
/// attribute. `Display`, `AsRef`, `From`, Serde, and so on stay
/// hand-written: the macro covers the construction surface
/// without dictating what the carrier looks like beyond it.
///
/// The macro wraps an existing `Inner` type and any Serde
/// `Deserialize` impl is forwarded to `Inner`. If `Inner` is a
/// struct/map type and you want to reject unknown fields, put
/// `#[serde(deny_unknown_fields)]` on `Inner` itself — the macro
/// doesn't generate fielded structs, so it can't attach the
/// attribute. See [`crate::Refined`]'s `Deserialize` impl for the full
/// rationale.
///
/// # Design limit: composed rules and domain error shape
///
/// The macro's generated `try_new` returns the rule's `Error`
/// **unchanged**. Whittle's binary composition operators require
/// both rules to share an `Error` type, so:
///
/// - `And<A, B>` where `A::Error = B::Error = E` produces `E`.
/// - `Or<A, B>` where `A::Error = B::Error = E` produces `[E; 2]`.
///
/// When the inner rule is a single primitive (`NonEmpty`,
/// `Within<MIN, MAX>`, `RelativePath`, and so on) the error is the
/// primitive's flat domain enum (`StringError`, `NumericError`,
/// `PathError`) and the macro is the right tool.
///
/// When the inner rule is an `And<...>` chain whose rules share an
/// error type, the macro is still fine: callers see the shared flat
/// enum directly.
///
/// When the inner rule is `Or<...>`, the macro's `try_new` returns
/// `[E; 2]`. That is informationally complete but rarely the shape a
/// public domain API wants; hand-write the newtype and collapse the
/// pair into a named variant inside `try_new`. See
/// `tests/composition-or.rs` for the pattern.
///
/// # Syntax
///
/// ```text
/// refinement! {
///     #[derive(...)]
///     /// doc comment, attributes, etc.
///     pub Name: InnerType, Rule;
/// }
/// ```
///
/// `InnerType` and `Rule` are separated by a comma because the
/// `ty` macro fragment cannot be followed by the `in` keyword
/// (Rust's macro follow-set rules forbid it). The `:` separates
/// the new type's name from its underlying definition.
///
/// # Example
///
/// Single-primitive rule — the error is the rule's flat enum
/// (`StringError`), no composition tree is exposed.
///
/// ```
/// use whittle_core::primitive::NonEmpty;
/// use whittle_core::refinement;
///
/// refinement! {
///     /// User-supplied display name. Always at least one char.
///     #[derive(Debug, Clone, Hash, PartialEq, Eq)]
///     pub Identifier: String, NonEmpty;
/// }
///
/// // Admit: non-empty input passes the rule.
/// let id = Identifier::try_new("user_42".to_string()).unwrap();
/// assert_eq!(id.as_inner(), "user_42");
///
/// // `into_inner` consumes the wrapper and returns the raw value.
/// let owned: String = id.into_inner();
/// assert_eq!(owned, "user_42");
///
/// // Reject: empty string fails the rule.
/// let bad = Identifier::try_new(String::new());
/// bad.unwrap_err();
/// ```
#[macro_export]
macro_rules! refinement {
    (
        $(#[$attr:meta])*
        $vis:vis $name:ident : $inner:ty, $rule:ty $(;)?
    ) => {
        $(#[$attr])*
        $vis struct $name($crate::Refined<$inner, $rule>);

        impl $name {
            /// Validate `raw` against the rule and wrap.
            ///
            /// # Errors
            ///
            /// Returns the rule's `Error` when `raw` does not
            /// satisfy the refinement.
            #[inline]
            pub fn try_new(
                raw: $inner,
            ) -> ::core::result::Result<
                Self,
                <$rule as $crate::Rule<$inner>>::Error,
            > {
                $crate::Refined::try_new(raw).map(Self)
            }

            /// Borrow the inner value.
            #[inline]
            #[must_use]
            pub const fn as_inner(&self) -> &$inner {
                self.0.as_inner()
            }

            /// Consume the wrapper and return the inner value.
            #[inline]
            #[must_use]
            pub fn into_inner(self) -> $inner {
                self.0.into_inner()
            }
        }
    };
}

/// Implement [`crate::DeserializeRule`] for a rule via the default
/// parse-then-refine path ([`crate::parse_then_refine`]).
///
/// `Refined<T, R>: serde::Deserialize` requires
/// `R: DeserializeRule<'de, T>`; this macro is the one-liner that
/// gives a rule that impl with today's standard behaviour —
/// deserialize the raw `T`, run `Refined::try_new`, and surface
/// rejections through `serde::de::Error::custom`. Rules that bound
/// the *size* of their input (e.g. `LenItems` over `Vec<T>`)
/// hand-write the hook instead so the bound is enforced while the
/// wire value is decoded.
///
/// # Syntax
///
/// ```text
/// deserialize_rule! {
///     impl[<generics>] DeserializeRule<Carrier> for Rule
///     where [<extra bounds>]   // optional
/// }
/// ```
///
/// The macro supplies `Carrier: serde::Deserialize<'de> + 'static`
/// and `Rule::Error: Display` itself; `where [...]` carries whatever
/// additional bounds the rule's own `Rule<Carrier>` impl needs.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::{Refined, Rule, deserialize_rule};
///
/// /// Accepts only multiples of `N`.
/// struct MultipleOf<const N: i64>;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct NotMultiple;
///
/// impl core::fmt::Display for NotMultiple {
///     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
///         f.write_str("not a multiple")
///     }
/// }
///
/// impl<const N: i64> Rule<i64> for MultipleOf<N> {
///     type Error = NotMultiple;
///     fn refine(raw: i64) -> Result<i64, Self::Error> {
///         if raw % N == 0 { Ok(raw) } else { Err(NotMultiple) }
///     }
/// }
///
/// deserialize_rule! {
///     impl[const N: i64] DeserializeRule<i64> for MultipleOf<N>
/// }
///
/// let ok: Refined<i64, MultipleOf<3>> = serde_json::from_str("9").unwrap();
/// assert_eq!(*ok.as_inner(), 9);
/// let err = serde_json::from_str::<Refined<i64, MultipleOf<3>>>("10").unwrap_err();
/// assert!(err.to_string().contains("not a multiple"));
/// # }
/// ```
#[cfg(feature = "serde")]
#[macro_export]
macro_rules! deserialize_rule {
    (
        impl[$($generics:tt)*] DeserializeRule<$carrier:ty> for $rule:ty
        $(where [$($bounds:tt)*])?
        $(;)?
    ) => {
        impl<'de, $($generics)*> $crate::DeserializeRule<'de, $carrier> for $rule
        where
            $carrier: $crate::serde::Deserialize<'de> + 'static,
            <Self as $crate::Rule<$carrier>>::Error: ::core::fmt::Display,
            $($($bounds)*)?
        {
            #[inline]
            fn deserialize_refined<D>(
                deserializer: D,
            ) -> ::core::result::Result<$crate::Refined<$carrier, Self>, D::Error>
            where
                D: $crate::serde::Deserializer<'de>,
            {
                $crate::parse_then_refine::<$carrier, Self, D>(deserializer)
            }
        }
    };
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::primitive::{
        AtLeast, AtMost, EachChar, FirstChar, IdentChar, IdentStart, LenChars, LenItems,
    };
    use crate::{And, Rule};

    refinement! {
        /// Bounded identifier (head: alpha/underscore;
        /// body: alnum/underscore; 1..=64 chars).
        #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
        pub TestIdentifier:
            String,
            And<LenChars<1, 64>,
                And<EachChar<IdentChar>,
                    FirstChar<IdentStart>>>;
    }

    refinement! {
        /// Vec<i32> with 1..=10 items.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub TestVec: Vec<i32>, LenItems<1, 10>;
    }

    refinement! {
        /// Numeric type with Copy support.
        #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
        pub TestBounded: i32, And<AtLeast<0>, AtMost<100>>;
    }

    #[test]
    fn refinement_macro_string_admits_good() {
        let id = TestIdentifier::try_new("user_42".to_string()).unwrap();
        assert_eq!(id.as_inner(), "user_42");
    }

    #[test]
    fn refinement_macro_string_rejects_bad() {
        let bad = TestIdentifier::try_new("1abc".to_string());
        bad.unwrap_err();
    }

    #[test]
    fn refinement_macro_into_inner_returns_owned() {
        let id = TestIdentifier::try_new("name".to_string()).unwrap();
        let owned: String = id.into_inner();
        assert_eq!(owned, "name");
    }

    #[test]
    fn refinement_macro_vec_admits_in_range() {
        let v = TestVec::try_new(vec![1_i32, 2, 3]).unwrap();
        assert_eq!(v.as_inner(), &vec![1, 2, 3]);
        // Exercise into_inner so the generated method isn't
        // dead-coded.
        let owned: Vec<i32> = v.into_inner();
        assert_eq!(owned, vec![1, 2, 3]);
    }

    #[test]
    fn refinement_macro_vec_rejects_overlength() {
        let too_many: Vec<i32> = (0_i32..11_i32).collect();
        let bad = TestVec::try_new(too_many);
        bad.unwrap_err();
    }

    #[test]
    fn refinement_macro_copy_inner_can_be_copy() {
        let n = TestBounded::try_new(42_i32).unwrap();
        let copied = n; // requires Copy
        assert_eq!(*n.as_inner(), 42_i32);
        assert_eq!(*copied.as_inner(), 42_i32);
        // Exercise into_inner so the generated method isn't
        // dead-coded for the Copy-bearing test type.
        let inner: i32 = copied.into_inner();
        assert_eq!(inner, 42_i32);
    }

    #[test]
    fn refinement_macro_copy_rejects_out_of_range() {
        // Both rules share `NumericError`, so the composition's
        // error surfaces directly — no positional wrapping.
        let bad = TestBounded::try_new(200_i32);
        assert_eq!(
            bad.unwrap_err(),
            crate::primitive::NumericError::OutOfRange { value: 200_i128 },
        );
    }

    #[test]
    fn refinement_macro_inner_type_visible_through_rule() {
        // Confirm the rule's Error type is reachable via the
        // standard `Rule` trait path.
        type R = LenItems<1, 10>;
        let err: <R as Rule<Vec<i32>>>::Error =
            crate::primitive::CollectionError::LenOutOfRange { actual: 0 };
        assert_eq!(
            err,
            crate::primitive::CollectionError::LenOutOfRange { actual: 0 },
        );
    }

    proptest::proptest! {
        #[test]
        fn macro_test_bounded_inner_is_in_range(
            x in 0_i32..=100_i32
        ) {
            // Construct the macro-generated newtype through its
            // `try_new` surface and confirm the inner value is
            // in the rule's admissible range.
            let v = TestBounded::try_new(x).unwrap();
            proptest::prop_assert!((0..=100).contains(v.as_inner()));
        }
    }
}
