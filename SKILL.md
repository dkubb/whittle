# whittle

Whittle is a Rust library for parse-don't-validate domain types. You hand it
a raw `T` and a `Rule<T>` marker that names the invariant; if the rule's
`refine` accepts, you get back a `Refined<T, R>` whose existence is the
proof that the invariant held at construction. Downstream code receives the
nominal newtype wrapping that `Refined`, not the bare primitive, so the type
system witnesses what was checked and where. The big idea is a hard split
between the domain surface and the implementation: the newtype is the
domain, the rule composition that produced it is implementation, and the
error surface is a flat domain enum that never leaks `And`/`Or` machinery.
Construction is the single boundary; downstream code trusts the type.

## When to Activate

- The user wants to introduce a domain newtype (identifier, percentage,
  bounded length, hex hash, relative path, non-empty list, ...) and is
  reaching for `String`, `i32`, `Vec<T>` directly.
- The user is hand-rolling `try_new` / `from_str` validators that return
  ad-hoc errors and is about to scatter the same predicate across modules.
- The user wants `serde` to refuse invalid payloads instead of accepting
  them and panicking later, or wants `proptest::Arbitrary` strategies that
  emit valid domain values without `prop_assume!` filtering downstream.
- The user is replacing primitive-typed fields on a struct ("`age: u8`",
  "`name: String`", "`path: String`") with stronger types.
- The user already uses whittle in this repo and is adding another bounded
  type, fixing an error-leak, or stacking transformers / rules.

## When Not to Use

- The invariant is dynamic — depends on runtime configuration, another
  field, a database lookup, or a value the type system cannot witness at
  the boundary. Use a constructor-side check; whittle's `Rule` is a pure
  function on a single value.
- The carrier should mutate in place after construction. Whittle exposes
  only `into_inner` → mutate → `try_new`; there is no `as_mut`.
- The invariant is a multi-field consistency check on a struct. Whittle
  refines one value at a time; cross-field invariants belong in a smart
  constructor on the struct itself.
- You want a `&str` carrier. Whittle's `Rule<T>` requires `T: 'static`;
  every string primitive is `Rule<String>`.
- You want to embed user-friendly localised error text. Whittle errors are
  machine-readable variants with a stable `Display`; localisation is the
  caller's concern.

## Mental Model

`Rule<T>` is a one-method trait: `fn refine(raw: T) -> Result<T, Self::Error>`.
A rule is a narrowing morphism — it takes ownership of the input, may
canonicalise it (lowercase, trim, NFC-normalise), and returns the value on
success or a typed error on rejection. Rules are zero-sized marker types
(`enum NonNeg {}` / `struct Positive`); they carry no instance state and
compose at the type level. See `crates/whittle-core/src/rule.rs:29`.

`Refined<T, R>` is the carrier: a `#[repr(transparent)]` wrapper around `T`
with a `PhantomData<fn() -> R>` tag. Its existence is the proof that
`R::refine` returned `Ok` at construction. The sole public construction
path is `Refined::try_new`, which calls `R::refine`. `serde::Deserialize`
routes through `try_new`. `proptest::Arbitrary` routes through `try_new`.
There is no escape hatch. See `crates/whittle-core/src/rule.rs:122`,
`crates/whittle-core/src/rule.rs:287`, and `crates/whittle-core/src/rule.rs:314`.

Composition is at the type level via `And<A, B>` and `Or<A, B>` (see
`crates/whittle-core/src/composition.rs`). Both rules must share the
same `Rule::Error` type: `And<A, B>` short-circuits on left failure
and returns the shared `E` directly; `Or<A, B>` retries the right
side with a clone of the original input and returns `[E; 2]` when
both reject. There is no positional `Left`/`Right` wrapping in the
composition's `Self::Error` — the rules' flat error enum surfaces to
callers as-is. N-ary `All<(...)>` / `Any<(...)>` operators are
planned follow-up.

Transformers (`AsciiLowercase<R>`, `AsciiUppercase<R>`, `Trim<R>`, see
`crates/whittle-core/src/transform.rs`) normalise before delegating to
the inner rule. The stored carrier is the canonical form, not the input
verbatim — `try_new("ABCD")` and `try_new("abcd")` through
`AsciiLowercase<HexFixedAny<4>>` produce equal `Refined` values.

