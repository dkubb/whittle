# Whittle Architecture Specification

Status of This Memo

This document is an internal project specification written in an RFC-style
Markdown form. The document borrows structure and editorial discipline from
RFC 7322 and uses RFC 2026 as process vocabulary for maturity, review, and
applicability.

This document is the concrete architecture for Whittle. It is derived from
[IDEA.md](IDEA.md), which is authoritative for goals, scope, non-goals,
and invariants. When this document conflicts with [IDEA.md](IDEA.md),
[IDEA.md](IDEA.md) takes precedence.

Section 16 is non-normative sequencing guidance. Section 17 is a
non-normative open-issue list. Staged decisions MUST NOT weaken the
invariants in [IDEA.md](IDEA.md).

Abstract

This document specifies the architecture for the Whittle library: a Rust
parse-don't-validate engine that narrows raw input into refined values at
construction time and propagates the resulting proofs through ordinary
Rust types. The architecture uses a Cargo workspace with one facade crate
and three member crates: a pure core crate holding the `Rule` trait,
the `Refined` carrier, the contextual-rule companion, the implication
trait, the library-supplied primitive rules, and (behind a `proptest`
feature) the `proptest::Strategy` derivation; a proc-macro crate
hosting the `refinement!` declarative macro and `#[derive(Refined)]`
derive; and a workspace-internal integration-test crate. `serde`,
`schemars`, `sqlx`, and `proptest` are integration modules exposed on
the facade crate behind Cargo features; keeping the `Arbitrary` impl
inside `whittle-core` (rather than a separate `whittle-arbitrary`
crate) avoids Rust's orphan-rule violation for the foreign
`Arbitrary` trait on the foreign `Refined<T, R>` type.

Table of Contents

