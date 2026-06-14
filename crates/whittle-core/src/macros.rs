//! Declarative macros that drop the newtype-with-`Refined` boilerplate.

/// Define a refined newtype whose only construction path is
/// `try_new` and whose inner field is private.
///
/// The macro has two forms.
///
/// - The **simple form** — `pub Name: Inner, Rule;` — expands to a
///   tuple struct wrapping `Refined<Inner, Rule>` whose `try_new`
///   surfaces the rule's `Error` unchanged.
/// - The **error-block form** — the same declaration followed by an
///   `error SourceErr => pub DomainErr { ... }` block — additionally
///   generates a flat domain error enum and maps the rule's error
///   into it, so callers never see the rules' shared primitive enum.
///
/// Both forms generate three inherent methods:
///
/// - `try_new(raw: Inner) -> Result<Self, Error>`
/// - `as_inner(&self) -> &Inner`
/// - `into_inner(self) -> Inner`
///
/// With the `proptest` feature enabled, both forms also implement
/// `proptest::arbitrary::Arbitrary` by forwarding to the inner
/// `Refined` carrier's `Arbitrary` impl when the generated newtype
/// implements `Debug` (a `proptest::Arbitrary` requirement). Generated
/// samples still go through the rule's `ArbitraryRule` strategy and
/// `try_new` path.
///
/// Standard trait impls — `Debug`, `Clone`, `Hash`, `PartialEq`,
/// `Eq`, `PartialOrd`, `Ord`, and `Copy` — are forwarded from
/// `Refined` and selected by the user-supplied `#[derive(...)]`
/// attribute.
///
/// The macro wraps an existing `Inner` type and any Serde
/// `Deserialize` impl is forwarded to `Inner`. If `Inner` is a
/// struct/map type and you want to reject unknown fields, put
/// `#[serde(deny_unknown_fields)]` on `Inner` itself — the macro
/// doesn't generate fielded structs, so it can't attach the
/// attribute. See [`crate::Refined`]'s `Deserialize` impl for the full
/// rationale.
///
/// # Simple form
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
/// The generated `try_new` returns the rule's `Error` **unchanged**.
/// When the inner rule is a single primitive (`NonEmpty`,
/// `Within<MIN, MAX>`, `RelativePath`, and so on) the error is the
/// primitive's flat domain enum (`StringError`, `NumericError`,
/// `PathError`) and the simple form is the right tool. `Display`,
/// `AsRef`, `From`, Serde, and so on stay hand-written in this form:
/// it covers the construction surface without dictating what the
/// carrier looks like beyond it.
///
/// With the `proptest` feature enabled, the generated newtype can be
/// used directly with `proptest::arbitrary::any` whenever its rule has
/// an [`ArbitraryRule`](crate::ArbitraryRule) impl:
///
/// ```
/// # #[cfg(feature = "proptest")] {
/// use proptest::arbitrary::any;
/// use proptest::strategy::{Strategy as _, ValueTree as _};
/// use proptest::test_runner::TestRunner;
/// use whittle_core::primitive::Within;
/// use whittle_core::refinement;
///
/// refinement! {
///     /// Percentage, 0..=100.
///     #[derive(Debug)]
///     pub Percent: i32, Within<0, 100>;
/// }
///
/// let strategy = any::<Percent>();
/// let mut runner = TestRunner::deterministic();
/// let value = strategy.new_tree(&mut runner).unwrap().current();
///
/// assert!((0..=100).contains(value.as_inner()));
/// # }
/// ```
///
/// # Error-block form
///
/// ```text
/// refinement! {
///     #[derive(...)]
///     /// doc comment, attributes, etc.
///     pub Name: InnerType, Rule;
///     impl Display;                        // optional, see below
///
///     /// doc comment, attributes, etc.
///     error SourceError => pub NameError {
///         /// per-variant doc comment
///         SourcePattern => Variant {
///             /// per-field doc comment
///             field: Ty,
///         }: "display text {field}",
///         /// unit variants omit the braces
///         SourcePattern => Variant: "display text",
///         // explicit residual list; omit when the mapping is total
///         unreachable ResidualPattern | ResidualPattern,
///     }
/// }
/// ```
///
/// `SourceError` must be the rule's `Error` type (for an `And<...>`
/// or `All<(...)>` chain, the operands' shared flat enum). Each arm
/// pairs a source pattern with a domain variant: the pattern's
/// bindings must match the variant's declared field names, and the
/// field types are restated on the right because the generated enum
/// needs them. Doc comments pass through per variant **and per
/// field** — the enum is public API, so `missing_docs` applies to
/// both. The string literal after `:` is the variant's `Display`
/// text; inline format captures (`{field}`) bind the variant's
/// fields.
///
/// The expansion emits:
///
/// - the newtype wrapping
///   `Refined<Inner, MapErr<Rule, NameError>>`, with the same three
///   inherent methods as the simple form — `try_new` returns
///   `Result<Self, NameError>` and contains **no** mapping match;
/// - the domain error enum, with `#[derive(Debug, PartialEq, Eq)]`
///   plus the attribute passthrough (add `Clone`, `Hash`, ... there;
///   do **not** add `thiserror::Error` — `Display` and `Error` impls
///   are already emitted, so a thiserror derive is a conflict);
/// - hand-rolled `impl Display` (the per-arm literals) and
///   `impl core::error::Error` for the enum;
/// - `impl ErrorMapper<SourceError> for NameError` with
///   `type Error = Self` — **the enum is its own mapper**, and this
///   impl is the single place the mapping match lives. `try_new`
///   and every other path through the rule (`Refined::try_new`,
///   serde deserialisation, proptest's `ArbitraryRule`) inherit it
///   through the `MapErr` rule;
/// - `impl AsRef<Inner>` borrowing the inner value;
/// - when whittle's `serde` feature is enabled, transparent
///   `Serialize` / `Deserialize` impls forwarding to the inner
///   `Refined`. The wire shape is the bare carrier value, and —
///   because the inner rule is `MapErr<Rule, NameError>` — a
///   deserialize-time rejection renders the **domain** error's
///   `Display` text, not the raw rule text. Do not add
///   `#[derive(serde::Serialize, serde::Deserialize)]` through the
///   attribute passthrough; the impls are already emitted;
/// - with the optional `impl Display;` token, a carrier `Display`
///   forwarding to `Inner`'s. It is opt-in because not every
///   carrier is `Display` (`Vec<i32>` is not).
///
/// # The `unreachable` arm
///
/// A composed rule usually produces only a subset of its error
/// enum's variants. The residual variants must still be matched —
/// whittle's error enums are closed sums — and the `unreachable`
/// arm names them **explicitly**. There is no `_` catch-all, by
/// design: when a source enum gains a variant, every declaration
/// that maps it fails to compile, which is the intended ratchet. A
/// `_` residual is rejected at expansion time, and a residual
/// pattern that repeats a variant already mapped above is a compile
/// error (`unreachable_patterns` is denied inside the generated
/// mapper). When the mapping is total (every source variant has an
/// arm), omit the `unreachable` arm entirely.
///
/// At runtime the residual arm panics; it is unreachable as long as
/// the declared residual list is accurate for the composed rule.
///
/// # Examples
///
/// Simple form: single-primitive rule — the error is the rule's
/// flat enum (`StringError`), no composition tree is exposed.
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
///
/// Error-block form: a composed rule with a flat domain error.
///
/// ```
/// use whittle_core::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};
/// use whittle_core::{And, refinement};
///
/// refinement! {
///     /// IATA-ish flight code: 3..=8 ASCII alphanumeric chars.
///     #[derive(Debug, Clone, PartialEq, Eq, Hash)]
///     pub FlightCode: String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>;
///     impl Display;
///
///     /// Flat domain error for [`FlightCode`].
///     error StringError => pub FlightCodeError {
///         /// Length (in characters) outside `3..=8`.
///         StringError::CharCountOutOfRange { actual } => Length {
///             /// Observed character count.
///             actual: usize,
///         }: "flight code length {actual} not in 3..=8",
///         /// Character at the offset is not ASCII alphanumeric.
///         StringError::BadChar { offset } => BadChar {
///             /// UTF-8 byte offset of the rejected character.
///             offset: usize,
///         }: "flight code character at byte offset {offset} is not ASCII alphanumeric",
///         unreachable StringError::ByteLenOutOfRange { .. }
///             | StringError::Empty
///             | StringError::BadFirstChar
///             | StringError::BadHexLength { .. },
///     }
/// }
///
/// // Admit: `try_new` returns the newtype; `Display` is opt-in.
/// let code = FlightCode::try_new("BA2490".to_string()).unwrap();
/// assert_eq!(code.to_string(), "BA2490");
/// assert_eq!(<FlightCode as AsRef<String>>::as_ref(&code), "BA2490");
///
/// // Reject: the domain enum, never `StringError`.
/// let err = FlightCode::try_new("AB".to_string()).unwrap_err();
/// assert_eq!(err, FlightCodeError::Length { actual: 2 });
/// assert_eq!(err.to_string(), "flight code length 2 not in 3..=8");
/// ```
///
/// With the `serde` feature, rejection at ingress carries the same
/// domain diagnostics as `try_new` — both paths run the one
/// `ErrorMapper` impl:
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::primitive::{LenChars, StringError};
/// use whittle_core::refinement;
///
/// refinement! {
///     /// Display name: 3..=32 chars.
///     #[derive(Debug, Clone, PartialEq, Eq)]
///     pub UserName: String, LenChars<3, 32>;
///
///     /// Flat domain error for [`UserName`].
///     error StringError => pub UserNameError {
///         /// Length (in characters) outside `3..=32`.
///         StringError::CharCountOutOfRange { actual } => Length {
///             /// Observed character count.
///             actual: usize,
///         }: "user name length {actual} not in 3..=32",
///         unreachable StringError::ByteLenOutOfRange { .. }
///             | StringError::Empty
///             | StringError::BadChar { .. }
///             | StringError::BadFirstChar
///             | StringError::BadHexLength { .. },
///     }
/// }
///
/// // Admit: the wire shape is the bare carrier value.
/// let name: UserName = serde_json::from_str(r#""Alice""#).unwrap();
/// assert_eq!(serde_json::to_string(&name).unwrap(), r#""Alice""#);
///
/// // Reject: the ingress message is the domain `Display` text, not
/// // the raw rule text ("character count 2 not in admissible range").
/// let err = serde_json::from_str::<UserName>(r#""AB""#).unwrap_err();
/// assert_eq!(err.to_string(), "user name length 2 not in 3..=32");
/// # }
/// ```
///
/// Two arms cannot target the same domain variant — the macro
/// generates the enum, so the duplicate is a duplicate definition
/// (**compile error**):
///
/// ```compile_fail,E0428
/// use whittle_core::primitive::{NonEmpty, StringError};
///
/// whittle_core::refinement! {
///     /// Newtype under test.
///     #[derive(Debug, Clone, PartialEq, Eq)]
///     pub Dup: String, NonEmpty;
///
///     /// Two arms claim the variant `Bad`.
///     error StringError => pub DupError {
///         /// First claimant.
///         StringError::Empty => Bad: "first",
///         /// Second claimant.
///         StringError::CharCountOutOfRange { .. } => Bad: "second",
///         unreachable StringError::ByteLenOutOfRange { .. }
///             | StringError::BadChar { .. }
///             | StringError::BadFirstChar
///             | StringError::BadHexLength { .. },
///     }
/// }
/// ```
///
/// A residual pattern that repeats a variant already mapped above
/// trips the denied `unreachable_patterns` lint (**compile error**):
///
/// ```compile_fail
/// use whittle_core::primitive::{NonEmpty, StringError};
///
/// whittle_core::refinement! {
///     /// Newtype under test.
///     #[derive(Debug, Clone, PartialEq, Eq)]
///     pub Re: String, NonEmpty;
///
///     /// `Empty` is mapped above AND listed as residual.
///     error StringError => pub ReError {
///         /// Mapped arm.
///         StringError::Empty => Empty: "empty",
///         unreachable StringError::Empty
///             | StringError::CharCountOutOfRange { .. }
///             | StringError::ByteLenOutOfRange { .. }
///             | StringError::BadChar { .. }
///             | StringError::BadFirstChar
///             | StringError::BadHexLength { .. },
///     }
/// }
/// ```
///
/// A source variant that is neither mapped nor listed as residual
/// leaves the generated match non-exhaustive (**compile error**) —
/// this is the ratchet that fires when a source enum gains a
/// variant:
///
/// ```compile_fail,E0004
/// use whittle_core::primitive::{NonEmpty, StringError};
///
/// whittle_core::refinement! {
///     /// Newtype under test.
///     #[derive(Debug, Clone, PartialEq, Eq)]
///     pub Gap: String, NonEmpty;
///
///     /// Maps `Empty` only; the other variants are unhandled.
///     error StringError => pub GapError {
///         /// Mapped arm.
///         StringError::Empty => Empty: "empty",
///     }
/// }
/// ```
///
/// A `_` catch-all residual is rejected at expansion time
/// (**compile error**) — it would silently absorb new source
/// variants:
///
/// ```compile_fail
/// use whittle_core::primitive::{NonEmpty, StringError};
///
/// whittle_core::refinement! {
///     /// Newtype under test.
///     #[derive(Debug, Clone, PartialEq, Eq)]
///     pub Wild: String, NonEmpty;
///
///     /// `unreachable _` defeats the closed-sum ratchet.
///     error StringError => pub WildError {
///         /// Mapped arm.
///         StringError::Empty => Empty: "empty",
///         unreachable _,
///     }
/// }
/// ```
#[macro_export]
macro_rules! refinement {
    // ─── Error-block form, with opt-in carrier `Display`. ────────
    //
    // Delegates to the plain error-block rule below and adds the
    // carrier-forwarding `Display` impl.
    (
        $(#[$attr:meta])*
        $vis:vis $name:ident : $inner:ty, $rule:ty ;
        impl Display ;
        $(#[$eattr:meta])*
        error $source:ty => $evis:vis $error:ident {
            $($body:tt)+
        }
    ) => {
        $crate::refinement! {
            $(#[$attr])*
            $vis $name : $inner, $rule ;
            $(#[$eattr])*
            error $source => $evis $error { $($body)+ }
        }

        impl ::core::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(self.0.as_inner(), f)
            }
        }
    };

    // ─── Error-block form. ────────────────────────────────────────
    //
    // The newtype wraps `Refined<Inner, MapErr<Rule, Error>>`, so
    // `try_new` needs no mapping match: the `ErrorMapper` impl
    // emitted by the `@error_block` muncher is the single
    // determinant of the rule-to-domain mapping, and every path
    // through the rule inherits it.
    (
        $(#[$attr:meta])*
        $vis:vis $name:ident : $inner:ty, $rule:ty ;
        $(#[$eattr:meta])*
        error $source:ty => $evis:vis $error:ident {
            $($body:tt)+
        }
    ) => {
        $(#[$attr])*
        $vis struct $name($crate::Refined<$inner, $crate::MapErr<$rule, $error>>);

        impl $name {
            /// Validate `raw` against the rule and wrap.
            ///
            /// # Errors
            ///
            /// Returns the domain error the declaration's `error`
            /// block maps the rule's rejection into.
            #[inline]
            pub fn try_new(raw: $inner) -> ::core::result::Result<Self, $error> {
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

        impl ::core::convert::AsRef<$inner> for $name {
            #[inline]
            fn as_ref(&self) -> &$inner {
                self.0.as_inner()
            }
        }

        $crate::refinement! {
            @error_block
            attrs = [$(#[$eattr])*],
            vis = [$evis],
            error = [$error],
            source = [$source],
            formatter = [f],
            variants = [],
            displays = [],
            maps = [],
            $($body)+
        }

        $crate::__refinement_serde!($name, $inner, $rule, $error);
        $crate::__refinement_arbitrary!($name, $inner, $crate::MapErr<$rule, $error>);
    };

    // ─── Internal: error-block muncher. ───────────────────────────
    //
    // Walks the arm list, accumulating the enum variants, the
    // `Display` match arms, and the `ErrorMapper` match arms in
    // parallel, then hands the three streams to `@error_items`.
    // `formatter` threads the `f` ident from a single transcription
    // so the accumulated `write!` arms and the eventual `fmt`
    // signature share one hygiene context.

    // Reject a `_` catch-all residual: it would silently absorb new
    // source variants, defeating the closed-sum ratchet.
    (
        @error_block
        attrs = [$(#[$eattr:meta])*],
        vis = [$evis:vis],
        error = [$error:ident],
        source = [$source:ty],
        formatter = [$f:ident],
        variants = [$($variants:tt)*],
        displays = [$($displays:tt)*],
        maps = [$($maps:tt)*],
        unreachable _ $(,)?
    ) => {
        ::core::compile_error!(
            "refinement!: `unreachable` requires the explicit residual source-variant list; a `_` catch-all would silently absorb new source variants"
        );
    };

    // Terminal: explicit residual list. The residual arm completes
    // the generated match without a catch-all, so a new source
    // variant is a compile error in every declaration.
    (
        @error_block
        attrs = [$(#[$eattr:meta])*],
        vis = [$evis:vis],
        error = [$error:ident],
        source = [$source:ty],
        formatter = [$f:ident],
        variants = [$($variants:tt)*],
        displays = [$($displays:tt)*],
        maps = [$($maps:tt)*],
        unreachable $residual:pat $(,)?
    ) => {
        $crate::refinement! {
            @error_items
            attrs = [$(#[$eattr])*],
            vis = [$evis],
            error = [$error],
            source = [$source],
            formatter = [$f],
            variants = [$($variants)*],
            displays = [$($displays)*],
            maps = [
                $($maps)*
                $residual => ::core::unreachable!(
                    "refinement! error mapping: the rule composition cannot produce this source-error variant"
                ),
            ],
        }
    };

    // Terminal: total mapping — every source variant has an arm, so
    // there is no residual and the match is exhaustive as-is.
    (
        @error_block
        attrs = [$(#[$eattr:meta])*],
        vis = [$evis:vis],
        error = [$error:ident],
        source = [$source:ty],
        formatter = [$f:ident],
        variants = [$($variants:tt)*],
        displays = [$($displays:tt)*],
        maps = [$($maps:tt)*],
    ) => {
        $crate::refinement! {
            @error_items
            attrs = [$(#[$eattr])*],
            vis = [$evis],
            error = [$error],
            source = [$source],
            formatter = [$f],
            variants = [$($variants)*],
            displays = [$($displays)*],
            maps = [$($maps)*],
        }
    };

    // Step: one mapped arm. The variant's declared field idents are
    // reused as the construction shorthand, so they must match the
    // source pattern's bindings — a mismatch is an unresolved-name
    // compile error at the declaration.
    (
        @error_block
        attrs = [$(#[$eattr:meta])*],
        vis = [$evis:vis],
        error = [$error:ident],
        source = [$source:ty],
        formatter = [$f:ident],
        variants = [$($variants:tt)*],
        displays = [$($displays:tt)*],
        maps = [$($maps:tt)*],
        $(#[$vattr:meta])*
        $pat:pat => $variant:ident $({
            $($(#[$fattr:meta])* $field:ident : $fty:ty),+ $(,)?
        })? : $display:literal
        $(, $($rest:tt)*)?
    ) => {
        $crate::refinement! {
            @error_block
            attrs = [$(#[$eattr])*],
            vis = [$evis],
            error = [$error],
            source = [$source],
            formatter = [$f],
            variants = [
                $($variants)*
                $(#[$vattr])*
                $variant $({ $($(#[$fattr])* $field: $fty),+ })?,
            ],
            displays = [
                $($displays)*
                Self::$variant $({ $($field),+ })? => ::core::write!($f, $display),
            ],
            maps = [
                $($maps)*
                $pat => Self::$variant $({ $($field),+ })?,
            ],
            $($($rest)*)?
        }
    };

    // ─── Internal: emit the error-enum item set. ──────────────────
    (
        @error_items
        attrs = [$(#[$eattr:meta])*],
        vis = [$evis:vis],
        error = [$error:ident],
        source = [$source:ty],
        formatter = [$f:ident],
        variants = [$($variants:tt)*],
        displays = [$($displays:tt)*],
        maps = [$($maps:tt)*],
    ) => {
        $(#[$eattr])*
        #[derive(Debug, PartialEq, Eq)]
        $evis enum $error {
            $($variants)*
        }

        impl ::core::fmt::Display for $error {
            #[inline]
            fn fmt(&self, $f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    $($displays)*
                }
            }
        }

        impl ::core::error::Error for $error {}

        // The domain enum is its own `ErrorMapper`: the mapping
        // match below is the single determinant every construction
        // and deserialisation path inherits through `MapErr`.
        // `unreachable_patterns` is denied so a residual pattern
        // that repeats a mapped variant is a compile error.
        impl $crate::ErrorMapper<$source> for $error {
            type Error = Self;

            #[deny(unreachable_patterns)]
            #[inline]
            fn map_error(error: $source) -> Self::Error {
                match error {
                    $($maps)*
                }
            }
        }
    };

    // ─── Simple form (unchanged surface). ─────────────────────────
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

        $crate::__refinement_arbitrary!($name, $inner, $rule);
    };
}

/// Internal `refinement!` helper: emits the transparent `Serialize`
/// / `Deserialize` impls for an error-block newtype. Defined as a
/// separate macro so its expansion follows **whittle's** own `serde`
/// feature (resolved when whittle-core is compiled) rather than a
/// feature of the downstream crate expanding `refinement!`.
///
/// `Serialize` forwards to the inner `Refined` (the wire shape is
/// the bare carrier value). `Deserialize` forwards to
/// `Refined<Inner, MapErr<Rule, Error>>::deserialize`, so a
/// rejection at ingress renders the **domain** error's `Display`
/// text — the same diagnostics `try_new` returns, because both paths
/// share the one `ErrorMapper` impl.
#[cfg(feature = "serde")]
#[doc(hidden)]
#[macro_export]
macro_rules! __refinement_serde {
    ($name:ident, $inner:ty, $rule:ty, $error:ident) => {
        impl $crate::serde::Serialize for $name {
            #[inline]
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: $crate::serde::Serializer,
            {
                $crate::serde::Serialize::serialize(&self.0, serializer)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            #[inline]
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: $crate::serde::Deserializer<'de>,
            {
                let refined: $crate::Refined<$inner, $crate::MapErr<$rule, $error>> =
                    $crate::serde::Deserialize::deserialize(deserializer)?;
                ::core::result::Result::Ok(Self(refined))
            }
        }
    };
}

/// Internal `refinement!` helper: no-op arm used when whittle's
/// `serde` feature is disabled.
#[cfg(not(feature = "serde"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __refinement_serde {
    ($name:ident, $inner:ty, $rule:ty, $error:ident) => {};
}

/// Implement `serde::Serialize` by projecting a domain value into an
/// explicit flat field list.
///
/// This macro is **egress-only**: it emits no `Deserialize` impl and
/// does not change the type's construction path. Use it when a domain
/// wrapper stores refined tuple or struct carriers internally but the
/// wire shape must stay a flat object with a known field order.
///
/// The `as |value|` receiver is required because a declarative item
/// macro cannot capture method `self` hygienically from the call site.
/// Each projection expression is serialized directly, so normal serde
/// semantics apply (`Option::None` becomes `null`, borrowed strings are
/// not cloned, and no fields are skipped unless the projection itself
/// encodes that behaviour).
///
/// # Syntax
///
/// ```text
/// serialize_flat! {
///     impl Serialize for Type as |value| {
///         "field_name" => value.projected_field(),
///     }
/// }
/// ```
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use whittle_core::{Refined, Rule, serialize_flat};
///
/// /// Accepts a non-zero token lifetime.
/// enum ValidToken {}
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct TokenError;
///
/// impl Rule<(u64, u64, Option<String>)> for ValidToken {
///     type Error = TokenError;
///
///     fn refine(
///         raw: (u64, u64, Option<String>),
///     ) -> Result<(u64, u64, Option<String>), Self::Error> {
///         if raw.1 > 0 { Ok(raw) } else { Err(TokenError) }
///     }
/// }
///
/// struct Token {
///     fields: Refined<(u64, u64, Option<String>), ValidToken>,
/// }
///
/// impl Token {
///     fn try_new(
///         created_at: u64,
///         expires_in: u64,
///         refresh_token: Option<String>,
///     ) -> Result<Self, TokenError> {
///         Refined::try_new((created_at, expires_in, refresh_token))
///             .map(|fields| Self { fields })
///     }
/// }
///
/// serialize_flat! {
///     impl Serialize for Token as |token| {
///         "created_at" => token.fields.as_inner().0,
///         "expires_in" => token.fields.as_inner().1,
///         "refresh_token" => token.fields.as_inner().2.as_ref(),
///     }
/// }
///
/// let token = Token::try_new(1_700_000_000, 3_600, None).unwrap();
/// let json = serde_json::to_string(&token).unwrap();
///
/// assert_eq!(
///     json,
///     r#"{"created_at":1700000000,"expires_in":3600,"refresh_token":null}"#,
/// );
/// # }
/// ```
#[cfg(feature = "serde")]
#[macro_export]
macro_rules! serialize_flat {
    (
        impl Serialize for $ty:ty as |$receiver:ident| {
            $($field:literal => $value:expr),* $(,)?
        }
    ) => {
        impl $crate::serde::Serialize for $ty {
            #[inline]
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: $crate::serde::Serializer,
            {
                let $receiver = self;
                let field_count = $crate::__serialize_flat_field_count!($($field),*);
                let mut state = serializer.serialize_struct(stringify!($ty), field_count)?;

                $(
                    $crate::serde::ser::SerializeStruct::serialize_field(&mut state, $field, &$value)?;
                )*

                $crate::serde::ser::SerializeStruct::end(state)
            }
        }
    };
}

/// Internal `serialize_flat!` helper: count field literals without
/// evaluating projection expressions.
#[cfg(feature = "serde")]
#[doc(hidden)]
#[macro_export]
macro_rules! __serialize_flat_field_count {
    ($($field:literal),* $(,)?) => {
        <[()]>::len(&[$($crate::__serialize_flat_field_count!(@unit $field)),*])
    };
    (@unit $field:literal) => {
        ()
    };
}

/// Internal `refinement!` helper: emits an `Arbitrary` impl for a
/// generated newtype by forwarding to its inner `Refined` carrier.
///
/// The generated strategy is still rule-derived: `Refined<Inner,
/// Rule>`'s blanket `Arbitrary` impl consumes `Rule`'s
/// [`ArbitraryRule`](crate::ArbitraryRule) strategy, runs `try_new`,
/// and panics if that strategy violates its contract.
#[cfg(feature = "proptest")]
#[doc(hidden)]
#[macro_export]
macro_rules! __refinement_arbitrary {
    ($name:ident, $inner:ty, $rule:ty) => {
        impl $crate::proptest::arbitrary::Arbitrary for $name
        where
            $name: ::core::fmt::Debug,
            $inner: ::core::fmt::Debug + 'static,
            $rule: $crate::ArbitraryRule<$inner> + 'static,
        {
            type Parameters = ();
            type Strategy = $crate::proptest::strategy::BoxedStrategy<Self>;

            #[inline]
            fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
                use $crate::proptest::strategy::Strategy as _;
                <$crate::Refined<$inner, $rule> as $crate::proptest::arbitrary::Arbitrary>::arbitrary_with(())
                    .prop_map(Self)
                    .boxed()
            }
        }
    };
}

/// Internal `refinement!` helper: no-op arm used when whittle's
/// `proptest` feature is disabled.
#[cfg(not(feature = "proptest"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __refinement_arbitrary {
    ($name:ident, $inner:ty, $rule:ty) => {};
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

/// Define a closed-set enum from a single declaration: each variant
/// paired with its wire string, everything else derived.
///
/// This is `refinement!`-class **declarative codegen**: the macro
/// generates the enum itself, the [`ClosedSet`](crate::ClosedSet)
/// impl whose `MEMBERS` table is the single determinant, the
/// standard derives, and the forwarding impls — it does not merely
/// validate and forward an existing type. Generating enum and table
/// from one declaration list makes "variant without a wire string",
/// "wire string without a variant", and "variant declared twice"
/// unrepresentable in the declaration artifact itself.
///
/// The expansion emits:
///
/// - the enum, with
///   `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]`
///   (the full forwarded set `refinement!` documents; do not
///   re-derive these via the attribute passthrough);
/// - `impl ClosedSet` with the `MEMBERS` table in declaration order;
/// - a `const` forcing [`ClosedSet::VALID`](crate::ClosedSet::VALID)
///   at declaration time, so a duplicate wire string is a compile
///   error on the declaration itself;
/// - `FromStr` and `TryFrom<&str>` forwarding to
///   [`closed_set::parse`](crate::closed_set::parse), and `Display`
///   forwarding to [`closed_set::as_str`](crate::closed_set::as_str);
/// - an inherent `schema()` returning
///   an `Enumerated` schema
///   ([`SchemaView::Enumerated`](crate::schema::SchemaView::Enumerated)) with
///   the wire-string labels in declaration order — the constructive
///   description of the closed set, derived from the same
///   declaration list as the `MEMBERS` table (do not define another
///   inherent `schema` on the enum);
/// - when whittle's `serde` feature is enabled, `Serialize` /
///   `Deserialize` impls forwarding to the closed-set codec — the
///   wire shape is the plain wire string, serialization is the
///   table's wire form, and deserialization routes untrusted
///   ingress through `parse` so rejections carry the domain
///   diagnostics. Do not add `#[derive(serde::Serialize)]` /
///   `#[derive(serde::Deserialize)]` through the attribute
///   passthrough; the impls are already emitted.
///
/// # Syntax
///
/// ```text
/// closed_set! {
///     /// doc comment, attributes, etc.
///     pub enum Name {
///         /// per-variant docs/attributes
///         Variant = "wire-string",
///         ...
///     }
/// }
/// ```
///
/// # Examples
///
/// The bank-integration `ActivityStatus` shape — one declaration,
/// parse and wire form derived:
///
/// ```
/// use whittle_core::closed_set;
///
/// closed_set! {
///     /// Account activity status.
///     pub enum ActivityStatus {
///         /// The account is in active use.
///         Active = "active",
///         /// The account is dormant.
///         Inactive = "inactive",
///     }
/// }
///
/// // Admit: `FromStr` routes through `closed_set::parse`.
/// let status: ActivityStatus = "active".parse().unwrap();
/// assert_eq!(status, ActivityStatus::Active);
///
/// // `Display` is the wire form (`closed_set::as_str`).
/// assert_eq!(status.to_string(), "active");
///
/// // `TryFrom<&str>` is the same boundary morphism.
/// let by_try: ActivityStatus = "inactive".try_into().unwrap();
/// assert_eq!(by_try, ActivityStatus::Inactive);
///
/// // The schema is the declared label set, in declaration order.
/// assert_eq!(
///     ActivityStatus::schema(),
///     whittle_core::schema::Schema::enumerated(&["active", "inactive"]),
/// );
///
/// // Reject: exact error contents — the bounded offending value
/// // and the expected set borrowed from the MEMBERS table.
/// let err = "actve".parse::<ActivityStatus>().unwrap_err();
/// assert_eq!(err.value(), "actve");
/// assert_eq!(err.expected(), <ActivityStatus as whittle_core::ClosedSet>::MEMBERS);
/// assert_eq!(
///     err.to_string(),
///     r#"invalid value "actve": expected one of "active", "inactive""#,
/// );
/// ```
///
/// A duplicate wire string is a **compile error** (the `VALID`
/// side condition, forced at declaration time):
///
/// ```compile_fail
/// whittle_core::closed_set! {
///     /// Two variants cannot share a wire string.
///     pub enum Dup {
///         /// First claimant.
///         A = "same",
///         /// Second claimant.
///         B = "same",
///     }
/// }
/// ```
///
/// A duplicate variant is a **compile error** (the macro generates
/// the enum, so the duplicate is a duplicate definition):
///
/// ```compile_fail
/// whittle_core::closed_set! {
///     /// A variant cannot be declared twice.
///     pub enum Dup {
///         /// First declaration.
///         A = "first",
///         /// Duplicate declaration.
///         A = "second",
///     }
/// }
/// ```
#[macro_export]
macro_rules! closed_set {
    (
        $(#[$attr:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$vattr:meta])*
                $variant:ident = $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$attr])*
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord
        )]
        $vis enum $name {
            $(
                $(#[$vattr])*
                $variant
            ),+
        }

        impl $crate::ClosedSet for $name {
            const MEMBERS: &'static [(&'static str, Self)] = &[
                $(($wire, Self::$variant)),+
            ];
        }

        // Force the injectivity side condition at declaration time
        // rather than first use.
        const _: () = <$name as $crate::ClosedSet>::VALID;

        impl $name {
            /// Constructive schema of the closed set: the wire-string
            /// labels in declaration order, as an `Enumerated` schema
            /// node. Derived from the same declaration list as the
            /// `ClosedSet::MEMBERS` table, so the two cannot drift.
            #[must_use]
            pub fn schema() -> $crate::schema::Schema {
                $crate::schema::Schema::enumerated(&[$($wire),+])
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::ClosedSetError<Self>;

            #[inline]
            fn from_str(raw: &str) -> ::core::result::Result<Self, Self::Err> {
                $crate::closed_set::parse(raw)
            }
        }

        impl ::core::convert::TryFrom<&str> for $name {
            type Error = $crate::ClosedSetError<Self>;

            #[inline]
            fn try_from(raw: &str) -> ::core::result::Result<Self, Self::Error> {
                $crate::closed_set::parse(raw)
            }
        }

        impl ::core::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str($crate::closed_set::as_str(*self))
            }
        }

        $crate::__closed_set_serde!($name);
    };
}

/// Internal `closed_set!` helper: emits the `Serialize` /
/// `Deserialize` impls forwarding to the closed-set codec. Defined
/// as a separate macro so its expansion follows **whittle's** own
/// `serde` feature (resolved when whittle-core is compiled) rather
/// than a feature of the downstream crate expanding `closed_set!`.
#[cfg(feature = "serde")]
#[doc(hidden)]
#[macro_export]
macro_rules! __closed_set_serde {
    ($name:ident) => {
        impl $crate::serde::Serialize for $name {
            #[inline]
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: $crate::serde::Serializer,
            {
                $crate::closed_set::serialize(self, serializer)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            #[inline]
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: $crate::serde::Deserializer<'de>,
            {
                $crate::closed_set::deserialize(deserializer)
            }
        }
    };
}

/// Internal `closed_set!` helper: no-op arm used when whittle's
/// `serde` feature is disabled.
#[cfg(not(feature = "serde"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __closed_set_serde {
    ($name:ident) => {};
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
        AsciiAlphanumeric, AtLeast, AtMost, CollectionError, EachChar, FirstChar, IdentChar,
        IdentStart, LenChars, LenItems, NumericError, StringError, Within,
    };
    use crate::{And, ErrorMapper, Rule};

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

    // ─── Error-block form fixtures. ────────────────────────────────
    //
    // Three declarations so every generated surface is exercised
    // across distinct monomorphisations: a string composition with a
    // residual list, a numeric total mapping, and a non-`Display`
    // collection carrier (which must omit `impl Display;`).

    refinement! {
        /// Flight-code shape: 3..=8 ASCII alphanumeric chars.
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub TestCode: String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>;
        impl Display;

        /// Flat domain error for [`TestCode`].
        error StringError => pub TestCodeError {
            /// Length (in characters) outside `3..=8`.
            StringError::CharCountOutOfRange { actual } => Length {
                /// Observed character count.
                actual: usize,
            }: "code length {actual} not in 3..=8",
            /// Character at `offset` is not ASCII alphanumeric.
            StringError::BadChar { offset } => BadChar {
                /// UTF-8 byte offset of the rejected character.
                offset: usize,
            }: "code character at byte offset {offset} not ASCII alphanumeric",
            unreachable StringError::ByteLenOutOfRange { .. }
                | StringError::Empty
                | StringError::BadFirstChar
                | StringError::BadHexLength { .. },
        }
    }

    refinement! {
        /// Percentage score: 0..=100.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub TestScore: i32, Within<0, 100>;
        impl Display;

        /// Flat domain error for [`TestScore`]. `Within` emits only
        /// `OutOfRange`, so the mapping is total: no `unreachable`
        /// arm.
        error NumericError => pub TestScoreError {
            /// Value outside `0..=100`.
            NumericError::OutOfRange { value } => OutOfRange {
                /// Offending value widened into `i128`.
                value: i128,
            }: "score {value} not in 0..=100",
        }
    }

    refinement! {
        /// Roster of 1..=3 player ids. The carrier (`Vec<i32>`) is
        /// not `Display`, so the declaration omits `impl Display;`.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub TestRoster: Vec<i32>, LenItems<1, 3>;

        /// Flat domain error for [`TestRoster`] (unit variant).
        error CollectionError => pub TestRosterError {
            /// Item count outside `1..=3`.
            CollectionError::LenOutOfRange { .. } => BadLength:
                "roster needs 1 to 3 players",
            unreachable CollectionError::BadItem { .. }
                | CollectionError::DuplicateKey { .. }
                | CollectionError::MatchingItem { .. }
                | CollectionError::NoMatchingItem
                | CollectionError::NotSorted { .. },
        }
    }

    #[test]
    fn refinement_error_block_try_new_admits_good_input() {
        let code = TestCode::try_new("BA2490".to_string()).unwrap();
        assert_eq!(code.as_inner(), "BA2490");
        let score = TestScore::try_new(42_i32).unwrap();
        assert_eq!(*score.as_inner(), 42_i32);
        let roster = TestRoster::try_new(vec![7_i32, 9]).unwrap();
        assert_eq!(roster.as_inner(), &vec![7_i32, 9]);
    }

    #[test]
    fn refinement_error_block_try_new_rejects_with_domain_error() {
        // Each mapped arm of each declaration: the caller sees the
        // domain enum, never the source enum.
        let too_short = TestCode::try_new("AB".to_string()).unwrap_err();
        assert_eq!(too_short, TestCodeError::Length { actual: 2 });
        let bad_char = TestCode::try_new("BA 490".to_string()).unwrap_err();
        assert_eq!(bad_char, TestCodeError::BadChar { offset: 2 });
        let out_of_range = TestScore::try_new(101_i32).unwrap_err();
        assert_eq!(out_of_range, TestScoreError::OutOfRange { value: 101 });
        let empty = TestRoster::try_new(Vec::new()).unwrap_err();
        assert_eq!(empty, TestRosterError::BadLength);
    }

    #[test]
    fn refinement_error_block_into_inner_returns_owned() {
        let code = TestCode::try_new("BA2490".to_string()).unwrap();
        let owned: String = code.into_inner();
        assert_eq!(owned, "BA2490");
        let score = TestScore::try_new(42_i32).unwrap();
        assert_eq!(score.into_inner(), 42_i32);
        let roster = TestRoster::try_new(vec![7_i32]).unwrap();
        let players: Vec<i32> = roster.into_inner();
        assert_eq!(players, vec![7_i32]);
    }

    #[test]
    fn refinement_error_block_as_ref_borrows_inner() {
        let code = TestCode::try_new("BA2490".to_string()).unwrap();
        let s: &String = code.as_ref();
        assert_eq!(s, "BA2490");
        let score = TestScore::try_new(42_i32).unwrap();
        let n: &i32 = score.as_ref();
        assert_eq!(*n, 42_i32);
        let roster = TestRoster::try_new(vec![7_i32]).unwrap();
        let players: &Vec<i32> = roster.as_ref();
        assert_eq!(players, &vec![7_i32]);
    }

    #[test]
    fn refinement_error_block_opt_in_display_forwards_to_carrier() {
        // `impl Display;` is the opt-in token; both opted
        // declarations forward to the carrier's `Display`.
        let code = TestCode::try_new("BA2490".to_string()).unwrap();
        assert_eq!(code.to_string(), "BA2490");
        let score = TestScore::try_new(42_i32).unwrap();
        assert_eq!(score.to_string(), "42");
    }

    #[test]
    fn refinement_error_block_error_display_uses_declared_literals() {
        // Every variant of every generated enum renders its
        // declared literal, with inline captures bound to fields.
        assert_eq!(
            TestCodeError::Length { actual: 2 }.to_string(),
            "code length 2 not in 3..=8",
        );
        assert_eq!(
            TestCodeError::BadChar { offset: 2 }.to_string(),
            "code character at byte offset 2 not ASCII alphanumeric",
        );
        assert_eq!(
            TestScoreError::OutOfRange { value: 101 }.to_string(),
            "score 101 not in 0..=100",
        );
        assert_eq!(
            TestRosterError::BadLength.to_string(),
            "roster needs 1 to 3 players"
        );
    }

    #[test]
    fn refinement_error_block_error_implements_error_trait() {
        // Hand-rolled `Display` + emitted `core::error::Error`, so
        // the enums work with `?`, `anyhow`, and stdlib machinery.
        let _: &dyn core::error::Error = &TestCodeError::Length { actual: 2 };
        let _: &dyn core::error::Error = &TestScoreError::OutOfRange { value: 101 };
        let _: &dyn core::error::Error = &TestRosterError::BadLength;
    }

    #[cfg(feature = "serde")]
    struct TestFlatToken {
        fields: (u64, Option<&'static str>),
    }

    #[cfg(feature = "serde")]
    serialize_flat! {
        impl Serialize for TestFlatToken as |token| {
            "created_at" => token.fields.0,
            "refresh_token" => token.fields.1,
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialize_flat_macro_writes_flat_fields_in_order() {
        let token = TestFlatToken {
            fields: (1_700_000_000, None),
        };

        serde_test::assert_ser_tokens(
            &token,
            &[
                serde_test::Token::Struct {
                    name: "TestFlatToken",
                    len: 2,
                },
                serde_test::Token::Str("created_at"),
                serde_test::Token::U64(1_700_000_000),
                serde_test::Token::Str("refresh_token"),
                serde_test::Token::None,
                serde_test::Token::StructEnd,
            ],
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialize_flat_macro_propagates_serialize_struct_error() {
        let token = TestFlatToken {
            fields: (1_700_000_000, None),
        };

        serde_test::assert_ser_tokens_error(
            &token,
            &[serde_test::Token::Bool(true)],
            r#"expected Token::Bool(true) but serialized as Struct { name: "TestFlatToken", len: 2, }"#,
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialize_flat_macro_propagates_serialize_field_error() {
        let token = TestFlatToken {
            fields: (1_700_000_000, None),
        };

        serde_test::assert_ser_tokens_error(
            &token,
            &[
                serde_test::Token::Struct {
                    name: "TestFlatToken",
                    len: 2,
                },
                serde_test::Token::Str("wrong"),
            ],
            r#"expected Token::Str("wrong") but serialized as Str("created_at")"#,
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialize_flat_macro_propagates_later_serialize_field_error() {
        let token = TestFlatToken {
            fields: (1_700_000_000, None),
        };

        serde_test::assert_ser_tokens_error(
            &token,
            &[
                serde_test::Token::Struct {
                    name: "TestFlatToken",
                    len: 2,
                },
                serde_test::Token::Str("created_at"),
                serde_test::Token::U64(1_700_000_000),
                serde_test::Token::Str("wrong"),
            ],
            r#"expected Token::Str("wrong") but serialized as Str("refresh_token")"#,
        );
    }

    #[test]
    #[should_panic(expected = "cannot produce this source-error variant")]
    fn refinement_error_block_string_residual_arm_panics() {
        // The residual arm is unreachable through `try_new`; calling
        // the mapper directly is the only way to exercise it.
        TestCodeError::map_error(StringError::Empty);
    }

    #[test]
    #[should_panic(expected = "cannot produce this source-error variant")]
    fn refinement_error_block_collection_residual_arm_panics() {
        TestRosterError::map_error(CollectionError::NoMatchingItem);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn refinement_error_block_serde_round_trips_the_bare_carrier() {
        // The emitted impls are transparent: the wire shape is the
        // carrier value, with no rule-marker noise.
        let code = TestCode::try_new("BA2490".to_string()).unwrap();
        let code_json = serde_json::to_string(&code).unwrap();
        assert_eq!(code_json, r#""BA2490""#);
        let code_back: TestCode = serde_json::from_str(&code_json).unwrap();
        assert_eq!(code_back, code);

        let score = TestScore::try_new(42_i32).unwrap();
        let score_json = serde_json::to_string(&score).unwrap();
        assert_eq!(score_json, "42");
        let score_back: TestScore = serde_json::from_str(&score_json).unwrap();
        assert_eq!(score_back, score);

        let roster = TestRoster::try_new(vec![7_i32, 9]).unwrap();
        let roster_json = serde_json::to_string(&roster).unwrap();
        assert_eq!(roster_json, "[7,9]");
        let roster_back: TestRoster = serde_json::from_str(&roster_json).unwrap();
        assert_eq!(roster_back, roster);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn refinement_error_block_serde_rejects_with_domain_text_at_ingress() {
        // Deserialize-time rejection surfaces the DOMAIN `Display`
        // string — the `MapErr` mapping runs at ingress — not the
        // raw rule text ("character count 2 not in admissible
        // range"). Exact-match assertions pin the full message.
        let code_err = serde_json::from_str::<TestCode>(r#""AB""#).unwrap_err();
        assert_eq!(code_err.to_string(), "code length 2 not in 3..=8");
        let score_err = serde_json::from_str::<TestScore>("101").unwrap_err();
        assert_eq!(score_err.to_string(), "score 101 not in 0..=100");
        let roster_err = serde_json::from_str::<TestRoster>("[]").unwrap_err();
        assert_eq!(roster_err.to_string(), "roster needs 1 to 3 players");
    }

    proptest::proptest! {
        #[test]
        fn refinement_error_block_admits_entire_declared_range(
            x in 0_i32..=100_i32
        ) {
            // The `MapErr` wrapper must not change the admissible
            // set — only the error codomain.
            let score = TestScore::try_new(x).unwrap();
            proptest::prop_assert!((0..=100).contains(score.as_inner()));
        }
    }

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn refinement_simple_form_arbitrary_forwards_inner_refined(
            value in proptest::arbitrary::any::<TestBounded>()
        ) {
            proptest::prop_assert!((0..=100).contains(value.as_inner()));
        }

        #[test]
        fn refinement_error_block_arbitrary_forwards_mapped_rule(
            value in proptest::arbitrary::any::<TestScore>()
        ) {
            proptest::prop_assert!((0..=100).contains(value.as_inner()));
        }
    }

    closed_set! {
        /// Account activity status (macro-generated test fixture).
        pub enum TestActivityStatus {
            /// In active use.
            Active = "active",
            /// Dormant.
            Inactive = "inactive",
        }
    }

    closed_set! {
        /// Branch code: a second, distinct monomorphisation of the
        /// generic closed-set fns through the macro front door.
        pub enum TestBranch {
            /// Main branch.
            Main = "main",
            /// Satellite branch.
            Satellite = "satellite",
        }
    }

    #[test]
    fn closed_set_macro_from_str_admits_members() {
        let status: TestActivityStatus = "active".parse().unwrap();
        assert_eq!(status, TestActivityStatus::Active);
        let branch: TestBranch = "satellite".parse().unwrap();
        assert_eq!(branch, TestBranch::Satellite);
    }

    #[test]
    fn closed_set_macro_from_str_rejects_non_members() {
        let err = "actve".parse::<TestActivityStatus>().unwrap_err();
        assert_eq!(err.value(), "actve");
        "trunk".parse::<TestBranch>().unwrap_err();
    }

    #[test]
    fn closed_set_macro_try_from_routes_through_parse() {
        let status = TestActivityStatus::try_from("inactive").unwrap();
        assert_eq!(status, TestActivityStatus::Inactive);
        TestActivityStatus::try_from("paused").unwrap_err();
        let branch = TestBranch::try_from("main").unwrap();
        assert_eq!(branch, TestBranch::Main);
        TestBranch::try_from("MAIN").unwrap_err();
    }

    #[test]
    fn closed_set_macro_display_is_the_wire_form() {
        assert_eq!(TestActivityStatus::Active.to_string(), "active");
        assert_eq!(TestActivityStatus::Inactive.to_string(), "inactive");
        assert_eq!(TestBranch::Main.to_string(), "main");
        assert_eq!(TestBranch::Satellite.to_string(), "satellite");
    }

    #[test]
    fn closed_set_macro_members_follow_declaration_order() {
        assert_eq!(
            <TestActivityStatus as crate::ClosedSet>::MEMBERS,
            &[
                ("active", TestActivityStatus::Active),
                ("inactive", TestActivityStatus::Inactive),
            ],
        );
    }

    #[test]
    fn closed_set_macro_schema_is_the_declared_label_set() {
        assert_eq!(
            TestActivityStatus::schema(),
            crate::schema::Schema::enumerated(&["active", "inactive"]),
        );
        assert_eq!(
            TestBranch::schema(),
            crate::schema::Schema::enumerated(&["main", "satellite"]),
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn closed_set_macro_serde_round_trips_the_plain_wire_string() {
        let json = serde_json::to_string(&TestActivityStatus::Active).unwrap();
        assert_eq!(json, r#""active""#);
        let back: TestActivityStatus = serde_json::from_str(r#""inactive""#).unwrap();
        assert_eq!(back, TestActivityStatus::Inactive);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn closed_set_macro_serde_rejects_non_members_at_ingress() {
        let err = serde_json::from_str::<TestActivityStatus>(r#""actve""#).unwrap_err();
        assert!(
            err.to_string()
                .contains(r#"invalid value "actve": expected one of "active", "inactive""#),
        );
    }

    #[test]
    fn closed_set_macro_derives_ord_in_declaration_order() {
        // The emitted derive set includes `PartialOrd`/`Ord` (and
        // `Hash`), matching the standard forwarded set documented
        // on `refinement!`.
        assert!(TestActivityStatus::Active < TestActivityStatus::Inactive);
        let mut branches = alloc::vec![TestBranch::Satellite, TestBranch::Main];
        branches.sort();
        assert_eq!(
            branches,
            alloc::vec![TestBranch::Main, TestBranch::Satellite]
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