The load-bearing pattern is: the user-facing type is a hand-written or
macro-generated newtype around `Refined<T, R>`, with a hand-written
flat error enum. The `And`/`Or` composition is the rule machinery
underneath; because both inner rules share an error type, the
composition surfaces a flat enum (or `[E; 2]` for `Or`) that the
newtype's `try_new` maps into named domain variants. The newtype is
the domain; `Refined<T, R>` is implementation.

## Core API

- `Rule<T>` (trait, `crates/whittle-core/src/rule.rs:29`): one method
  `fn refine(raw: T) -> Result<T, Self::Error>`. `Self::Error` is the
  rule's typed rejection. Implementers discharge the soundness obligation:
  `Ok(y)` implies `y` is admissible under the rule. Markers are zero-sized.
- `Refined<T, R>` (carrier, `crates/whittle-core/src/rule.rs:83`):
  `#[repr(transparent)]` over `T`. Methods: `try_new(raw) -> Result<Self,
  R::Error>`, `as_inner(&self) -> &T`, `into_inner(self) -> T`. Forwards
  `Debug`, `Clone`, `Copy`, `Hash`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`
  to `T` with no rule wrapper noise.
- `refinement!` (macro, `crates/whittle-core/src/macros.rs:69`): expands
  `pub Foo: Inner, Rule;` to `pub struct Foo(Refined<Inner, Rule>)` plus
  `try_new`, `as_inner`, `into_inner`. Inherited attrs (`#[derive(...)]`,
  doc comments) pass through to the generated struct. The macro does not
  flatten errors — `Foo::try_new` returns `<Rule as Rule<Inner>>::Error`
  unchanged. When the rule is a composition and you need a flat domain
  error, hand-write the newtype.
- `And<A, B>` (`crates/whittle-core/src/composition.rs`): both rules
  must accept; `A::refine` runs first, output threaded into `B::refine`.
  Both rules must share `Rule::Error = E`. `Self::Error = E` — the
  shared flat enum surfaces directly with no positional wrapping.
- `Or<A, B>` (`crates/whittle-core/src/composition.rs`): either rule
  may accept; on left failure the right rule runs against a clone of the
  original input. Requires `T: Clone`. Both rules must share
  `Rule::Error = E`. `Self::Error = [E; 2]` when both reject — the
  left rejection first, the right rejection second.
- N-ary `All<(...)>` / `Any<(...)>` operators that collapse the
  binary nesting are planned follow-up.
- Transformers (`crates/whittle-core/src/transform.rs`): `AsciiLowercase<R>`,
  `AsciiUppercase<R>`, `Trim<R>`. Each is a `Rule<String>` that normalises
  the input then delegates to `R`; `Self::Error = R::Error`.

## Primitive Catalog

Numeric (`crates/whittle-core/src/primitive/numeric.rs`, `Rule<T: Numeric>`,
all return `NumericError`):

- `Within<MIN, MAX>` — inclusive `MIN..=MAX` (`i128` const generics);
  nominal domain newtype hiding `And<AtLeast, AtMost>`.
- `AtLeast<MIN>` — `MIN <= value`.
- `AtMost<MAX>` — `value <= MAX`.
- `NonZero` — rejects `0`.
- `Positive` — `value > 0`.
- `Negative` — `value < 0`.
- `Numeric` trait — `into_i128` / `from_i128`; implemented for
  `i8..=i128`, `u8..=u64`, `usize`, `isize`. `u128` is intentionally not
  supported (cannot round-trip through `i128`).

String (`crates/whittle-core/src/primitive/string.rs`, `Rule<String>`, all
return `StringError`):

- `LenChars<MIN, MAX>` — inclusive Unicode-scalar-count bound.
- `LenBytes<MIN, MAX>` — inclusive UTF-8 byte-length bound.
- `NonEmpty` — rejects the empty string.
- `EachChar<P>` — every char must satisfy `P: CharPredicate`.
- `FirstChar<P>` — first char (if any) must satisfy `P`. Empty string is
  admitted; compose with a length bound to forbid empty.
- `CharPredicate` trait — `fn test(ch: char) -> bool`.
- Built-in predicates: `AsciiAlphanumeric`, `IdentChar` (alnum or `_`),
  `IdentStart` (alpha or `_`), `IdentDashChar` (alnum, `_`, `-`),
  `NonControl`, `HexChar` (behind `hex`), `PrintableLine` and
  `PrintableMultiline` (behind `unicode`).
