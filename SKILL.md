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
`crates/whittle-core/src/composition.rs:43` and
`crates/whittle-core/src/composition.rs:71`). `And` short-circuits on
left failure; `Or` retries the right side with a clone of the original
input. Their error types are `AndError<EA, EB>` and `OrError<EA, EB>`. These
machinery errors should not appear in your public domain surface.

Transformers (`AsciiLowercase<R>`, `AsciiUppercase<R>`, `Trim<R>`, see
`crates/whittle-core/src/transform.rs`) normalise before delegating to
the inner rule. The stored carrier is the canonical form, not the input
verbatim — `try_new("ABCD")` and `try_new("abcd")` through
`AsciiLowercase<HexFixedAny<4>>` produce equal `Refined` values.

The load-bearing pattern is: the user-facing type is a hand-written or
macro-generated newtype around `Refined<T, R>`, with a hand-written
flat error enum. The `And`/`Or` composition is the rule machinery
underneath; the `AndError`/`OrError` tree is flattened to a domain enum
inside the newtype's `try_new`. The newtype is the domain; `Refined<T, R>`
is implementation.

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
- `And<A, B>` (`crates/whittle-core/src/composition.rs:43`): both rules
  must accept; `A::refine` runs first, output threaded into `B::refine`.
  `Self::Error = AndError<A::Error, B::Error>`.
- `Or<A, B>` (`crates/whittle-core/src/composition.rs:71`): either rule
  may accept; on left failure the right rule runs against a clone of the
  original input. Requires `T: Clone`. `Self::Error = OrError<A::Error,
  B::Error>` (both sides rejected).
- `AndError<EA, EB>` / `OrError<EA, EB>`: machinery error types. Do not
  expose in your public API unless the left/right structure is genuinely
  meaningful — flatten to a domain enum inside `try_new` instead.
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

When the underlying rule is `And<X, Y, ...>` (or anything else that ends
up with a tree-shaped error), wrap it in a hand-written tuple newtype with
a private field and define a flat domain error enum. Implement `try_new`
to call `Refined::try_new` and match-flatten the rule's `AndError` tree
into your flat domain variants.

Anti-pattern (do not do this):

```rust
// Leaks whittle into the public API: callers must import And, AtLeast,
// AtMost, AndError, NumericError just to read the error type.
pub type FlightNumber = Refined<String, And<LenChars<3, 7>, EachChar<...>>>;
pub type FlightNumberError = AndError<StringError, StringError>;
```

Pattern:

```rust
// Public surface: nominal type + flat error enum.
pub struct FlightNumber(Refined<String, FlightNumberRule>);

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
        Refined::try_new(raw).map(Self).map_err(|err| match err {
            AndError::Left(StringError::CharCountOutOfRange { actual }) =>
                FlightNumberError::BadLength { actual },
            AndError::Right(StringError::BadChar { offset }) =>
                FlightNumberError::BadCharacter { offset },
            // ...flatten remaining StringError variants...
        })
    }
    // as_inner / into_inner delegate to self.0.
}
```

The user's flight-number example, the kernel's own `Within<MIN, MAX>`
(see `crates/whittle-core/src/primitive/numeric.rs:285`), and `Finite`
(see `crates/whittle-core/src/primitive/float.rs:230`) all follow this
shape: domain newtype, flat domain error, composition machinery flattened
in `refine` / `try_new`.

### Composition (when `And` IS the domain)

Rare. Only acceptable when the left/right split is itself the domain
contract you want to preserve forever — typically a two-stage parse where
"failed at stage A" and "failed at stage B" are different user-facing
states that should never collapse. Exposing `AndError<EA, EB>` in your
public API freezes the rule's internal structure (you cannot later
restructure the composition without breaking callers), so the default
should be: flatten.

If you genuinely want this, `pub type FooRule = And<X, Y>;` is fine and
`pub type FooError = AndError<X::Error, Y::Error>;` follows. Document the
left/right split as part of the type's contract.

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

With the `proptest` feature, `Refined<T, R>` implements `Arbitrary` as
`T::arbitrary_with(args).prop_filter_map(...)`. Generated values are
guaranteed to satisfy `R` — downstream tests can write
`let r in any::<Refined<T, R>>()` without `prop_assume!` filtering.

The strategy uses rejection sampling, so for sparse rules (`Within<0,
100>` over `i32` admits 101 out of 2^32 values) proptest may exhaust its
retry budget. Two ways out:

- Drive a narrower inner strategy and pipe it through `Refined::try_new`
  manually: `let r: Refined<i32, Within<0, 100>> = Refined::try_new(x)?;`
  where `x in 0..=100`. See
  `crates/whittle-core/src/primitive/numeric.rs:651`.
- Write a custom strategy that emits admissible values directly and have
  the newtype's `Arbitrary` impl use it.

Transformers are reflected in the `Arbitrary` distribution: the inner
strategy generates raw `T`, `try_new` runs the transformer, and the
stored carrier is the canonical form. Property tests that assert "every
stored value equals its own canonical form" hold by construction.

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
- Do not write `pub type FooError = AndError<X, Y>;` for public domain
  error types. Flatten the composition into a named enum via a
  hand-written `try_new` that matches `AndError::Left | Right` into
  domain variants. The rare exception is when the `Left`/`Right`
  positional split genuinely IS the public domain semantic (a two-stage
  parse where "failed at stage A" and "failed at stage B" are
  user-facing states that should never collapse) — and even then,
  prefer a flat enum if you might restructure the rule later, because
  exposing `AndError` freezes the rule's internal shape into the API.
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
     "Error derive macros are your choice" note below). Match the
     `AndError` / `OrError` tree inside `try_new` and produce your
     flat variants.
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

**Error derive macros are your choice.** Whittle itself has no
error-derive dependency: its own primitive errors (`NumericError`,
`StringError`, etc.) are hand-rolled `impl Display + impl Error` so
downstream `cargo tree` shows no `thiserror`. The `Rule` trait does
NOT require any specific derive — `Rule::Error` only needs
`Debug + Display + core::error::Error` for downstream ergonomics. Use
any derive macro you prefer (`thiserror`, `snafu`, `miette`), or
hand-write `impl Display` + `impl Error` — whittle is agnostic.

## Examples

See `./examples/` for runnable Cargo examples covering each pattern.
(If absent, the kernel's own doctests in
`crates/whittle-core/src/primitive/` are the next-best reading list:
every primitive includes admit-and-reject doctests, and the `Within` /
`Finite` newtype-hiding-composition pattern is illustrated by their own
implementations.)

## Validation Checklist

A whittle domain type is well-formed when:

- The invariant is stated in a doc comment on the newtype.
- The newtype is nominal (a struct or `refinement!` invocation), not a
  `pub type` alias to `Refined<T, R>`.
- The inner `Refined` field is private. Construction goes through
  `try_new`; access goes through `as_inner` / `into_inner`.
- The public error type is a flat enum with `Debug + PartialEq + Eq`
  and `Display` + `Error` impls (derived with any macro you prefer or
  hand-written). No `AndError` / `OrError` appears in any public
  signature.
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