- [Section 1: Introduction](#1-introduction)
- [Section 2: Requirements Language](#2-requirements-language)
- [Section 3: Sources](#3-sources)
- [Section 4: Foundations](#4-foundations)
- [Section 5: Toolchain and Gates](#5-toolchain-and-gates)
- [Section 6: Crate and Module Shape](#6-crate-and-module-shape)
- [Section 7: Dependency Direction](#7-dependency-direction)
- [Section 8: Core Traits](#8-core-traits)
- [Section 9: The Refined Carrier](#9-the-refined-carrier)
- [Section 10: Library-Supplied Primitive Rules](#10-library-supplied-primitive-rules)
- [Section 11: Contextual Rules](#11-contextual-rules)
- [Section 12: Schema Reflection and Derived Integrations](#12-schema-reflection-and-derived-integrations)
- [Section 13: Implication and Subtyping](#13-implication-and-subtyping)
- [Section 14: The Refinement Macro](#14-the-refinement-macro)
- [Section 15: Testing Architecture](#15-testing-architecture)
- [Section 16: Build Sequence](#16-build-sequence)
- [Section 17: Open Issues](#17-open-issues)
- [Section 18: References](#18-references)

## 1. Introduction

Whittle is a Rust library that turns untrusted raw values into refined
values through a single user-defined rule per refinement. The refinement
runs at construction time; once a refined value exists, downstream code
trusts it without further checks. The library provides the kernel (`Rule`,
`Refined`, implication, contextual variants), a set of primitive rules
for common cases, a declarative macro that emits named refinements from
one source of truth, and derived integrations for property testing,
serialization, JSON Schema, and SQL row decoding.

This document specifies the concrete mechanisms that realize the
requirements in [IDEA.md](IDEA.md). The architecture is a Technical
Specification in the RFC 2026 sense: it describes concrete procedures,
conventions, and formats for this library.

## 2. Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here.

Lowercase uses of these words have their ordinary English meanings.

## 3. Sources

The authoritative source is:

- [IDEA.md](IDEA.md), which is authoritative for goals, scope,
  non-goals, and invariants.

Reference inputs are:

- the `refined` crate, whose `Predicate::test(&T) -> bool` shape we
  deliberately diverge from in favour of consume-and-rebuild narrowing;
- the `witnessed` crate, whose contextual-witness pattern (a
  `WitnessIn<T, Env>` trait with borrowed and owned environment
  carriers) we adopt;
- the `branded` crate, whose nominal-newtype derive structure informs
  the macro's generated surface (`Display`, `AsRef`, `Deref`, optional
  `serde`/`sqlx` derivation);
- the Effect.Schema library, whose schema-as-value design informs the
  reflectable-schema architecture in Section 12;
- the [DESIGN.md](DESIGN.md) sketch that preceded this document, which
  this document supersedes for architectural commitments.

## 4. Foundations

The implementation uses these concrete foundations:

- Language: Rust, edition 2024.
- Toolchain: pinned per commit via `rust-toolchain.toml`.
- Workspace shape: one Cargo workspace containing a thin facade package
  at the workspace root and three member crates under `crates/`.
- Async runtime: none. Whittle is synchronous; the constructor surface
  is `fn`, not `async fn`.
- Serialization: `serde` with derive support, behind a Cargo feature.
- JSON Schema: `schemars`, behind a Cargo feature.
- SQL row decoding: `sqlx`, behind a Cargo feature.
- Property testing: `proptest`, behind a Cargo feature. `quickcheck`
  support is a future extension.
- Numeric representation for decimals: `rust_decimal`, behind a Cargo
  feature.
- Time representation: `chrono` with `clock` and `serde`, behind a
  Cargo feature.
- Bounded collections: `bounded-vec`, `non-empty-string`, shared across
  crates via `workspace.dependencies`.
- Error handling: `thiserror` for typed error definitions.

Local `whittle-core` domain types built on the foundations above include
the `Rule` trait, the `Refined<T, R>` carrier, `RuleWith<T, Env>`, the
`RefinedWith` family, the `Implies` trait, the `Schema` reflection enum,
and the library-supplied primitive rule markers. These are part of the
crate's public surface, not external dependencies.

### 4.1. Constants

Numeric limits referenced throughout the architecture live in
`whittle-core::limits` as `pub const` items. The initial values satisfy
the requirements of [IDEA.md](IDEA.md):

| Constant | Value | Unit | Used in |
| --- | --- | --- | --- |
| `MAX_STRING_LEN` | 65536 | UTF-8 bytes | default `LenBytes` cap |
| `MAX_COLLECTION_LEN` | 65536 | items | default `LenItems` cap |
| `MAX_ENUM_VARIANTS` | 256 | variants | `subset_of!` admit list cap |
| `MAX_RULE_DEPTH` | 32 | nesting levels | `And`/`Or` nesting cap |
| `MAX_SCHEMA_NODES` | 4096 | nodes | `Schema` tree size cap |

Implementations MAY raise these constants; they MUST NOT lower any
constant below a value that breaks a documented integration test.

## 5. Toolchain and Gates

The local gate vocabulary is provided by `just` and cargo aliases:

- `just fmt-check` runs `cargo fmt --all --check`.
- `just lint` runs the `clippy-all` alias, which expands to
  `cargo clippy --all-features --all-targets --tests --workspace`.
- `just test` runs the `test-all` alias (nextest under the hood) plus
  `cargo test --doc --workspace --all-features`.
- `just docs` runs `mado check` against the committed Markdown
  documents using the repository's `.mado.toml` configuration.
- `just deny` runs `cargo deny check` against `.cargo/deny.toml`.
- `just coverage` runs `cargo llvm-cov` and asserts zero uncovered
  regions, functions, or lines.
- `just ci` runs the full gate: `check deny`, where `check` itself is
  `fmt-check lint test docs coverage`.

Lint posture: every default Clippy lint, plus `pedantic`, `nursery`,
`cargo`, and `restriction`, is denied. Every Rustdoc lint is denied.
Suppressions MUST use `#![expect(LINT, reason = "…")]` with a reason
string. The unfulfilled-lint-expectations lint is denied so an `expect`
whose lint does not fire becomes a build failure.

Dependency-license posture: `.cargo/deny.toml` allows the standard
permissive set (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception,
BSD-3-Clause, ISC, Unicode-3.0, BSL-1.0, CC0-1.0, CDLA-Permissive-2.0,
0BSD, Unlicense, Zlib). Unknown registries are denied. Unknown git
sources are denied.

## 6. Crate and Module Shape

The repository layout is:

```text
whittle/
├── Cargo.toml                  workspace + thin facade package
├── justfile                    gate recipes
├── rust-toolchain.toml
├── .mado.toml                  Markdown lint configuration
├── .cargo/
│   ├── clippy.toml             disallowed-methods configuration
│   ├── config.toml             cargo aliases + build flags
│   └── deny.toml               license allowlist, registry rules
├── src/lib.rs                  re-exports core + selected features
├── tests/                      integration tests exercising the feature matrix
├── crates/
│   └── whittle-core/           Rule, Refined, refinement!, primitives, Schema, Implies
└── docs/
    ├── README.md
    ├── IDEA.md
    ├── ARCHITECTURE.md
    └── DESIGN.md
```

`whittle-schemars`, `whittle-sqlx`, and `whittle-serde` integrations are
Cargo features on the root facade crate rather than separate member
crates; their public surface lives in `whittle::integrations` (with
sub-modules `serde`, `schemars`, `sqlx`) and is conditionally compiled.

## 7. Dependency Direction

The dependency graph MUST be one-way through the facade boundary:

- `whittle-core` depends on `serde` (optional), `thiserror`,
  `rust_decimal` (optional), `chrono` (optional), `bounded-vec`,
  `non-empty-string`. It MUST NOT depend on any other whittle crate.
  The `refinement!` declarative macro lives in `whittle-core::macros`;
  there is no separate proc-macro crate.
- The `proptest::Strategy` derivation and the `Arbitrary` impl for
  `Refined<T, R>` live inside `whittle-core` behind a `proptest`
  Cargo feature. They cannot live in a separate crate without an
  orphan-rule violation, because both the `Arbitrary` trait
  (foreign, from `proptest`) and `Refined<T, R>` would be foreign
  to that crate.
- The root facade `whittle` re-exports `whittle-core`'s public surface
  (including the `refinement!` macro) and gates each integration
  module behind a Cargo feature. Integration tests live in the root
  `tests/` directory and exercise the full feature matrix.

The default feature set enables `serde` and the `refinement!` macro.
`proptest`, `schemars`, and `sqlx` are opt-in features. A consumer that
wants only the `Rule`/`Refined` kernel pays nothing for the
integrations they do not enable.

## 8. Core Traits

### 8.1. Rule

```rust
pub trait Rule<T: 'static>: Sized + 'static {
    type Error;
    fn refine(raw: T) -> Result<T, Self::Error>;
    fn schema() -> Schema;
}
```

`T: 'static` is required because every `Schema` variant that carries
a type identity uses `TypeId::of::<T>()`, which is bounded
`T: 'static`. The kernel could expose a separate `Rule<T>` without
`T: 'static` and a `RuleSchema<T: 'static>: Rule<T>` super-trait, but
the simplification is not worth the API surface; refined types in
practice are owned types (`String`, `i64`, `Decimal`, `Url`, ...),
not borrows.

`refine` is the narrowing morphism: it consumes raw input, returns the
narrowed (and possibly canonicalised) value on success, or a typed
error on rejection. Rules whose narrowing is purely a predicate return
`Ok(raw)` unchanged.

`schema()` returns the reflectable schema described in Section 12.
Rules that cannot provide a schema MUST still implement `Rule<T>` but
MUST return a `Schema::Unconstrained` variant whose `reason` is
`UnconstrainedReason::OpaqueRule` so the integrations that depend on
schema
metadata can detect the absence and either skip or fall back gracefully.

### 8.2. PureFilter

```rust
pub trait PureFilter<T>: Rule<T> {}
```

A marker trait implemented by rules whose `refine` is the identity on
admissible inputs (no canonicalisation). The codec, JSON Schema, and
the optimiser of consumer libraries (such as axiom-rs) can exploit
this to prove the rule preserves bytes. The marker is otherwise
load-bearing only as a contract: implementers assert that
`refine(x) == Ok(x)` for every admissible `x`.

## 9. The Refined Carrier

### 9.1. Layout

```rust
#[repr(transparent)]
pub struct Refined<T, R> {
    inner: T,
    _rule: PhantomData<R>,
}
```

The struct definition does not bound `R: Rule<T>` because the carrier
itself never invokes `R::refine` after construction. Bounding on impl
blocks lets accessors and trait passes through without re-stating the
bound everywhere.

`#[repr(transparent)]` plus the zero-sized `PhantomData` guarantee
that `Refined<T, R>` has the same layout as `T`. Niche optimisations
available on `T` are preserved by `Refined<T, R>`.

### 9.2. Construction

```rust
impl<T, R: Rule<T>> Refined<T, R> {
    pub fn try_new(raw: T) -> Result<Self, R::Error> {
        R::refine(raw).map(|inner| Self { inner, _rule: PhantomData })
    }
}
```

`try_new` is the sole public construction path. The inner field is
crate-private; no public accessor returns a mutable reference to the
inner value. No `unsafe` construction shortcut is provided, and the
library MUST NOT introduce one without a corresponding amendment to
[IDEA.md](IDEA.md) §5.2.

### 9.3. Accessors

```rust
impl<T, R> Refined<T, R> {
    pub fn as_inner(&self) -> &T { &self.inner }
    pub fn into_inner(self) -> T { self.inner }
}
```

`into_inner` returns an owned `T` and is the proof-erasing morphism
required by [IDEA.md](IDEA.md) §5.2: returning the inner value is
sound because the only public path back to `Refined<T, R>` is
`try_new`, which re-runs the rule. There is no `AsMut<T>`; mutation
of the inner value MUST go through `into_inner` followed by `try_new`
on the modified value.

Pass-through derives (`Debug`, `Clone`, `Hash`, `PartialEq`, `Eq`,
`PartialOrd`, `Ord`) are implemented manually with the appropriate
`where T: ...` bounds rather than via `#[derive]`, because Rust's
derive macros do not introduce per-trait `where`-clauses (they bound
on the *type parameters* of the struct, which would force `R: Hash`
etc.). The manual impls delegate to the inner value; the rule
identity is part of the type, not part of equality or ordering.

### 9.4. Serde

When the `serde` feature is enabled:

- `Serialize` delegates to the inner value's `Serialize`. No
  rule-specific transformation is applied during serialisation; the
  refined value's inner bytes are what get written.
- `Deserialize` deserialises the inner value, then routes through
  `try_new`, returning the rule's typed error wrapped in
  `serde::de::Error::custom`.

The round-trip law is `Deserialize(Serialize(x)) == Ok(x)` for every
refined `x`, which holds because the rule's canonical form is stable
under re-narrowing.

## 10. Library-Supplied Primitive Rules

The library provides the following primitive rule markers in
`whittle-core::primitive`:

### 10.1. Numeric

```rust
pub struct Within<const MIN: i128, const MAX: i128>;
pub struct AtLeast<const MIN: i128>;
pub struct AtMost<const MAX: i128>;
pub struct NonZero;
pub struct Positive;
pub struct Negative;
```

`Within<MIN, MAX>` MUST reject `MIN > MAX` at the boundary that has
the most static information available. The library SHOULD enforce
this through a `const_assert` (compile-time check) when stable Rust
admits it, and MUST verify it through a `Rule` impl whose `refine`
returns `Err(NumericError::EmptyRange)` on the first call when the
const-generic check could not be performed at compile time.

Numeric primitives are generic over the underlying type via a blanket
`impl<T: TryInto<i128> + TryFrom<i128>, ...> Rule<T> for Within<…>`.
Implementations exist for `i8`, `i16`, `i32`, `i64`, `i128`, `u8`,
`u16`, `u32`, `u64`, `usize`. Unsigned bounds delegate to the
signed-`i128` implementation and reject out-of-range conversions with
`NumericError::OutOfRange`.

`u128` is not covered by the `i128`-parameterised primitives because
`i128` cannot represent values above `i128::MAX`. Consumers needing
ranges in the `i128::MAX..=u128::MAX` band MUST write a custom rule
or use the `WithinUnsigned<MIN, MAX>` companion (added when a real
use-case appears).

### 10.2. Floating-Point

```rust
pub struct Finite;        // rejects ±inf and NaN
pub struct NotNan;        // rejects NaN, accepts ±inf
pub struct InClosedRange<const MIN_BITS: u64, const MAX_BITS: u64>;
```

Float-range constants use bit-pattern encoding because const-generic
`f64` is not stable. Helper macros (`closed_range!(0.0, 1.0)`) emit
the bit patterns at compile time. The check is performed on `f64`
values reconstructed via `f64::from_bits`, NOT on the raw bit
patterns (whose `u64` ordering does not match float ordering — `-1.0`
has a larger `u64` representation than `+1.0`). Callers MUST ensure
`f64::from_bits(MIN_BITS) <= f64::from_bits(MAX_BITS)`; the
`closed_range!` macro emits a `const_assert` to that effect.

For `f32` ranges, the companion type `InClosedRangeF32` (a
`u32`-parameterised analogue) exists so the const-generic parameter
is correctly sized.

### 10.3. Decimal

```rust
pub struct DecimalPrecision<const P: u8>;       // total significant digits
pub struct DecimalScale<const S: u8>;           // digits after the point
pub struct DecimalPositive;
pub struct DecimalInRange<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8>;
```

Behind the `decimal` Cargo feature, which pulls in `rust_decimal`.
Range constants encode the range's representative as a fixed-point
integer with explicit scale. The same dodge `InClosedRange` uses for
`f64` — Rust 2024 does not yet allow `Decimal` const generics.

### 10.4. String Grammar

```rust
pub struct LenChars<const MIN: usize, const MAX: usize>;
pub struct LenBytes<const MIN: usize, const MAX: usize>;
pub struct NonEmpty;
pub struct EachChar<P: CharPredicate>(PhantomData<P>);
pub struct AsciiOnly;
pub struct IsTrimmed;     // PureFilter: rejects leading/trailing whitespace
pub struct Trim;          // canonicalising: normalises by trimming
pub struct LowerCase;     // canonicalising: normalises to lowercase
pub struct IsLowerCase;   // PureFilter: rejects non-lowercase input
pub struct UpperCase;     // canonicalising: normalises to uppercase
pub struct IsUpperCase;   // PureFilter: rejects non-uppercase input
pub struct NfcNormalised; // canonicalising: Unicode NFC normalisation
pub struct IsNfcNormalised; // PureFilter: rejects non-NFC input
```

`CharPredicate` is a sub-trait with library-supplied implementations
(`NonControl`, `AsciiAlphanumeric`, `Identifier`, `Digit`, ...). Users
may add their own.

### 10.5. Collection

```rust
use core::cmp::Ordering;
use core::marker::PhantomData;

pub struct LenItems<const MIN: usize, const MAX: usize>;
pub struct NonEmptyCollection;
pub struct AllItems<R>(PhantomData<R>);          // every item satisfies R
pub struct UniqueByKey<T, F: KeyOf<T>>(PhantomData<(T, F)>);
pub struct Sorted<T, K: KeyOf<T> = IdentityKey<T>>(PhantomData<(T, K)>);
pub struct SortedBy<T, C: Cmp<T>>(PhantomData<(T, C)>);

pub trait KeyOf<T> {
    type Key: Ord + Eq;
    fn key_of(t: &T) -> Self::Key;
}
pub trait Cmp<T> { fn compare(a: &T, b: &T) -> Ordering; }

/// Identity key: a type that uses `T` itself as the ordering key.
/// Requires `T: Ord + Clone`.
pub struct IdentityKey<T>(PhantomData<T>);
impl<T: Ord + Clone + 'static> KeyOf<T> for IdentityKey<T> {
    type Key = T;
    fn key_of(t: &T) -> T { t.clone() }
}
```

`AllItems<R>::Error` carries the failing index so callers can locate
the offending element:

```rust
pub struct AllItemsError<E> {
    pub index: usize,
    pub source: E,
}
```

`AllItems<R>` is the per-element refinement primitive. The standard
pattern for a bounded list of refined items is

```rust
Refined<Vec<T>, And<LenItems<1, 100>, AllItems<MyItemRule>>>
```

### 10.6. Enum Subset

```rust
// Per-subset marker type, generated by the `subset_of!` macro:
// subset_of!(NonCleanFileState, FileState, [Dirty, Fixing, Testing,
//                                            FileGreen, Failed]);
// generates `pub struct NonCleanFileState;` plus the `Rule` impl.
```

Accepts only the listed variants of an enum. Used for "this
`FileState` must not be `Clean`" cases. A per-subset marker type is
generated by macro rather than encoding admitted variants in a
const-generic `&'static [&'static str]` parameter; the latter
requires the unstable `adt_const_params` feature and admits
misspelled variant names at the const-generic boundary, which the
type system cannot catch.

The `Enum` trait used by the generated implementations is
derive-emitted by `#[derive(EnumRule)]`.

### 10.7. Composition

```rust
pub struct And<A, B>(PhantomData<(A, B)>);   // both rules must accept
pub struct Or<A, B>(PhantomData<(A, B)>);    // either rule may accept
```

`And` short-circuits on first failure; `Or` short-circuits on first
success. Both compose with any other rule. There is no `AndN` n-ary
alias; the `refinement!` macro nests binary `And<A, And<B, ...>>` to
build longer compositions, and the `<And<A, B> as Rule<T>>::schema`
impl flattens its right-spine into a single `Schema::And(vec![...])`
so the reflectable schema is flat regardless of nesting depth.

## 11. Contextual Rules

```rust
pub trait RuleWith<T: 'static, Env: 'static>: Sized + 'static {
    type Error;
    fn refine_with(env: &Env, raw: T) -> Result<T, Self::Error>;
    fn schema() -> Schema {
        Schema::ContextOpaque {
            ty:     TypeId::of::<T>(),
            env_ty: TypeId::of::<Env>(),
        }
    }
}

#[repr(transparent)]
pub struct RefinedWithRef<'a, T, Env, R: RuleWith<T, Env>> {
    inner: T,
    _env:  PhantomData<&'a Env>,
    _rule: PhantomData<R>,
}

impl<'a, T, Env, R: RuleWith<T, Env>> RefinedWithRef<'a, T, Env, R> {
    pub fn try_new(env: &'a Env, raw: T) -> Result<Self, R::Error> {
        R::refine_with(env, raw).map(|inner| Self {
            inner, _env: PhantomData, _rule: PhantomData,
        })
    }
    pub fn as_inner(&self) -> &T { &self.inner }
    pub fn into_inner(self) -> T { self.inner }
}

pub struct RefinedWithOwned<T, Env, R: RuleWith<T, Env>> {
    inner: T,
    env:   Env,                  // stored — proof identity depends on it
    _rule: PhantomData<R>,
}

impl<T, Env, R: RuleWith<T, Env>> RefinedWithOwned<T, Env, R> {
    pub fn try_new(env: Env, raw: T) -> Result<Self, R::Error> {
        R::refine_with(&env, raw).map(|inner| Self {
            inner, env, _rule: PhantomData,
        })
    }
    pub fn as_inner(&self) -> &T { &self.inner }
    pub fn as_env(&self)   -> &Env { &self.env }
    pub fn into_parts(self) -> (T, Env) { (self.inner, self.env) }
}
```

Adapted from the `witnessed` crate's `WitnessIn<T, Env>` /
`WitnessedInRef` / `WitnessedInOwned` pattern, with two structural
fixes:

1. `RefinedWithOwned` **stores** the environment alongside the inner
   value. The previous `PhantomData<EnvHandle>` carrier could not
   tell apart "constructed against this `Arc<X>`" from "constructed
   against that `Arc<X>`" — both produced the same phantom type, so
   the proof was purely nominal and a different instance of the same
   type defeated it. Storing the env trades the `#[repr(transparent)]`
   layout for owned carriers in exchange for proof identity tied to
   the actual constructed-against environment value.
2. Parameter order is `(T, Env)` on both carriers and the trait, so
   the position of the inner type is consistent across the surface.

The borrowed carrier ties the proof to a borrowed environment's
lifetime: `RefinedWithRef<'a, T, Env, R>` cannot outlive the borrow.
The owned carrier ties the proof to a specific environment value that
the carrier holds.

Documentation MUST warn callers that owned contextual refinement is a
snapshot at construction time: if the environment is internally
mutable and changes after construction, the proof against the
*original* construction-time state MAY no longer hold against the
*current* state. Consumers needing live-environment guarantees MUST
use the borrowed carrier (and a non-mutable environment).

`Deserialize` is NOT implemented on `RefinedWithRef` or
`RefinedWithOwned` — `serde` has no slot for the environment.
Contextual values MUST be constructed via `try_new(env, raw)` after a
separate deserialization of `raw` against the environment in scope.

## 12. Schema Reflection and Derived Integrations

### 12.1. The Schema enum

```rust
pub enum Schema {
    /// No structural constraints beyond the inner type's own invariants.
    /// `reason` discriminates *why* the rule is unconstrained so derived
    /// integrations can report it (OpaqueRule for an external rule with
    /// no schema, CustomRefine for a `custom_refine:` closure). Both
    /// reasons cause schema-driven strategy and JSON-schema generation
    /// to be skipped; the discriminator exists for diagnostics, not for
    /// behavioural branching.
    Unconstrained { ty: TypeId, reason: UnconstrainedReason },
    /// Contextual rule; schema for the value-side is not extractable.
    ContextOpaque { ty: TypeId, env_ty: TypeId },
    /// Numeric range over i128.
    NumericRange { ty: TypeId, min: Option<i128>, max: Option<i128> },
    /// Float finiteness / non-NaN / range.
    FloatRefined {
        ty: TypeId, finite: bool, not_nan: bool,
        min_bits: Option<u64>, max_bits: Option<u64>,
    },
    /// Decimal precision/scale.
    Decimal {
        precision: Option<u8>,
        scale:     Option<u8>,
        positive:  bool,
        range:     Option<DecimalRange>,
    },
    /// String with length and per-character rules.
    StringRule {
        len_bytes:  Option<(usize, usize)>,
        len_chars:  Option<(usize, usize)>,
        each_char:  Option<CharSchema>,
        normalised: Vec<StringNormaliser>,
    },
    /// Collection with length and per-element rule.
    Collection {
        len:       Option<(usize, usize)>,
        item:      Option<Box<Schema>>,
        unique_by: Option<&'static str>,
        sorted:    Option<SortSpec>,
    },
    /// Enum subset.
    EnumSubset { ty: TypeId, variants: Vec<&'static str> },
    /// Composition.
    And(Vec<Schema>),
    Or(Vec<Schema>),
}

pub enum UnconstrainedReason {
    /// Rule exists but provides no introspectable structure
    /// (e.g. an opaque external rule).
    OpaqueRule,
    /// Rule body is a `custom_refine:` closure with no schema metadata.
    CustomRefine,
}

pub struct SortSpec {
    pub direction: SortDirection,
    pub key: Option<&'static str>,  // None == identity (Ord on T)
}
pub enum SortDirection { Ascending, Descending }

pub struct DecimalRange {
    pub min_repr: i128,
    pub max_repr: i128,
    pub scale:    u8,    // shared scale for min_repr and max_repr
}
```

The `Schema::Unconstrained` variant collapses the earlier `Any` and
`Opaque` variants into one shape with a discriminator, so the
"unconstrained" fact lives in one place and derived integrations
branch on `reason` rather than on variant identity.

`Schema::ContextOpaque` carries both the value and environment type
ids so two distinct `RuleWith<T, Env>` rules — even with the same
`T` but different `Env` — produce structurally distinct schemas.

`DecimalRange` replaces the earlier loose pair of `min_repr` /
`max_repr` / `scale_anchor` fields with one struct whose three fields
have a single, documented meaning.

`SortSpec` carries the direction and the key extractor's identity so
schema equality reflects sort identity rather than collapsing to a
boolean.

The exact enum is revisable; the contract is that `Schema` is enough
information for the derived integrations of Sections 12.2–12.4 to
function.

### 12.2. Property Generators

Behind a `proptest` Cargo feature, `whittle-core::arbitrary`
introduces a sub-trait of `Rule<T>` that promises a schema-driven
strategy, and implements `Arbitrary` only for rules that provide
it. The `Arbitrary` impl, the `StrategyFromSchema` trait, and the
primitive impls all live in `whittle-core` so Rust's orphan rule is
satisfied (the trait is local — even though `Arbitrary` is foreign,
`Refined<T, R>` is local to `whittle-core`):

```rust
pub trait StrategyFromSchema<T: 'static>: Rule<T> {
    fn strategy_from_schema() -> BoxedStrategy<T>;
}

impl<T, R> proptest::arbitrary::Arbitrary for Refined<T, R>
where
    T: 'static,
    R: StrategyFromSchema<T>,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;
    fn arbitrary_with(_: ()) -> Self::Strategy {
        R::strategy_from_schema()
            .prop_map(|raw| Refined::<T, R>::try_new(raw)
                // SAFETY by schema: the derived strategy generates
                // only admissible values; refuted via property test
                // in `whittle-core::arbitrary` tests if a primitive misbehaves.
                .expect("schema-derived strategy produced inadmissible value"))
            .boxed()
    }
}
```

Every library-supplied primitive whose `schema()` is structurally
derivable implements `StrategyFromSchema`; these impls live in
`whittle-core` alongside the primitives themselves. The
implementation walks the rule's `Schema`:

- a `NumericRange` node with `min` and `max` produces a `min..=max`
  strategy;
- a `StringRule` node with declared character predicates and length
  bounds produces a constrained-character `proptest::collection::vec`
  projected to `String`;
- an `And` node composes the constrained strategies of its branches;
- an `Or` node samples a branch uniformly (see §17 for the weights
  open issue).

The strategy MUST NOT use rejection sampling ("generate any value,
filter through refine") as its default path.

For rules whose schema is `Unconstrained` with reason `OpaqueRule` or
`CustomRefine`, or for contextual rules (`Schema::ContextOpaque`),
`StrategyFromSchema` is NOT implemented — `Refined<T, OpaqueRule>`
correspondingly does NOT implement `Arbitrary`. Users MAY supply a
hand-written strategy by implementing `StrategyFromSchema` for their
rule, or they MAY skip schema-driven arbitrary generation entirely
for that type. The library MUST NOT silently fall back to rejection
sampling for opaque rules.

### 12.3. JSON Schema (schemars feature)

Each schema node maps onto the corresponding JSON Schema construct:
`NumericRange` → `{type: integer, minimum, maximum}`; `StringRule` →
`{type: string, minLength, maxLength, pattern}`; etc. The schema's
unique identity (TypeId) provides the `$id`.

### 12.4. Pretty Display

`Schema` implements a `fmt_human(&self)` method that returns a short
human-readable description ("integer between 0 and 100", "non-empty
string of printable Unicode") used by the default `Display` impl on a
rule's typed error.

## 13. Implication and Subtyping

```rust
/// `Self: Implies<W>` means `Self` is strictly stronger than `W`.
/// Implementers MUST NOT write `R: Implies<R>` (reflexive edges are
/// forbidden by convention).
pub trait Implies<Weaker> {}
```

`Self` is the stronger rule. When `S: Implies<W>` holds, the library
provides an explicit upcast method on `Refined<T, S>`:

```rust
impl<T: 'static, S: Rule<T>> Refined<T, S> {
    /// Upcast to a refined value carrying the weaker rule. No
    /// narrowing morphism runs; the inner value is moved.
    pub fn weaken<W>(self) -> Refined<T, W>
    where
        W: Rule<T>,
        S: Implies<W>,
    {
        Refined { inner: self.inner, _rule: PhantomData }
    }
}
```

`weaken` was chosen over a `From<Refined<T, S>>` blanket impl
because that blanket overlaps with the reflexive
`impl<X> From<X> for X` from `core::convert` whenever `S = W`. Rust's
coherence checker reasons over all possible parameter instantiations
and rejects the overlap, regardless of any convention against
reflexive `Implies` edges. When a future Rust release admits
negative bounds, the library MAY add a `From` impl gated on a
`NotEqual<S, W>` marker; until then, `weaken` is the explicit upcast
path and is what consumers MUST call.

The implication contract from [IDEA.md](IDEA.md) §5.7 is on the
implementer: `adm(S) ⊆ adm(W)`; when `W` canonicalises, every value
in `S::refine`'s range MUST already be in `W::refine`'s range; the
weaker rule has no observable behaviour that depends on re-running
its narrowing morphism on the upcast value. The conversion consumes
the stronger value and moves the inner field; no `Clone` bound on
`T` is introduced.

The library supplies implication edges for common numeric narrowings
(`Within<A, B>: Implies<Within<C, D>>` when `C <= A && B <= D`) via
macro expansion. Edges for arbitrary rules are user-written.

Transitivity: the library does NOT automatically derive transitive
implication edges. If `A: Implies<B>` and `B: Implies<C>` both hold,
`A: Implies<C>` MUST be declared explicitly (the library MAY supply
the impl via macro for documented numeric chains).

## 14. The Refinement Macro

### 14.1. Declarative form

```rust
refinement! {
    /// A 1..=100 character string of non-control Unicode.
    pub struct BoundedPrintable(String) {
        normalize: trim,
        normalize: nfc,
        min_chars: 1,
        max_chars: 100,
        each_char: non_control,
    }
}
```

Expansion (sketch), preserving all five declared steps in order:

```rust
pub struct BoundedPrintable(
    Refined<String, And<Trim,
                       And<NfcNormalised,
                           And<LenChars<1, 100>,
                               EachChar<NonControl>>>>>,
);

#[derive(Debug, thiserror::Error)]
pub enum BoundedPrintableError {
    // Normalisation steps (Trim, NfcNormalised) in the default
    // vocabulary are infallible and do not produce variants.
    #[error("length not in 1..=100 characters")]
    LenChars(<LenChars<1, 100> as Rule<String>>::Error),
    #[error("contains a control character")]
    EachChar(<EachChar<NonControl> as Rule<String>>::Error),
}

impl BoundedPrintable {
    pub fn try_new(raw: String) -> Result<Self, BoundedPrintableError> {
        Refined::try_new(raw).map(Self).map_err(Into::into)
    }
    pub fn as_str(&self) -> &str { self.0.as_inner().as_str() }
    // Plus generated Display, AsRef<str>, Debug, Deserialize impls
    // and a generated `From<<RuleComposition as Rule<String>>::Error>
    // for BoundedPrintableError` impl that maps the composed rule's
    // error variants into BoundedPrintableError's named variants.
}
```

The macro emits `And<A, And<B, And<C, D>>>` nested compositions in
declaration order, NOT an n-ary `AndN` alias. The macro additionally
emits a documentation comment listing the canonical step sequence so
readers can audit the pipeline from the generated code.

Normalisation steps in the default vocabulary are documented as
infallible; the generated error enum carries only variants for the
validation steps that can fail. A `custom_refine:` step contributes
its own error variant (see Section 14.3).

### 14.2. Step vocabulary

Named pipeline steps the macro recognises by default:

- **Normalisation** (canonicalising): `trim`, `to_lowercase`,
  `to_uppercase`, `nfc`, `nfd`, `strip_trailing_slash`,
  `collapse_internal_whitespace`.
- **Validation** (predicate, no canonicalisation):
  `min_chars: N`, `max_chars: N`, `min_bytes: N`, `max_bytes: N`,
  `min_len: N`, `max_len: N`, `min: N`, `max: N`, `non_zero`,
  `finite`, `not_nan`, `each_char: <predicate-name>`,
  `each_item: <rule-name>`, `unique_by: <key-expr>`, `sorted`.

The step vocabulary is extensible: users can register custom named
steps via `whittle_register_step!(step_name, fn(T) -> Result<T, _>)`.

### 14.3. Escape hatch

For cases the structured vocabulary cannot express:

```rust
refinement! {
    pub struct OddIso8601(String) {
        custom_refine: |s: String| -> Result<String, MyErr> {
            // parse, validate, return canonical or fail
        },
    }
}
```

`custom_refine:` produces no structural schema metadata; the
resulting rule's `schema()` returns
`Schema::Unconstrained { reason: UnconstrainedReason::CustomRefine, ty }`.
Schema-driven integrations skip automatic strategy and JSON-schema
generation for it; `Arbitrary` is not auto-derived. The macro emits a
documentation note when `custom_refine` is used so reviewers can find
it.

### 14.4. Derive macro

```rust
#[derive(Refined)]
#[refined(rule = And<LenChars<1, 100>, EachChar<NonControl>>)]
pub struct BoundedPrintable(String);
```

Equivalent to the declarative form for the case where the rule is
already a named library or user composition. The derive macro is
preferred when the rule type is the abstraction the user wants to
expose; the declarative macro is preferred when the pipeline is the
abstraction the user wants to expose.

## 15. Testing Architecture

### 15.1. Core tests

`whittle-core` MUST contain:

- unit tests for every library-supplied primitive's accept/reject
  behaviour;
- property tests proving each primitive's narrowing morphism is
  idempotent on admissible inputs;
- property tests proving canonicalising rules are deterministic;
- property tests proving implication edges preserve admissibility;
- compile-time tests (via `compile_fail` doctests) proving rule
  type-mismatch is rejected at compile time.

### 15.2. Macro tests

The `refinement!` declarative macro in `whittle-core::macros` is
covered by:

- doctests on the macro itself showing each supported invocation shape;
- `compile_fail` doctests for malformed invocations (missing
  separators, contradictory steps, unknown named steps).

### 15.3. Arbitrary derivation tests

`whittle-core::arbitrary` (under the `proptest` feature) MUST
contain property tests proving that every
value the derived strategy generates passes the rule's `refine`. This
encodes the blanket `Refined<T, R>: Arbitrary` impl's no-rejection
guarantee in code; primitive and composition strategies may apply
bounded filtering on dense or composed regions.

### 15.4. Integration tests

The root `tests/` directory MUST exercise:

- the `serde` feature against deserialization fixtures that include
  both admissible and invariant-violating payloads;
- the `schemars` feature against generated JSON Schema fragments
  compared with a committed expected schema (cassette-style);
- the `sqlx` feature against a fixture database. The default
  `just ci` gate runs `sqlx` type-derivation tests (encode/decode
  round-trips against in-memory `sqlx::Decode` mocks); a separate
  `just it` gate runs against a live fixture database. The mock-based
  tests are required; the live-database tests are opt-in and excluded
  from `just ci`;
- the `proptest` feature against random schemas.

## 16. Build Sequence

Phase A — kernel:

1. `whittle-core::rule`: `Rule<T>` trait, `Refined<T, R>` carrier,
   `try_new`, accessors, plus the manual pass-through impls
   (`Debug`, `Clone`, `Hash`, `PartialEq`, `Eq`, `PartialOrd`,
   `Ord`) with appropriate `where T: ...` bounds per §9.3.
2. `whittle-core::schema`: `Schema` enum, `fmt_human`, equality.
3. `whittle-core::primitive::numeric`: `Within`, `AtLeast`, `AtMost`,
   `NonZero`, `Positive`, `Negative` with `Rule` impls for all
   integer types.
4. `whittle-core::primitive::float`: `Finite`, `NotNan`,
   `InClosedRange`.
5. `whittle-core::primitive::string`: `LenChars`, `LenBytes`,
   `NonEmpty`, `EachChar`, `Trim`, `IsTrimmed`, `LowerCase`,
   `IsLowerCase`, `UpperCase`, `IsUpperCase`, `NfcNormalised`,
   `IsNfcNormalised`, `AsciiOnly`.
6. `whittle-core::primitive::collection`: `LenItems`, `AllItems`,
   `UniqueByKey`, `Sorted`, `SortedBy`, plus the `KeyOf` /
   `Cmp` / `IdentityKey` companion traits.
7. `whittle-core::composition`: `And`, `Or`, plus the schema-
   flattening rule so a nested `And<A, And<B, ...>>` reflects as a
   flat `Schema::And(vec![...])`.
8. `whittle-core::implies`: `Implies` trait, the
   `Refined::weaken` upcast method, library-supplied numeric
   implication edges.

Phase B — contextual and integrations:

1. `whittle-core::contextual`: `RuleWith`, `RefinedWithRef`,
   `RefinedWithOwned`.
2. `serde` integration on `Refined`.
3. `whittle-core::macros::refinement`: declarative macro with the step
   vocabulary in Section 14.2.
4. `whittle-core::arbitrary` (under `proptest` feature):
   schema-driven `StrategyFromSchema` trait, primitive impls, and
   the `Arbitrary` impl for `Refined<T, R>`.
5. Root `tests/`: full integration coverage.
6. `schemars` integration.
7. `sqlx` integration.

Each step is its own commit. Each commit passes the full gate.

This sequence is non-normative guidance. Implementations MAY reorder
if the dependency graph permits and the invariants in
[IDEA.md](IDEA.md) are preserved.

## 17. Open Issues

The items in this section are unresolved questions that affect the
architecture but not yet the implementation.

- **Step vocabulary registration.** Section 14.2's
  `whittle_register_step!` macro is sketched but the exact mechanism
  for cross-crate step registration is not designed. Defer until the
  first cross-crate user appears.
- **Contextual rule schema.** Contextual rules currently emit
  `Schema::ContextOpaque`, which forfeits schema-driven property
  generation. Whether the schema can carry a description of "the
  environment-dependent portion" is open.
- **Implication for arbitrary const expressions.** Section 13's
  numeric implication edges are macro-expanded over a finite set. A
  generic implementation using `generic_const_exprs` would cover all
  cases but is unstable. Defer.
- **Bidirectional codec for canonicalising rules.** Section 9.4 says
  `Serialize` delegates to the inner value. For a canonicalising
  rule, the round-trip is `decode(encode(x)) == x` because the
  canonical form is stable; but `encode(decode(raw))` may differ from
  `raw` for non-canonical raws. The architecture does not currently
  surface this asymmetry to the user.
- **Float NaN and the `Eq` derive.** A `Refined<f64, NotNan>` does
  not contain NaN, so `Eq` is sound. Whether the library should emit
  `Eq` on such types automatically is open; the conservative default
  is "no, the user must opt in."
- **`no_std` support.** The kernel could be `no_std` with
  `alloc`; the integrations cannot. Whether to gate the kernel on a
  feature is open. Defer until a `no_std` consumer appears.
- **Property-test strategy for `Or`.** The default is "pick a branch
  uniformly," which biases toward strategies that admit larger value
  sets. Whether the schema should carry weights is open.
- **Macro hygiene with user-defined types.** A user rule that
  references a type from the user's own crate must be resolvable from
  the macro-emitted code. The current sketch assumes the user re-
  exports the type in scope; a more robust solution may require
  attribute-macro parsing of the full type path.

## 18. References

### 18.1. Normative References

[RFC2119] Bradner, S., "Key words for use in RFCs to Indicate
Requirement Levels", BCP 14, RFC 2119, March 1997,
<https://www.rfc-editor.org/rfc/rfc2119.html>.

[RFC8174] Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119
Key Words", BCP 14, RFC 8174, May 2017,
<https://www.rfc-editor.org/rfc/rfc8174.html>.

### 18.2. Informative References

[RFC2026] Bradner, S., "The Internet Standards Process -- Revision 3",
BCP 9, RFC 2026, October 1996,
<https://www.rfc-editor.org/rfc/rfc2026.html>.

[RFC7322] Flanagan, H. and S. Ginoza, "RFC Style Guide", RFC 7322,
September 2014, <https://www.rfc-editor.org/rfc/rfc7322.html>.

[KING2019] King, A., "Parse, don't validate", November 2019,
<https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/>.

[REFINED] "refined: Rust refinement types",
<https://crates.io/crates/refined>.

[WITNESSED] "witnessed: Type-level witness wrapper for carrying
validated invariants", <https://crates.io/crates/witnessed>.

[BRANDED] "branded: Branded types for Rust",
<https://crates.io/crates/branded>.

[EFFECT-SCHEMA] "Effect Schema: a TypeScript schema library",
<https://effect.website/docs/schema/introduction/>.

[NUTYPE] "nutype: A proc-macro for creating newtypes with sanitization
and validation",
<https://crates.io/crates/nutype>.