- `HexFixedLower<LEN>` (feature `hex`) — exactly `LEN` lowercase hex
  chars; `LEN` must be even (compile-time `const { assert!(...) }`).
- `HexFixedAny<LEN>` (feature `hex`) — exactly `LEN` mixed-case hex chars.
- `HexFixedNormalized<LEN>` (feature `hex`) — type alias for
  `AsciiLowercase<HexFixedAny<LEN>>`; admits any case, stores lowercase.

Float (`crates/whittle-core/src/primitive/float.rs`, `Rule<F: Float>`,
return `FloatError`; `Float` implemented for `f32`, `f64`, sealed):

- `NotNan` — rejects NaN; admits infinities.
- `NotInfinite` — rejects `+/-INF`; admits NaN.
- `Finite` — rejects NaN and infinities (nominal newtype hiding
  `And<NotNan, NotInfinite>`, flat `FloatError`).
- `InClosedRange<MIN_NUM, MIN_DEN, MAX_NUM, MAX_DEN>` — closed range
  written as ratios because Rust 2024 lacks `f64` const generics
  (`InClosedRange<0, 1, 1, 1>` is `0.0..=1.0`).

Collection (`crates/whittle-core/src/primitive/collection.rs`,
`Rule<Vec<T>>`, return `CollectionError` or `CollectionError<EI>`):

- `LenItems<MIN, MAX>` — inclusive item-count bound.
- `AllItems<R>` — every item refined through `R`; error variant is
  `BadItem { index, source: R::Error }`.
- `UniqueByKey<T, K>` — items unique under `K: KeyOf<T>`; reports the
  second occurrence's index.
- `Distinct<T>` — type alias for `UniqueByKey<T, IdentityKey<T>>`.
- `Sorted<T, K>` — non-strict ascending by key (equal adjacent keys
  admissible). Reports the index of the first out-of-order element.
- `NoneOf<P>` — forbid items matching `P: Predicate<T>`.
- `AnyOf<P>` — require at least one item matching `P`.
- `KeyOf<T>` trait — extracts an owned `Ord + Clone` key from `&T`.
- `IdentityKey<T>` — `T` is its own key (requires `T: Ord + Clone`).
- `Predicate<T>` trait — `fn test(&T) -> bool`; distinct from `Rule`
  because predicates only answer yes/no, they neither validate nor
  produce an output.

Path (`crates/whittle-core/src/primitive/path.rs`, `Rule<String>`, returns
`PathError`):

- `RelativePath` — non-empty, no leading `/`, no Windows drive letter or
  UNC prefix, no empty segments (no `//`, no trailing `/`), no `..`
  segments. `PathError::{Empty, Absolute, ParentTraversal{index},
  EmptySegment{index}}`.

## Patterns

### Newtype hiding rule composition (the load-bearing pattern)

When the underlying rule is `And<X, Y, ...>` (or `Or<...>`), wrap it
in a hand-written tuple newtype with a private field and define a
flat domain error enum. Implement `try_new` to call `Refined::try_new`
and map the rules' shared error variants into your flat domain
variants. The composition's `Self::Error` is the rules' shared flat
enum (or `[E; 2]` for `Or`), so the match is a direct 1:1 mapping —
no positional indirection.

Anti-pattern (do not do this):

```rust
// Leaks whittle into the public API: callers must import And, AtLeast,
// AtMost, StringError just to read the error type.
pub type FlightNumber = Refined<String, And<LenChars<3, 7>, EachChar<...>>>;
pub type FlightNumberError = StringError;
```

Pattern:

