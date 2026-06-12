# Whittle Architecture Specification

Status of This Memo

This document is an internal project specification written in an RFC-style
Markdown form. The document borrows structure and editorial discipline from
RFC 7322 and uses RFC 2026 as process vocabulary for maturity, review, and
applicability.

This document is the concrete architecture for Whittle and describes the
library as implemented. It is derived from [IDEA.md](IDEA.md), which is
authoritative for goals, scope, non-goals, and invariants. When this
document conflicts with [IDEA.md](IDEA.md), [IDEA.md](IDEA.md) takes
precedence. Designs that are required or admitted by [IDEA.md](IDEA.md)
but not yet built are collected in Section 15 with their evidence
triggers; every other section describes shipped code.

Abstract

This document specifies the architecture for the Whittle library: a Rust
parse-don't-validate engine that narrows raw input into refined values at
construction time and propagates the resulting proofs through ordinary
Rust types. The implementation is a Cargo workspace with a thin facade
package at the root and two member crates: `whittle-core`, a `no_std`
kernel holding the `Rule` trait, the `Refined` carrier, the composition
operators, the library-supplied primitive rules, the closed-set family,
the implication trait, and the declarative macros; and `whittle-macros`,
a proc-macro crate hosting the compile-time-validated `pattern!` macro.
The kernel is dependency-free by default; `serde`, `proptest`, `regex`,
`rust_decimal`, `chrono`, and Unicode-category support are opt-in Cargo
features. The `serde` and `proptest` impls for `Refined<T, R>` live
inside `whittle-core` (rather than separate integration crates) because
implementing a foreign trait for `Refined` from any other crate would
violate Rust's orphan rule.

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
  ([Numeric](#101-numeric), [Floating-Point](#102-floating-point),
  [Decimal](#103-decimal), [String Grammar](#104-string-grammar),
  [String Transformers](#105-string-transformers),
  [Collection](#106-collection), [Composition](#107-composition),
  [Date](#108-date), [DateTime](#109-datetime), [Path](#1010-path),
  [Pattern](#1011-pattern))
- [Section 11: Closed Sets](#11-closed-sets)
- [Section 12: Implication and Subtyping](#12-implication-and-subtyping)
- [Section 13: Macros](#13-macros)
- [Section 14: Testing Architecture](#14-testing-architecture)
- [Section 15: Planned Milestones](#15-planned-milestones)
- [Section 16: References](#16-references)

## 1. Introduction

Whittle is a Rust library that turns untrusted raw values into refined
values through a single user-defined rule per refinement. The refinement
runs at construction time; once a refined value exists, downstream code
trusts it without further checks.

The shipped surface comprises:

- the kernel: the `Rule<T>` trait and the `Refined<T, R>` carrier, with
  `try_new` as the sole public construction path;
- library-supplied primitive rules for numerics, floats, decimals,
  strings, collections, dates, datetimes, relative paths, and regex
  patterns;
- composition operators (`And`, `Or`, `Not`, `Xor`, `MapErr`, and the
  n-ary `All`/`Any`) that keep error types flat;
- string transformers (`Trim`, `AsciiLowercase`, `AsciiUppercase`) that
  canonicalise before delegating;
- the closed-set family (`ClosedSet`, `closed_set!`) for parsing wire
  strings into plain enums;
- the `Implies` trait and the `weaken` upcast;
- declarative macros (`refinement!`, `deserialize_rule!`, `closed_set!`)
  and the procedural `pattern!` macro;
- behind the `serde` feature, deserialization gated through the rule,
  with streaming bound enforcement for length-bounded collections;
- behind the `proptest` feature, per-rule admissible-by-construction
  strategies and the `prop_total` / `prop_image_refines` test harness.

This document specifies the concrete mechanisms that realize the
requirements in [IDEA.md](IDEA.md). The architecture is a Technical
Specification in the RFC 2026 sense: it describes concrete procedures,
conventions, and formats for this library. Requirements that are stated
in [IDEA.md](IDEA.md) are cited, not restated; the practitioner guide in
the repository root `SKILL.md` covers usage patterns and is likewise not
duplicated here.

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
- the `witnessed` crate, whose contextual-witness pattern informs the
  planned contextual-rule design (Section 15);
- the `branded` crate, whose nominal-newtype structure informs the
  delegating surface domain newtypes expose;
- the Effect.Schema library, whose schema-as-value design informs the
  planned schema reflection (Section 15);
- the [DESIGN.md](DESIGN.md) sketch that preceded this document, which
  this document supersedes for architectural commitments.

## 4. Foundations

The implementation uses these concrete foundations:

- Language: Rust, edition 2024; workspace `rust-version` 1.94.
- Toolchain: a pinned nightly via `rust-toolchain.toml` (with
  `rustfmt`, `clippy`, and `llvm-tools-preview` components). Nightly is
  required by the branch-coverage gate (Section 5) and, behind the
  `regex` feature, by the `adt_const_params` / `unsized_const_params`
  features that let a `&'static str` regex live in a const generic.
- Workspace shape: one Cargo workspace containing a thin facade package
  at the workspace root and two member crates under `crates/`.
- `whittle-core` is `#![no_std]` with `alloc`; it pulls in `std` only
  when the `regex` feature is enabled (the regex crate and its keyed
  compile cache require `std`).
- Async runtime: none. Whittle is synchronous; the constructor surface
  is `fn`, not `async fn`.
- Default feature set: **empty**. A default build of the kernel has no
  dependencies at all.

Optional dependencies, each behind the Cargo feature of the same name
unless noted:

| Feature    | Dependency                          | Notes               |
| ---------- | ----------------------------------- | ------------------- |
| `serde`    | `serde` (no default features)       | codec + gating      |
| `proptest` | `proptest` (`std` feature)          | strategies, harness |
| `decimal`  | `rust_decimal` (no default feats)   | decimal rules       |
| `chrono`   | `chrono` (no default features)      | date/datetime rules |
| `unicode`  | `unicode-general-category`          | `PrintableChar`     |
| `regex`    | `regex` + `whittle-macros`          | `Pattern`, needs std|
| `hex`      | (none)                              | hex string rules    |

Error types are hand-written enums implementing `core::error::Error`;
the library does not depend on `thiserror`, and bounded carriers are
expressed as rules over `String` / `Vec<T>` rather than via external
bounded-container crates. Dev-dependencies (test-only) are `proptest`,
`serde` with derive, `serde_json`, and `serde_test`; the facade
additionally uses `thiserror` in its integration-test corpus.

There is no central limits module. Every bound is a const-generic
parameter on the rule that owns it (`LenBytes<MIN, MAX>`,
`LenItems<MIN, MAX>`, ...), so the bound's single determinant is the
rule instantiation at the use site. The only library-chosen numeric
limits are the closed-set diagnostic caps (Section 11).

## 5. Toolchain and Gates

The local gate vocabulary is provided by `just` recipes backed by cargo
aliases in `.cargo/config.toml`:

- `just ci` runs the full local gate, in order: `fmt-check`, `lint`,
  `test`, `test-default-build`, `docs`.
- `just fmt-check` runs `cargo fmt --all --check`.
- `just lint` runs
  `cargo clippy --workspace --all-features --all-targets`.
- `just test` runs `cargo test --workspace --all-features` (unit, doc,
  and integration tests).
- `just test-default-build` runs `cargo test -p whittle-core --no-run`.
  Every other gate runs `--all-features`, so a test that uses a
  feature-gated item without a `cfg` gate breaks only here.
- `just docs` runs `mado check` over `git ls-files '*.md'` using
  `.mado.toml`. The tracked-file list is the single determinant of the
  linted set: gitignored scratch documents are excluded and newly
  tracked Markdown joins the gate automatically.
- `just doc-build` builds the rustdoc tree
  (`cargo doc --workspace --all-features --no-deps`).

The `cargo coverage` alias runs `cargo llvm-cov nextest` over the whole
workspace with `--all-features` and `--branch`, and fails on any
uncovered region, function, or line (thresholds 0). Together with the
branch flag this enforces 100% coverage on four axes: regions,
functions, lines, and branches. `whittle-macros` is excluded from the
file filter because proc-macro code executes inside the compiler, where
coverage instrumentation cannot observe it; its expansion output is
covered by `whittle-core` and facade tests instead.

Git hooks are tracked in `scripts/hooks/` and installed into
`.git/hooks` by `scripts/install-hooks.sh`:

- `pre-commit` runs the rustfmt check (fast; per commit).
- `pre-push` runs the default-features test compile and the
  `cargo coverage` 100% gate (slow; once per push), pinning the
  toolchain from `rust-toolchain.toml` for every nested cargo
  invocation.

Lint posture: workspace lints are declared in the root `Cargo.toml`.
Every Clippy group — `all`, `pedantic`, `nursery`, `cargo`, and
`restriction` — is denied, with a documented allow-back list (each
entry carries its reason as a comment). Every Rustdoc lint is denied.
Suppressions MUST use `#[expect(LINT, reason = "...")]`; an `expect`
whose lint does not fire is itself a build failure.
`.cargo/clippy.toml` carries the disallowed-methods configuration.

Dependency-license posture: `.cargo/deny.toml` allows the standard
permissive set (0BSD, Apache-2.0, Apache-2.0 WITH LLVM-exception,
BSD-3-Clause, BSL-1.0, CC0-1.0, CDLA-Permissive-2.0, ISC, MIT,
Unicode-3.0, Unlicense, Zlib); unknown registries and unknown git
sources are denied. `cargo deny check` is run ad hoc (it is wired via
the `[deny]` table in `.cargo/config.toml`) and is not part of
`just ci`.

## 6. Crate and Module Shape

The repository layout is:

```text
whittle/
├── Cargo.toml                  workspace + thin facade package
├── justfile                    gate recipes
├── rust-toolchain.toml         pinned nightly + components
├── .mado.toml                  Markdown lint configuration
├── .cargo/
│   ├── clippy.toml             disallowed-methods configuration
│   ├── config.toml             cargo aliases (coverage, test-all, ...)
│   └── deny.toml               license allowlist, registry rules
├── src/lib.rs                  facade: re-exports whittle-core (+ pattern!)
├── tests/                      integration corpus (feature-gated targets)
├── scripts/
│   ├── install-hooks.sh        copies tracked hooks into .git/hooks
│   └── hooks/                  pre-commit, pre-push
├── crates/
│   ├── whittle-core/           the no_std kernel
│   │   └── src/
│   │       ├── rule.rs         Rule, Refined, DeserializeRule, ArbitraryRule
│   │       ├── composition.rs  And, Or, Not, Xor, MapErr, All, Any
│   │       ├── transform.rs    Trim, AsciiLowercase/Uppercase, StableUnder*
│   │       ├── implies.rs      Implies, weaken, library edges
│   │       ├── closed_set.rs   ClosedSet, parse/as_str, codec, strategies
│   │       ├── macros.rs       refinement!, deserialize_rule!, closed_set!
│   │       ├── testing.rs      prop_total, prop_image_refines (proptest)
│   │       └── primitive/      numeric, float, decimal, string,
│   │                           collection, path, pattern, date, datetime
│   └── whittle-macros/         proc-macro crate: pattern!
└── docs/
    ├── README.md
    ├── IDEA.md
    ├── ARCHITECTURE.md
    └── DESIGN.md
```

Unit and property tests live inline next to the rule they cover
(`mod tests` in each module file). Integration tests live in the root
`tests/` directory; targets that need optional features declare them
via `required-features` in the facade's `Cargo.toml`, so every feature
combination builds.

## 7. Dependency Direction

The dependency graph is one-way through the facade boundary:

- `whittle-core` depends only on the optional ecosystem crates listed
  in Section 4, plus (under the `regex` feature) `whittle-macros` for
  the `pattern!` re-export. It MUST NOT depend on the facade.
- `whittle-macros` is a leaf proc-macro crate (`proc-macro2`,
  `proc-macro-crate`, `quote`, `regex`, `syn`). It resolves the path
  its expansions emit through `proc-macro-crate`, preferring the
  `whittle` facade and falling back to `whittle-core`, so consumers of
  either crate can invoke `pattern!`.
- The root facade `whittle` re-exports `whittle-core`'s entire public
  surface (`pub use whittle_core::*`) and forwards each Cargo feature
  to the corresponding `whittle-core` feature. A consumer that wants
  only the `Rule`/`Refined` kernel pays nothing for the integrations
  they do not enable.

The `serde` and `proptest` impls for `Refined<T, R>` live inside
`whittle-core` behind their features. They cannot live in separate
crates without an orphan-rule violation: both the trait
(`serde::Deserialize`, `proptest::arbitrary::Arbitrary`) and the type
`Refined<T, R>` would be foreign to such a crate.

## 8. Core Traits

### 8.1. Rule

```rust
pub trait Rule<T>: Sized + 'static
where
    T: 'static,
{
    type Error;
    fn refine(raw: T) -> Result<T, Self::Error>;
}
```

`refine` is the narrowing morphism: it consumes raw input, returns the
narrowed (and possibly canonicalised) value on success, or a typed
error on rejection. Rules whose narrowing is purely a predicate return
`Ok(raw)` unchanged. The soundness obligation — `Ok(y)` implies `y` is
admissible — is [IDEA.md](IDEA.md) §5.1 and is discharged by property
test for every library-supplied rule (Section 14).

`T: 'static` is required so the planned schema reflection (Section 15)
can use `TypeId::of::<T>()`. Refined types in practice are owned types
(`String`, `i64`, `Decimal`, ...), not borrows, so the bound costs
nothing today.

There is no `schema()` method on `Rule`; schema reflection is unbuilt
(Section 15).

### 8.2. DeserializeRule (serde feature)

```rust
pub trait DeserializeRule<'de, T>: Rule<T>
where
    T: 'static,
{
    fn deserialize_refined<D>(deserializer: D) -> Result<Refined<T, Self>, D::Error>
    where
        D: serde::Deserializer<'de>;
}
```

`Refined<T, R>`'s `serde::Deserialize` impl delegates to this per-rule
hook, so each rule chooses *how* the wire value is consumed:

- Most rules use the default **parse-then-refine** path, exposed as the
  free function `parse_then_refine` and stamped as a one-line impl by
  the `deserialize_rule!` macro (Section 13.2): deserialize the raw
  `T`, then run `Refined::try_new`, surfacing rejections through
  `serde::de::Error::custom` (the rule error's `Display` output).
- Rules whose admissibility bounds the *size* of the input override the
  hook to enforce the bound **while** the wire value is decoded, so a
  hostile payload is rejected without materializing more than the rule
  admits. `LenItems<MIN, MAX>` over `Vec<T>` is the library's streaming
  override — the concrete mechanism behind [IDEA.md](IDEA.md) §5.13
  (bounded inputs) and the §7 requirement that the constructor surface
  be robust against resource-exhausting payloads.

Whatever the strategy, the accept/reject set and rejection diagnostics
MUST be identical to the parse-then-refine path; only the allocation
profile may differ. There remains no admissible code path that produces
a `Refined` without the rule's predicate holding ([IDEA.md](IDEA.md)
§5.3).

Unknown-field policy belongs to `T`, not to whittle: serde's data model
gives outer adapters no field-level callbacks, so consumers who want
`deny_unknown_fields` put the attribute on the inner type.

### 8.3. ArbitraryRule (proptest feature)

```rust
pub trait ArbitraryRule<T>: Rule<T>
where
    T: 'static,
{
    type Strategy: proptest::strategy::Strategy<Value = T>;
    fn arbitrary_strategy() -> Self::Strategy;
}
```

Each rule supplies a strategy that emits admissible-by-construction
values. The blanket `Arbitrary` impl for `Refined<T, R>` maps the
rule's strategy through `try_new` and, on contract violation, panics
with the violating rule's `type_name` — a strategy bug surfaces as a
localized test-time panic, never as silently dropped samples. The
blanket impl performs no rejection sampling; composition rules MAY
filter their operands' strategies, primitive rules MUST be
constructive. This is the per-rule mechanism sanctioned by
[IDEA.md](IDEA.md) §5.11 as amended; derivation from a reflected schema
is the destination (Section 15).

Bounded numeric strategies are edge-biased: samples concentrate on the
admissible region's boundaries, where off-by-one defects live.
Carrier-family helper traits (`ArbitraryNumeric`, `ArbitraryFloat`,
`ArbitraryDecimal`, `ArbitraryDate`, `ArbitraryDateTime`,
`ArbitraryChar`, `ArbitraryPredicate`) supply the per-carrier strategy
plumbing; see the rustdoc for each.

## 9. The Refined Carrier

### 9.1. Layout

```rust
#[repr(transparent)]
pub struct Refined<T, R> {
    inner: T,
    rule: PhantomData<fn() -> R>,
}
```

`#[repr(transparent)]` plus the zero-sized phantom guarantee that
`Refined<T, R>` has the same layout as `T`; niche optimisations on `T`
are preserved ([IDEA.md](IDEA.md) §5.4). The phantom is
`PhantomData<fn() -> R>` so the rule marker contributes neither
auto-trait obligations nor drop-check constraints. The struct does not
bound `R: Rule<T>`; the bound is applied on the impl blocks that need
it, so accessors and trait passes compile without restating it.

### 9.2. Construction

`Refined::<T, R>::try_new(raw: T) -> Result<Self, R::Error>` is the
sole public construction path ([IDEA.md](IDEA.md) §5.2). The inner
field is private; no `unsafe` construction shortcut exists, and the
library MUST NOT introduce one without a corresponding amendment to
[IDEA.md](IDEA.md) §5.2.

Two crate-private constructors exist for paths that have already
checked the invariant, each with a per-site soundness comment:
`from_inner` (used by the const constructors of Section 10.1 and the
streaming deserialization hooks of Section 8.2) and `as_inner_mut`
(used only by the checked mutation methods of Section 9.4, which verify
the rule's invariant before committing).

### 9.3. Accessors

- `as_inner(&self) -> &T` (a `const fn`) — proof-preserving borrow.
- `into_inner(self) -> T` — the proof-erasing morphism of
  [IDEA.md](IDEA.md) §5.2: the caller takes ownership but must re-run
  `try_new` to reconstruct a refined value.

There is no public mutable accessor; the general mutation path is
`into_inner` → mutate → `try_new`.

### 9.4. Proof-Preserving Operations

Beyond the accessors, the carrier offers operations whose outputs
remain proof-carrying:

- `try_map<U, S, F>(self, f: F) -> Result<Refined<U, S>, S::Error>` on
  any `Refined<T, R>`: transform the inner value and re-establish a
  (possibly different) rule by routing through `try_new`. No soundness
  debt — the target rule re-runs.
- On `Refined<Vec<T>, R>`:
  - `map_items` — an *infallible* element-wise map for rules marked
    `StableUnderElementMap` (length-only rules such as `LenItems`):
    the map preserves length, so the proof transfers without
    re-validation.
  - `try_push` / `try_extend` — checked mutation under
    `LenItems<MIN, MAX>` with the typed `CapacityFull` rejection that
    returns the rejected payload to the caller; `try_extend` is
    all-or-nothing.
  - `first` / `last` / `split_first` — **total** accessors available
    when the rule proves `MIN >= 1`: the non-empty proof makes the
    `Option` unnecessary.

### 9.5. Pass-Through Impls

`Debug`, `Display`, `Clone`, `Copy`, `Hash`, `PartialEq`, `Eq`,
`PartialOrd`, and `Ord` are implemented manually with `where T: ...`
bounds rather than via `#[derive]`, because derive macros bound on the
struct's type parameters (which would force `R: Hash` etc.). All
delegate to the inner value; the rule identity is part of the type, not
part of equality or ordering.

### 9.6. Serde

When the `serde` feature is enabled:

- `Serialize` delegates to the inner value's `Serialize`; refined
  values are indistinguishable from raw values on the wire.
- `Deserialize` routes through the rule's `DeserializeRule` hook
  (Section 8.2), so deserialization cannot bypass the narrowing
  morphism.

The round-trip law and its derivation from canonical-form stability are
[IDEA.md](IDEA.md) §5.12.

## 10. Library-Supplied Primitive Rules

The library provides the following primitive rule markers in
`whittle-core::primitive`. Every rule returns its module's flat error
enum (`NumericError`, `FloatError`, `DecimalError`, `StringError`,
`CollectionError`, `DateError`, `DateTimeError`, `PathError`,
`PatternError`); see the rustdoc for variant-level detail.

### 10.1. Numeric

Standalone primitives, parameterised over `i128` const generics:

```rust
pub struct Within<const MIN: i128, const MAX: i128>;
pub struct AtLeast<const MIN: i128>;        // closed lower bound
pub struct AtMost<const MAX: i128>;         // closed upper bound
pub struct GreaterThan<const MIN: i128>;    // open lower bound
pub struct LessThan<const MAX: i128>;       // open upper bound
pub struct EqualTo<const N: i128>;          // singleton
```

Type aliases that name the conventional spelling of common rules:

```rust
pub type NotEqualTo<const N: i128> = Not<EqualTo<N>>;
pub type NonZero   = NotEqualTo<0>;
pub type Positive  = GreaterThan<0>;
pub type Negative  = LessThan<0>;
```

`Within<MIN, MAX>` rejects `MIN > MAX` at compile time through its
`VALID` const (`const { assert!(MIN <= MAX) }`, forced from `refine`
and the proptest strategy), so the empty range is unrepresentable in
instantiated types. Internally `Within` is a nominal newtype over
`And<AtLeast<MIN>, AtMost<MAX>>`; the composition does not leak into
the public error surface.

`Within` additionally provides const-capable constructors —
`try_new_i8` through `try_new_i128`, `try_new_u8` through `try_new_u64`,
`try_new_usize`, `try_new_isize` — so known protocol constants can be
expressed as `const` refined values without a runtime `unwrap`.

Numeric primitives are generic over the carrier via a `Numeric` trait
(`into_i128` / `from_i128`), implemented for `i8`, `i16`, `i32`, `i64`,
`i128`, `u8`, `u16`, `u32`, `u64`, `usize`, and `isize`. `u128` is not
covered because `i128` cannot represent values above `i128::MAX`;
consumers needing that band write a custom rule.

### 10.2. Floating-Point

```rust
pub struct NotNan;          // rejects NaN
pub struct NotInfinite;     // rejects ±inf
pub struct Finite;          // rejects NaN and ±inf
pub struct InClosedRange<
    const MIN_NUMERATOR: i64,
    const MIN_DENOMINATOR: i64,
    const MAX_NUMERATOR: i64,
    const MAX_DENOMINATOR: i64,
>;
```

Float-range endpoints are encoded as `(numerator, denominator)` ratios
because const-generic `f64` is not stable in Rust 2024; the endpoints
are reconstructed at refine time. `InClosedRange<0, 1, 1, 1>` is
`0.0..=1.0`. The rule's `VALID` const asserts at compile time that both
denominators are positive and that the range is non-empty. A single
definition serves both `f32` and `f64` through the sealed `Float`
trait. `Finite` is a nominal domain newtype over
`And<NotNan, NotInfinite>`; both share `FloatError`, so the composition
is invisible to callers.

### 10.3. Decimal

```rust
pub struct DecimalPositive;
pub struct DecimalScale<const S: u8>;           // digits after the point
pub struct DecimalPrecision<const P: u8>;       // total significant digits
pub struct DecimalInRange<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8>;
```

Behind the `decimal` Cargo feature, which pulls in `rust_decimal`.
Range constants are scaled `i128` mantissas with an explicit shared
scale — the same dodge `InClosedRange` uses for `f64`, because Rust
2024 does not yet allow `Decimal` const generics.

### 10.4. String Grammar

Validation rules (each a `Rule<String>`; the carrier is owned because
of the kernel's `T: 'static` bound):

```rust
pub struct LenChars<const MIN: usize, const MAX: usize>;
pub struct LenBytes<const MIN: usize, const MAX: usize>;
pub struct NonEmpty;
pub struct EachChar<P>(PhantomData<P>);   // every char satisfies P
pub struct FirstChar<P>(PhantomData<P>);  // the first char satisfies P
```

`CharPredicate` is the per-character predicate trait. Library-supplied
implementations: `AsciiAlphabetic`, `AsciiAlphanumeric`, `AsciiDigit`,
`AsciiGraphic`, `AsciiLowercase`, `AsciiUppercase`, `IdentChar`,
`IdentStart`, `IdentDashChar`, `NonControl`, and the combinators
`CharLiteral<const CH: char>`, `CharEither<A, B>`, `CharExcept<A, B>`.
`HexChar` is behind the `hex` feature; `PrintableLine`,
`PrintableMultiline`, and `PrintableChar` are behind the `unicode`
feature — `PrintableChar` rejects the Unicode general categories
Cc/Cf/Cs/Co/Cn/Zl/Zp via `unicode-general-category`, while
`PrintableLine` / `PrintableMultiline` are dep-free hardcoded subsets.
Users may implement `CharPredicate` for their own predicates.

`RejectsTrimWhitespace` is a marker sub-trait for predicates that
reject every `char::is_whitespace()` character; it makes
`FirstChar<P>: StableUnderTrim` (Section 10.5) sound.

Fixed-length hex string rules (behind the `hex` feature):

```rust
pub struct HexFixedLower<const LEN: usize>;        // exactly LEN lowercase hex chars
pub struct HexFixedAny<const LEN: usize>;          // exactly LEN mixed-case hex chars
pub type HexFixedNormalized<const LEN: usize>
    = AsciiLowercase<HexFixedAny<LEN>>;            // admits any case, stores lowercase
```

### 10.5. String Transformers

In `whittle-core::transform` (not the primitive module): each
transformer normalises input *before* delegating to its inner rule, so
the stored carrier is the canonical form.

```rust
pub struct Trim<R>(PhantomData<fn() -> R>);           // str::trim, then delegate
pub struct AsciiLowercase<R>(PhantomData<fn() -> R>); // ASCII-lowercase, then delegate
pub struct AsciiUppercase<R>(PhantomData<fn() -> R>); // ASCII-uppercase, then delegate
```

Transformers are infallible themselves (`Error = R::Error`) and compose
with each other and with validation rules (`Trim<NonEmpty>` rejects
input that is empty *after* trimming). Use them only when canonical
form is part of the contract; for invariants where the input form must
be preserved verbatim, use validation-only rules.

Each transformer's proptest strategy applies the normalisation to an
inner-rule sample, which is sound only when the inner rule's admissible
region is invariant under the morphism. The `StableUnderTrim`,
`StableUnderAsciiLowercase`, and `StableUnderAsciiUppercase` marker
traits encode that proof obligation; the kernel provides the blanket
propagation impls for compositions, and the audit recipe for adding a
new `StableUnder*` marker is documented on `StableUnderTrim`'s rustdoc.

### 10.6. Collection

Rules over `Vec<T>` (other collection shapes land when a real consumer
needs them):

```rust
pub struct LenItems<const MIN: usize, const MAX: usize>;
pub struct AllItems<R>(PhantomData<R>);          // every item satisfies R
pub struct UniqueByKey<T, K>(PhantomData<(T, K)>);
pub type   Distinct<T> = UniqueByKey<T, IdentityKey<T>>;
pub struct Sorted<T, K>(PhantomData<(T, K)>);    // ascending by key
pub struct NoneOf<P>(PhantomData<P>);            // no item matches P
pub struct AnyOf<P>(PhantomData<P>);             // at least one item matches P
```

Companion traits: `KeyOf<T>` (key extraction; `IdentityKey<T>` is the
`T: Ord + Clone` identity) and `Predicate<T>` (pure item predicate).

`CollectionError<EI = Infallible>` is the shared flat error enum; the
`BadItem { index, source: EI }` variant carries the failing index and
the inner rule's error so callers can locate the offending element. The
standard pattern for a bounded list of refined items is:

```rust
Refined<Vec<T>, And<LenItems<1, 100>, AllItems<MyItemRule>>>
```

`LenItems` hand-writes its `DeserializeRule` hook to enforce the length
bound during decoding (Section 8.2). `StableUnderElementMap` marks
length-only rules for which `map_items` (Section 9.4) is sound.

### 10.7. Composition

Binary operators, generic over any carrier whose operands share the
same `Rule::Error` type:

```rust
pub struct And<A, B>(PhantomData<(A, B)>);   // both must accept; Error = E
pub struct Or<A, B>(PhantomData<(A, B)>);    // either may accept; Error = [E; 2]
```

`And` short-circuits on first failure and threads the previous
operand's (possibly canonicalised) output into the next. `Or`
short-circuits on first success and runs the right operand against a
clone of the original input; on full rejection both errors are
preserved positionally. Because the operands share an error type, no
positional `Left` / `Right` wrapper exists — domain newtypes
pattern-match the flat error enum directly.

Boolean inversion and exclusive-or, restricted to numeric carriers
(`T: Numeric + Copy`, operands sharing `Rule::Error = NumericError`)
because the rejection paths must fabricate an error variant:

```rust
pub struct Not<R>(PhantomData<fn() -> R>);   // admits exactly what R rejects
pub struct Xor<A, B>(PhantomData<(A, B)>);   // exactly one of A, B accepts
```

Both reuse `NumericError::OutOfRange { value }` for the fabricated
rejection. Other carrier families MAY add their own impls under the
same constraint. `NotEqualTo<N> = Not<EqualTo<N>>` is the canonical
use.

Error-codomain mapping:

```rust
pub trait ErrorMapper<E>: 'static {
    type Error;
    fn map_error(error: E) -> Self::Error;
}
pub struct MapErr<R, M>(PhantomData<(R, M)>);
```

`MapErr<R, M>` preserves `R`'s accepted values while mapping its
rejection error through `M`, so a domain type can expose only the
rejection cases reachable through its composition instead of the full
breadth of a shared primitive error enum.

N-ary tuple-based operators (same shared-error constraint):

```rust
pub struct All<TUPLE>(PhantomData<fn() -> TUPLE>); // every operand accepts; Error = E
pub struct Any<TUPLE>(PhantomData<fn() -> TUPLE>); // first acceptance wins; Error = [E; N]
```

Supported arities: 2..=8. `All<(A, B, C)>` runs three operands
sequentially (equivalent to `And<A, And<B, C>>` without the nesting);
`Any<(A, B, C)>` tries each in order against a clone and returns the
first acceptance or `[E; 3]` collecting every rejection. For arity 2
they reduce to the same shape as `And` / `Or`.

All composition operators forward the serde `DeserializeRule` hook
(`And` / `All` thread the streaming hooks of their first operand) and
provide `ArbitraryRule` impls; composition strategies MAY filter their
operands' strategies (Section 8.3).

### 10.8. Date

Behind the `chrono` Cargo feature. Carrier: `chrono::NaiveDate`.

```rust
pub struct DateAtLeast<const MIN_DAYS_FROM_CE: i32>;
pub struct DateAtMost<const MAX_DAYS_FROM_CE: i32>;
pub struct DateInRange<const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32>;
```

Bounds are encoded as `i32` days from CE (the value returned by
`NaiveDate::num_days_from_ce`) because Rust 2024 does not yet allow
`NaiveDate` const generics. Compile-time `const { ... }` blocks
validate that each bound is within `NaiveDate`'s representable range
and that the range is non-empty. Cross-field ordering (a `from <= to`
date-range struct) is multi-field and remains a downstream concern, not
a primitive.

### 10.9. DateTime

Behind the `chrono` Cargo feature. Carrier: `chrono::DateTime<Utc>`
only (`FixedOffset` / `Local` deliberately unsupported; convert to UTC
at the boundary).

```rust
pub struct DateTimeAtLeast<const MIN_SECS_SINCE_EPOCH: i64>;
pub struct DateTimeAtMost<const MAX_SECS_SINCE_EPOCH: i64>;
pub struct DateTimeInRange<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64>;
```

Bounds are encoded as `i64` seconds since the Unix epoch
(`DateTime::<Utc>::timestamp`), with the same compile-time validation
pattern as `DateInRange`.

### 10.10. Path

```rust
pub struct RelativePath;
```

A portable, forward-slash-segmented relative-path check for the "this
string is a sandbox-relative path" guarantee: rejects empty input,
absolute paths (Unix `/`-rooted, Windows drive letters, UNC prefixes),
`..` parent traversal, and empty segments, with the offending segment
index in `PathError`. Full cross-platform path handling is out of
scope.

### 10.11. Pattern

Behind the `regex` Cargo feature (which requires `std` and the nightly
const-generics features per Section 4):

```rust
pub struct Pattern<const RE: &'static str>;
```

The regular expression lives in the type. A candidate is admissible
only when the regex matches the **entire** input — the rule enforces
the full span itself, so unanchored and anchored patterns behave
identically. Compiled regexes are cached in a keyed `OnceLock`. A bare
`Pattern<RE>` with a malformed `RE` panics on first construction;
prefer the `pattern!` macro (Section 13.4), which turns the malformed
pattern into a compile error. `Pattern` is the escape hatch for
positional grammars the composable character-class rules cannot express
ergonomically.

## 11. Closed Sets

The closed-set family satisfies [IDEA.md](IDEA.md) §5.6's closed-set
bullet as amended: a wire string is admitted against a single declared
injective string ↔ variant table and parsed into a plain Rust enum.
The enum is already constructive — its representable states are exactly
the admissible states — so no `Refined` carrier is involved.

```rust
pub trait ClosedSet: Copy + PartialEq + Sized + 'static {
    const MEMBERS: &'static [(&'static str, Self)];
    const VALID: ();   // compile-time witness: non-empty, injective
}
```

`MEMBERS` is the single determinant; everything else is derived:

- `closed_set::parse(&str) -> Result<E, ClosedSetError<E>>` — the
  boundary morphism;
- `closed_set::as_str(E) -> &'static str` — the lossless inverse;
- `ClosedSetError<E>` — a typed rejection carrying the offending value
  (truncated to 64 characters so error payloads stay bounded) and a
  `'static` borrow of the expected table (its `Display` renders at most
  8 members, then `… (N total)`);
- behind `serde`, `closed_set::serialize` / `deserialize` — the
  plain-wire-string codec (`Serialize` = `as_str`, `Deserialize` =
  `parse`), emitted automatically for macro-generated enums and usable
  via `#[serde(with)]` for hand-written impls;
- behind `proptest`, `closed_set::admissible()` (select-from-`MEMBERS`,
  support exactly the closed set) and `closed_set::rejects()` (a
  derived reject-input generator: case flips, truncations, extensions,
  the empty string, filtered arbitrary strings), so boundary tests need
  no hand-maintained reject list.

`VALID` is forced at monomorphisation by `parse` and `as_str` (the same
house pattern as `Within`'s `MIN <= MAX` gate), so a table with a
duplicate wire string is a compile error at first use. The
`closed_set!` macro (Section 13.3) additionally forces it at
declaration time and makes variant/table mismatches unrepresentable; a
hand-written impl instead carries the documented variant-coverage
obligation described in the module rustdoc.

Enum-side *subset* markers over an existing enum are deferred per the
[IDEA.md](IDEA.md) §5.6 amendment (Section 15).

## 12. Implication and Subtyping

```rust
pub trait Implies<W>: Sized {
    const VALID: () = ();
}
```

`Self` is the stronger rule (`adm(Self) ⊆ adm(W)`). Declaring
`S: Implies<W>` unlocks the explicit upcast on the carrier:

```rust
impl<T: 'static, S: Rule<T>> Refined<T, S> {
    pub fn weaken<W>(self) -> Refined<T, W>
    where
        W: Rule<T>,
        S: Implies<W>;
}
```

`weaken` moves the inner value and re-runs neither rule's narrowing
morphism. It is a method rather than a blanket `From` impl because that
blanket would overlap with the reflexive `impl<X> From<X> for X` in
`core::convert` whenever `S = W`, which Rust's coherence checker
rejects regardless of any convention against reflexive edges.

`VALID` defaults to `()` for unconditional user-declared edges. The
const-generic family impls override it with an `assert!`-carrying body,
so a `weaken` call whose instantiation violates the side condition is a
compile error at monomorphisation rather than an unsound upcast.

Library-supplied edges (each side condition compile-checked through
`VALID`):

| Stronger         | Weaker           | Side condition     |
| ---------------- | ---------------- | ------------------ |
| `Within<A, B>`   | `Within<C, D>`   | `C <= A && B <= D` |
| `Within<A, B>`   | `AtLeast<C>`     | `C <= A`           |
| `Within<A, B>`   | `AtMost<D>`      | `B <= D`           |
| `AtLeast<A>`     | `AtLeast<C>`     | `C <= A`           |
| `AtMost<B>`      | `AtMost<D>`      | `B <= D`           |
| `GreaterThan<A>` | `GreaterThan<C>` | `C <= A`           |
| `LessThan<B>`    | `LessThan<D>`    | `B <= D`           |
| `LenChars<A, B>` | `LenChars<C, D>` | `C <= A && B <= D` |
| `LenBytes<A, B>` | `LenBytes<C, D>` | `C <= A && B <= D` |
| `LenItems<A, B>` | `LenItems<C, D>` | `C <= A && B <= D` |

The three-property implication contract, the irreflexivity requirement,
and its reading against the const-generic family impls (where the
degenerate same-parameters instantiation is a trivially valid
containment, not a declared self-edge) are [IDEA.md](IDEA.md) §5.7; the
contract discharge for all ten edges is documented in the `implies`
module rustdoc, along with the edges deliberately *not* supplied
(cross-shape strict/inclusive edges, `EqualTo` edges, transformer and
composition edges) and the reasoning for each deferral.

Transitive edges are not derived (OPTIONAL per [IDEA.md](IDEA.md)
§5.7): if `A: Implies<B>` and `B: Implies<C>` hold, `A: Implies<C>`
must be declared explicitly.

## 13. Macros

The declarative macros live in `whittle-core::macros` and are exported
with `#[macro_export]`; the only proc-macro is `pattern!` in
`whittle-macros`.

### 13.1. refinement!

The macro has two forms. The **simple form**:

```rust
refinement! {
    /// User-supplied display name. Always at least one char.
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub Identifier: String, NonEmpty;
}
```

expands to a tuple struct wrapping `Refined<Inner, Rule>` and three
inherent methods — `try_new`, `as_inner`, `into_inner` — with the
inner field private, so the named newtype inherits the
smart-constructor guarantee ([IDEA.md](IDEA.md) §5.2). Standard trait
impls are forwarded from `Refined` and selected by the user-supplied
`#[derive(...)]` passthrough; `Display`, `AsRef`, serde impls, and the
rest of the delegating surface stay hand-written, and `try_new`
returns the rule's error unchanged. `Inner` and `Rule` are separated
by a comma because Rust's macro follow-set rules forbid `ty` followed
by `in`.

The **error-block form** appends an `error` mapping block:

```rust
refinement! {
    /// IATA-ish flight code: 3..=8 ASCII alphanumeric chars.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub FlightCode: String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>;
    impl Display;

    /// Flat domain error for [`FlightCode`].
    error StringError => pub FlightCodeError {
        /// Length (in characters) outside `3..=8`.
        StringError::CharCountOutOfRange { actual } => Length {
            /// Observed character count.
            actual: usize,
        }: "flight code length {actual} not in 3..=8",
        /// Character at the offset is not ASCII alphanumeric.
        StringError::BadChar { offset } => BadChar {
            /// UTF-8 byte offset of the rejected character.
            offset: usize,
        }: "flight code character at byte offset {offset} is not ASCII alphanumeric",
        unreachable StringError::ByteLenOutOfRange { .. }
            | StringError::Empty
            | StringError::BadFirstChar
            | StringError::BadHexLength { .. },
    }
}
```

and additionally generates the flat domain error enum
(`#[derive(Debug, PartialEq, Eq)]` plus attribute passthrough),
hand-rolled `Display` / `core::error::Error` impls from the per-arm
literals (no thiserror in generated code; a thiserror derive through
the passthrough is a documented conflict), and an
`impl ErrorMapper<SourceErr> for DomainErr` with `type Error = Self` —
the enum is **its own mapper**. The newtype wraps
`Refined<Inner, MapErr<Rule, DomainErr>>`, so the generated `try_new`
is `Refined::try_new(raw).map(Self)` with no mapping match in it:
the `ErrorMapper` impl is the single determinant, and construction,
serde deserialisation, and `ArbitraryRule` all inherit it through
`MapErr` (Section 8.2's default path runs the mapping at deserialize
time, so ingress rejections carry the domain `Display` text). The
error-block form also emits `impl AsRef<Inner>`, an opt-in carrier
`Display` behind the `impl Display;` token, and — behind whittle's
`serde` feature, like `closed_set!`'s glue — transparent
`Serialize` / `Deserialize` impls forwarding to the inner `Refined`.

The `unreachable` arm takes the explicit residual variant list, never
a `_` catch-all (rejected at expansion time): whittle's error enums
are closed sums, so a new source variant breaks every declaration at
compile time, and a residual pattern repeating a mapped variant trips
the `unreachable_patterns` deny emitted inside the generated mapper.
A total mapping omits the arm. For `Or<...>` compositions (`[E; 2]`)
a public domain API still hand-writes the newtype and collapses the
pair into named variants.

The narrowing pipeline is the composed rule type itself — transformer
and validation rules composed in declaration order — per
[IDEA.md](IDEA.md) §5.10 as amended; the macro accepts that composed
type as its single source of truth and has no step DSL. Generation of
the remaining IDEA §5.10 artifacts (schema reflection, declared
implication edges) is queued (Section 15.3).

### 13.2. deserialize_rule!

```rust
deserialize_rule! {
    impl[const N: i64] DeserializeRule<i64> for MultipleOf<N>
}
```

Stamps a rule's `DeserializeRule` impl with the default
parse-then-refine body (Section 8.2). An optional `where [...]` clause
carries whatever extra bounds the rule's own `Rule` impl needs. Rules
that bound input size hand-write the hook instead.

### 13.3. closed_set!

```rust
closed_set! {
    /// Account activity status.
    pub enum ActivityStatus {
        Active = "active",
        Inactive = "inactive",
    }
}
```

Declarative codegen for the closed-set family (Section 11): generates
the enum (with the full forwarded derive set), the `ClosedSet` impl
whose `MEMBERS` table is in declaration order, a `const` forcing
`VALID` at declaration time, `FromStr` / `TryFrom<&str>` / `Display`
forwarding to `parse` / `as_str`, and — when the `serde` feature is
enabled — `Serialize` / `Deserialize` impls over the plain-wire-string
codec. Generating enum and table from one declaration list makes
"variant without a wire string", "wire string without a variant", and
"variant declared twice" unrepresentable in the declaration artifact.

### 13.4. pattern!

```rust
type Name = whittle::pattern!(r"^(?:[A-Z])(?:-?[A-Za-z]+)*$");
```

A function-like proc-macro (the `whittle-macros` crate's entire
surface) that parses its argument as a string literal, validates it as
a regular expression **at compile time**, and expands to the
const-generic rule type `Pattern::<"...">` (Section 10.11). A malformed
pattern is a compile error at the literal's span instead of a runtime
panic on first construction. The expansion resolves its path through
`proc-macro-crate`, so it works for consumers of the facade and of
`whittle-core` alike.

## 14. Testing Architecture

### 14.1. Core Tests

Unit and property tests are co-located with each rule (`mod tests` in
the module file). Every library-supplied rule has:

- accept/reject unit tests at the admissible region's boundaries;
- the property tests [IDEA.md](IDEA.md) §5.14 requires (admissibility,
  idempotence on admissible inputs, canonicalisation determinism,
  implication-edge admissibility preservation);
- doctests in the house style: each public item shows at least one
  admit and one reject example;
- `compile_fail` doctests for the compile-time gates (the `VALID`
  consts of `Within` / `InClosedRange` / `Implies` / `ClosedSet`, and
  malformed `pattern!` literals).

### 14.2. Property Harness (proptest feature)

`whittle-core::testing` ships the `f: A → B` harness: for a function
whose domain is a refined type, `prop_total(f)` asserts `f` never
panics over `Arbitrary`-generated admissible inputs, and
`prop_image_refines::<RB, _, _, _>(f)` additionally asserts `f`'s image
satisfies a stated output rule. When `f` already returns a refined `B`,
image-validity is discharged by the return type and `prop_total` alone
is the right call — the harness rustdoc states the obligations and the
"delete the test the type proves" rule. Entry points are closure-taking
functions (not strategy combinators) so the input set has exactly one
determinant: `A`'s `Arbitrary` impl.

### 14.3. Integration Tests

The root `tests/` corpus exercises the public surface as a consumer
would, including:

- serde round-trips and rejection fixtures proving invariant-violating
  payloads are rejected with typed errors ([IDEA.md](IDEA.md) §5.3);
- the closed-set codec and the derived reject-input generator;
- the `Arbitrary` derivations and the property harness;
- the domain-newtype, transformer, and composition patterns;
- the `pattern!` macro (expansion and compile-fail).

Targets that need optional features declare `required-features`, and
`just test-default-build` plus the pre-push hook keep the
default-feature build compiling (Section 5).

### 14.4. Coverage

The pre-push hook enforces 100% coverage on four axes — regions,
functions, lines, and branches — via the `cargo coverage` alias
(Section 5). New code lands with its tests in the same commit or the
push fails.

## 15. Planned Milestones

This section collects the designs that [IDEA.md](IDEA.md) requires or
admits but that are not yet built. Each entry names its evidence
trigger. Nothing in this section is implemented; earlier revisions of
this document carried concrete sketches for some of them (a `Schema`
enum, a `StrategyFromSchema` trait, a `RuleWith` family, a
`refinement!` step DSL), which are retrievable from git history but are
not normative.

### 15.1. Schema Reflection

[IDEA.md](IDEA.md) §5.9. A runtime-introspectable schema per rule,
sufficient to drive derived property strategies (the §5.11 destination;
per-rule `ArbitraryRule` impls are the sanctioned interim per the
amendment), JSON Schema generation, human-readable rule descriptions,
and schema equality. It is also the intended carrier for the dogfooding
audit's residual-set reporting (R-S2), storage-constraint
synchronisation (R-S4), and boundary-matrix generation (R-T1). Build it
constructor-by-constructor; each schema constructor that lands SHOULD
convert its rule family from hand-written to derived generation.

### 15.2. Contextual Rules

[IDEA.md](IDEA.md) §5.8. A contextual companion to `Rule` with borrowed
and owned carriers (the `witnessed`-informed design). Evidence trigger:
the first dogfooding adoption that reaches a genuine
environment-dependent invariant — one candidate site is identified (the
symbiote consumer's `RecoveredParentInvocation`); implement when that
adoption lands rather than speculatively.

### 15.3. refinement! Generation-Completeness

[IDEA.md](IDEA.md) §5.10. The error-block form (Section 13.1)
generates the newtype, the named typed-error enum with mapped
variants, the `Deserialize` impl, and the read-only delegating
surface from one declaration. The remaining §5.10 artifacts — schema
reflection (Section 15.1) and declared implication edges — are queued,
generated from the same single declaration when they land.

### 15.4. Enum-Subset Markers

[IDEA.md](IDEA.md) §5.6(b) as amended. Subset rules over an existing
enum are deferred: a smaller local enum with a
`From<Local> for Foreign` impl strictly dominates in every observed
case. Evidence trigger: the documented hard case — overlapping
subsets of one
(typically foreign) enum where per-subset local enums explode
combinatorially.

### 15.5. PureFilter Marker

[IDEA.md](IDEA.md) §5.1 admits a marker trait (named `PureFilter`) for
rules whose `refine` is the identity on admissible inputs, so derived
integrations can exploit byte preservation. No shipped integration
exploits it yet; add it together with the first consumer (codec
inversion or JSON Schema generation).

### 15.6. Ecosystem Integrations

[IDEA.md](IDEA.md) §3 names `quickcheck`, `schemars`, and `sqlx` as
optional integration targets. None has consumer demand yet; `schemars`
additionally depends on schema reflection (Section 15.1). Each would be
a Cargo feature, with impls hosted wherever the orphan rule requires.

## 16. References

### 16.1. Normative References

[RFC2119] Bradner, S., "Key words for use in RFCs to Indicate
Requirement Levels", BCP 14, RFC 2119, March 1997,
<https://www.rfc-editor.org/rfc/rfc2119.html>.

[RFC8174] Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119
Key Words", BCP 14, RFC 8174, May 2017,
<https://www.rfc-editor.org/rfc/rfc8174.html>.

### 16.2. Informative References

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