```rust
// Public surface: nominal type + flat error enum. The inner
// composition is anonymous — only `FlightNumber` is named.
pub struct FlightNumber(
    Refined<String, And<LenChars<3, 7>, EachChar<AsciiAlphanumeric>>>,
);

#[derive(Debug, PartialEq, Eq)]
pub enum FlightNumberError {
    BadLength { actual: usize },
    BadCharacter { offset: usize },
}

// Hand-rolled `Display` + `Error` — whittle is agnostic about
// error-derive macros. `thiserror`, `snafu`, `miette`, or no derive
// at all all work; the `Rule` trait only needs
// `Debug + Display + core::error::Error` on `Rule::Error`.
impl core::fmt::Display for FlightNumberError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::BadLength { actual } =>
                write!(f, "flight number length {actual} not in 3..=7"),
            Self::BadCharacter { offset } =>
                write!(f, "flight number contains non-alphanumeric at byte offset {offset}"),
        }
    }
}
impl core::error::Error for FlightNumberError {}

impl FlightNumber {
    pub fn try_new(raw: String) -> Result<Self, FlightNumberError> {
        Refined::try_new(raw).map(Self).map_err(|err: StringError| match err {
            StringError::CharCountOutOfRange { actual } =>
                FlightNumberError::BadLength { actual },
            StringError::BadChar { offset } =>
                FlightNumberError::BadCharacter { offset },
            // `StringError` is #[non_exhaustive]; the catch-all is
            // required even though the composition only emits the
            // two variants above.
            other => unreachable!("unexpected: {other:?}"),
        })
    }
    // as_inner / into_inner delegate to self.0.
}
```

The user's flight-number example, the kernel's own `Within<MIN, MAX>`
(see `crates/whittle-core/src/primitive/numeric.rs`), and `Finite`
(see `crates/whittle-core/src/primitive/float.rs`) all follow this
shape: domain newtype, flat domain error, the composition's shared
error variants mapped to named variants inside `try_new`.

### Transformers for canonical form

Wrap a validation rule in `AsciiLowercase<R>`, `AsciiUppercase<R>`, or
`Trim<R>` when the storage form should be canonical. The transformer runs
before the inner rule, so:

- The stored carrier is post-transform. `try_new("  Hello  ")` through
  `Trim<AsciiLowercase<NonEmpty>>` stores `"hello"` — the outer `Trim`
  runs first, the inner `AsciiLowercase` runs next, and the validation
  rule `NonEmpty` then sees the canonical form.
- Two inputs that differ only in transformer-equivalent ways (surrounding
  whitespace, letter case, etc.) produce equal `Refined` values.
- Transformers compose with each other (`Trim<AsciiLowercase<R>>`): the
  outer transformer runs first. See
  `crates/whittle-core/src/transform.rs:258`.

The headline use case is `HexFixedNormalized<LEN>` — a type alias for
`AsciiLowercase<HexFixedAny<LEN>>` that accepts hashes in any case and
stores the canonical lowercase form. See
`crates/whittle-core/src/primitive/string.rs:572`.

Tradeoff: silently rewriting input is a different semantic from
validation-only. Use transformers only when canonical form is part of the
contract (hex hashes, hostnames, IANA tokens). For invariants where the
input should be preserved verbatim, use the validation-only rule directly.

### Serde integration

`Refined<T, R>::deserialize` deserialises `T` first and then routes the
raw value through `Refined::try_new`. The rule's `Error` must implement
`Display`; rejections surface as `serde::de::Error::custom(rule_error)`.
This means: there is no admissible code path that produces a `Refined`
without running the rule. Bad JSON is rejected with the rule's own
message; good JSON produces a refined value.

`Refined<T, R>::serialize` forwards to `T` — refined values look identical
on the wire to their underlying primitive.

`#[serde(deny_unknown_fields)]` is `T`'s decision, not whittle's. Serde
does not expose field-level callbacks to outer adapters, so `Refined<T, R>`
cannot enforce `deny_unknown_fields` from outside. Put the attribute on
the inner `T` struct; see the doc comment on `Refined`'s `Deserialize`
impl in `crates/whittle-core/src/rule.rs:262`.

For hand-written newtypes around `Refined`, derive `serde::Deserialize`
on the newtype to forward to `Refined<T, R>::deserialize`. The
`refinement!` macro generates a tuple newtype around `Refined`; serde
derives flow through the same path.

### Property-based testing

With the `proptest` feature, `Refined<T, R>` implements `Arbitrary` for
every `R: ArbitraryRule<T>`. The blanket `Refined<T, R>: Arbitrary` impl
does no rejection sampling — it maps the rule's strategy through
`try_new` and panics on bugs. Each primitive rule supplies a constructive
strategy (range, regex, vec-of-element). Composition retains a bounded
amount of filtering: `And<A, B>`'s strategy filters `A`'s output through
`B::refine`. Place the narrowing rule on the *left* so the filter rate
stays tractable. For sparse intersections, a future n-ary `All<(...)>`
may admit direct intersection generators.

For the primitive rules themselves the strategy is admissible by
construction: `Within<0, 100>` over `i32` (101 values out of 2^32) is as
cheap to sample as `NonZero` (every i32 except 0) because each rule's
strategy targets the admissible region directly.

Downstream tests can write `let r in any::<Refined<T, R>>()` for any
library-supplied rule and trust every generated value satisfies the
invariant — no `prop_assume!` filtering, no narrower-strategy
workarounds.

`ArbitraryRule<T>` is a sub-trait of `Rule<T>` with one method:

```text
trait ArbitraryRule<T>: Rule<T> {
    type Strategy: proptest::strategy::Strategy<Value = T>;
    fn arbitrary_strategy() -> Self::Strategy;
}
```

Implementers carry a soundness obligation: every value emitted by the
returned strategy MUST satisfy `R::refine`. The blanket impl `expect`s
on `try_new`, so a strategy bug surfaces as a panic at test time, not
as silently dropped samples.

Four sub-traits expose building blocks rule strategies need:

- `ArbitraryNumeric` — per-integer-type range strategy. Each numeric
  primitive uses it: `Within<MIN, MAX>` calls
  `T::arbitrary_in_range(MIN, MAX)` to get exactly the admissible region.
- `ArbitraryFloat` — per-float-type strategies (`any`, `finite`,
  closed-range). The float primitives (`NotNan`, `NotInfinite`,
  `Finite`, `InClosedRange`) use these.
- `ArbitraryChar` — per-`CharPredicate` `char` strategy. `EachChar<P>`
  and `FirstChar<P>` compose it into a `String` strategy. Every
  library-supplied predicate (`AsciiAlphanumeric`, `IdentChar`,
  `IdentStart`, `IdentDashChar`, `NonControl`, `HexChar`,
  `PrintableLine`, `PrintableMultiline`) has an `ArbitraryChar` impl.
- `ArbitraryPredicate<T>` — per-`Predicate<T>` value strategy used by
  `AnyOf<P>` to seed the generated collection with a guaranteed match.

Every public `Strategy` associated type resolves to
`proptest::strategy::BoxedStrategy<T>`, so consumers see an opaque
strategy type instead of a concrete combinator stack
(`Map<VecStrategy<CharStrategy<'static>>, fn(...) -> String>`). The
boxing adds one heap allocation per strategy instantiation
(per-property-test, not per-sample) and keeps the public surface
tractable to read.

For a custom rule that wraps the library primitives:

- Delegate to the inner rule's strategy. `refinement! { pub Foo: Inner,
  Rule; }` does not implement `ArbitraryRule` for the newtype; if you
  want `proptest::any::<Foo>()` to work, hand-write `ArbitraryRule<...>`
  on the rule type and call `proptest::strategy::Strategy::prop_map` to
  wrap the inner value in your newtype.
- For composed rules, `And<A, B>` and `Or<A, B>` derive `ArbitraryRule`
  automatically when their components do. `And` uses `A`'s strategy
  filtered through `B::refine`; pick `A` to be the
  generator-shaped rule and `B` to be the predicate-shaped one. `Or`
  is `prop_oneof!`.

Transformers are reflected in the `Arbitrary` distribution: the inner
rule's strategy generates raw `T`, the transformer normalises, and the
stored carrier is the canonical form. Property tests that assert "every
stored value equals its own canonical form" hold by construction.

**Don't use filtering to make sparse rules pass.** When a custom
`Rule<T>` admits only a sparse region of `T` (e.g., a numeric rule
that admits 100 values out of 2³²), the cost of writing a smart
`ArbitraryRule` strategy is critical. The wrong reflex is to define a
generic `Rule<T>` strategy that filters from `T::arbitrary()` — that
is the rejection-sampling pattern that breaks on sparse rules. The
right pattern: encode the admissible-set shape directly. For
range-restricted rules, use proptest's range strategies; for
character-set rules, use `prop_oneof!` over admissible chars; for
collection rules, use `proptest::collection::vec` over the inner
element strategy.

**Transformers need stability proofs.** Wrapping a rule `R` in a
transformer like `Trim<R>`, `AsciiLowercase<R>`, or
`AsciiUppercase<R>` requires `R` to be *stable under the
transformation*: if `R` admits some `v` but rejects `f(v)`, then
`Refined<String, Transform<R>>::try_new(v)` will fail at the strategy
level — the `expect` in the blanket `Arbitrary` impl panics.
Whittle's kernel encodes this with capability marker traits
(`StableUnderTrim`, `StableUnderAsciiLowercase`,
`StableUnderAsciiUppercase`) that each rule implements only when it
genuinely satisfies the property. Custom rule authors should add
their own marker impls for transformers they want to compose with;
`And<A, B>` and `Or<A, B>` carry the marker when both operands do.

### Feature gating

Workspace root `Cargo.toml` lists workspace-level features
(`crates/whittle-core/Cargo.toml`):

- `hex` — enables `HexChar`, `HexFixedLower<LEN>`, `HexFixedAny<LEN>`,
  `HexFixedNormalized<LEN>`. No external deps.
- `unicode` — enables `PrintableLine`, `PrintableMultiline`. No external
  deps; future commits may add `unicode-properties` for fuller `Cf`/`Cn`
  classification.
- `serde` — enables `Serialize`/`Deserialize` impls on `Refined<T, R>`.
- `proptest` — enables `Arbitrary` impl on `Refined<T, R>`.

`default = []`. The crate is `#![no_std]` with `extern crate alloc`. All
features are additive.

## Anti-patterns

- Do not expose `Refined<T, R>` as a public field. It leaks whittle into
  your API surface and freezes the rule's internal structure. Wrap in a
  nominal newtype.
- Do not write `pub type Foo = Refined<T, R>;` for a domain type. Same
  leak as above. Use the `refinement!` macro or hand-write a tuple
  newtype.
- Do not expose the rules' shared error enum directly as your
  domain error. Wrap it in a named domain enum inside `try_new`
  even when both inner rules already share the same flat enum
  (`StringError`, `NumericError`, etc.) — the rename is the
  contract. For `Or<A, B>`, do not expose the raw `[E; 2]` pair;
  collapse it into a single named variant.
- Do not re-validate downstream. The whole point of the carrier is that
  `&Refined<T, R>` witnesses the invariant. If a function takes
  `&Refined<String, R>`, it does not need to re-check the rule. (If you
  feel the urge to re-check, ask whether the invariant is actually fully
  captured by `R`.)
- Do not `#[cfg_attr(coverage_nightly, coverage(off))]` to silence
  coverage. The project forbids this — coverage gaps must be closed with
  real tests. For compile-time const-generic checks, use
  `const { assert!(...) }`; for runtime branches, add a per-
  monomorphization test that exercises both the `Ok` and `Err` paths.
- Do not add a `&str`-based primitive. The kernel's `Rule<T>` requires
  `T: 'static`; every string primitive is `Rule<String>`. `&str`-based
  rules would conflict with the planned `Schema` reflection.
- Do not store mutable inner state on a rule. Rules are zero-sized
  markers; they are addressed by type, not by value.

## Process for adding a new domain type

1. State the invariant in one English sentence. Is it expressible via
   existing whittle primitives (numeric range, string length, character
   class, list uniqueness)? If yes, pick the rule or build it with
   `And` / `Or`.
2. Decide nominal newtype vs bare `Refined<T, R>`. Default: nominal
   newtype, because the newtype IS the domain. Bare `Refined<T, R>` is
   appropriate only for crate-internal helper types that never appear in
   a public signature.
3. Choose the error shape:
   - Inner rule's error is already flat AND domain-meaningful (e.g. you
     used a single primitive like `Within<0, 100>` with its own
     `NumericError`): reuse it.
   - Inner rule is an `And` / `Or` composition: write a flat domain
     enum with `Debug + PartialEq + Eq` plus `Display` + `Error` impls
     (hand-rolled, or via any derive macro you prefer — see the
     "Error derive macros are your choice" note below). For `And<A,
     B>`, the composition's `Self::Error` is the rules' shared flat
     enum, so the match is a flat 1:1 mapping. For `Or<A, B>`, it is
     `[E; 2]` — destructure the array and produce your flat variant.
4. Implement:
   - For single-error rules, `refinement! { pub Foo: Inner, Rule; }` is
     enough — it generates the newtype + `try_new` + `as_inner` +
     `into_inner` and forwards the error unchanged.
   - For composition-flattening, hand-write
     `pub struct Foo(Refined<Inner, Rule>);` plus the flat error enum
     plus a `try_new` that calls `Refined::try_new` and match-flattens
     the error. The `refinement!` macro cannot flatten — that is a
     deliberate limitation; macro complexity does not pay for the corner
     case.
   - Hand-write `Display`, `AsRef`, `From`, etc. as needed. The macro
     does not generate them.
5. Tests:
   - Admit and reject per error variant. For composition-flattening
     newtypes, hit every `match` arm.
   - Property tests through the type's `Arbitrary` strategy (when
     `proptest` is on) — confirm every generated value satisfies the
     invariant. For sparse rules, drive a narrower inner strategy.
   - Doctest in the type's doc comment showing both the admit shape and
     a reject shape. Match the kernel's primitives (e.g.
     `crates/whittle-core/src/primitive/numeric.rs:14`) for style.
6. If the type belongs in `whittle-core` (load-bearing across the
   workspace), add it under `crates/whittle-core/src/primitive/` and
   re-export through `crates/whittle-core/src/primitive/mod.rs`. If it is
   application-domain (`FlightNumber`, `AccountId`), keep it in the
   downstream crate.

**Error derive macros are your choice.** Whittle's kernel is
dep-free — `whittle-core`'s primitive errors (`NumericError`,
`StringError`, `FloatError`, `CollectionError`, `PathError`) are
hand-rolled `impl Display + impl Error`, so downstream `cargo tree`
shows no `thiserror` (or any other error-derive crate) under whittle. The `Rule` trait does NOT require
any specific derive — `Rule::Error` only needs
`Debug + Display + core::error::Error`. Your domain errors can use
`thiserror`, `snafu`, `miette`, or hand-roll — whittle is agnostic.
The test corpus under `tests/` uses `thiserror` for brevity (it is a
workspace `[dev-dependencies]` entry, never a production
dependency), proving the derive integrates cleanly without forcing
it on downstream consumers.

## Examples

See `./tests/` for integration tests that double as runnable examples
covering each pattern. Each file is a self-contained Cargo integration
test binary with a `//!` doc comment explaining what it shows and one or
more `#[test]` functions whose bodies are the demonstration. Run them
with `cargo nextest run --workspace --all-features` or
`cargo test --tests --all-features`. Bare `cargo test` and
`cargo nextest run` also pass: feature-gated integration tests
(`serde-roundtrip`, `proptest-arbitrary`, `hex-and-normalization`) are
declared with `required-features` in the root `Cargo.toml`, so Cargo
skips them when the relevant feature is off. Nextest's profile defaults
live in `.config/nextest.toml`. (If absent, the kernel's own doctests in
`crates/whittle-core/src/primitive/` are the next-best reading list:
every primitive includes admit-and-reject doctests, and the
`Within` / `Finite` newtype-hiding-composition pattern is illustrated by
their own implementations.)

## Validation Checklist

A whittle domain type is well-formed when:

- The invariant is stated in a doc comment on the newtype.
- The newtype is nominal (a struct or `refinement!` invocation), not a
  `pub type` alias to `Refined<T, R>`.
- The inner `Refined` field is private. Construction goes through
  `try_new`; access goes through `as_inner` / `into_inner`.
- The public error type is a flat enum with `Debug + PartialEq + Eq`
  and `Display` + `Error` impls (derived with any macro you prefer or
  hand-written). The rules' shared error enum (or `[E; 2]` for `Or`)
  is mapped to named domain variants — the underlying enum is not
  the public surface.
- Doctests cover at least one admit case and one reject case.
- If `proptest` is on, an `Arbitrary` round-trip test confirms every
  generated value satisfies the invariant.
- If `serde` is on and the type is reachable from a deserialised payload,
  a test confirms invalid input is rejected with the rule's error message.
- Per-monomorphization coverage: every concrete `Rule::refine` impl that
  the type produces is exercised by both an `Ok`-path and an `Err`-path
  test.
- No `coverage(off)` attribute is used to silence missing coverage.
- Downstream code does not re-check the invariant the type already
  witnesses.
