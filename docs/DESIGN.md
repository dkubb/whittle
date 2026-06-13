# Whittle: High-Level Design Sketch

> Status: high-level narrative that summarises the normative documents
> at sketch level. [IDEA.md](IDEA.md) is the authoritative
> specification; [ARCHITECTURE.md](ARCHITECTURE.md) is the concrete
> technical design. Where this sketch differs from either of those
> documents, the other documents win. The role of this file is to read
> top-to-bottom in one sitting and to convey the shape of the library;
> it is not a substitute for the normative texts.

## What Whittle is

Whittle is a Rust library that takes untrusted raw values at the edge of
a program and produces *refined values* whose invariants the rest of the
program can trust without re-checking. A rule/newtype declaration can
drive:

- the typed newtype that wraps the raw inner value;
- the smart constructor that narrows the raw input into the refined
  type's admissible state space;
- the typed error variants the constructor returns on rejection;
- the `Deserialize` impl that routes through the constructor, so wire
  payloads cannot bypass the invariant;
- the reflectable schema describing expressible rules, which drives
  boundary matrices, schema cross-checks, and residual-state reports.

The kernel is **parse-don't-validate** with type narrowing: every rule
is a morphism that maps a larger raw state space into a smaller
admissible one. `Refined<T, R>` adds no runtime bytes to the carrier;
pure filters preserve the input bytes, while transformers store the
canonical value they produce. Downstream code does no further checks.

Whittle does **not** attempt to be a compiler-assisted refinement-type
system in the Liquid Haskell sense. There is no SMT solver, no rustc
plugin, no type-level proof discharge. What Whittle delivers is the
*externally observable behaviour* such a system would provide,
implemented with ordinary Rust constructors that run at the boundary.

## Goals

1. Make parse-don't-validate the path of least resistance.
2. Provide one uniform interface so every constrained type in a
   codebase looks the same.
3. Drive validation and deserialization from a single declaration so
   `Deserialize` cannot bypass the constructor.
4. Make refinement composable: pipelines of normalisation and
   validation steps, library-supplied primitives, user-defined rules.
5. Derive boundary probes, schema cross-checks, and residual-state
   reports from the rule's reflectable schema.
6. Dogfood against multiple real consumers from day one.

## Non-Goals

- Compiler-assisted refinement types (no SMT solver, no Liquid-style
  proof discharge).
- A nominal-newtype derive for IDs without validation — `branded`
  handles that case and the concerns are different.
- An HTTP framework, a database adapter, a serialization format —
  Whittle sits below those layers and integrates with them via
  optional Cargo features.

---

## The Kernel

Two types and two traits.

```rust
pub trait Rule<T: 'static>: Sized + 'static {
    type Error;
    fn refine(raw: T) -> Result<T, Self::Error>;
}

pub trait SchemaRule<T: 'static>: Rule<T> {
    fn schema() -> Schema;
}

#[repr(transparent)]
pub struct Refined<T, R> {                    // bound is on the impl,
    inner: T,                                  // not on the struct,
    _rule: PhantomData<R>,                     // so accessors are free
}

impl<T: 'static, R: Rule<T>> Refined<T, R> {
    pub fn try_new(raw: T) -> Result<Self, R::Error> {
        R::refine(raw).map(|inner| Self { inner, _rule: PhantomData })
    }
}
```

`refine` is the narrowing morphism. It **consumes** raw input
(consume-and-rebuild) so it may canonicalise — trim whitespace,
lowercase a scheme, reorder commutative operands — not just inspect.
Rules whose narrowing is purely a predicate return `Ok(raw)` unchanged.
`SchemaRule` is the opt-in constructive surface for rules whose
admitted set fits Whittle's schema vocabulary; rules outside that
vocabulary have no schema impl rather than an opaque schema node.

`Refined<T, R>` is `#[repr(transparent)]` over `T`. The phantom
marker is zero-sized. The runtime bytes of `Refined<String, R>` are
identical to the bytes of `String`. Niche optimisations on `T` are
preserved.

Existence of a `Refined<T, R>` IS the proof. There is no `Witness`
token, no separate proof object, no admissible-input carrier. The type
itself carries the invariant.

`'static` is on the rule marker `R`. The kernel's `Rule<T>` also
bounds `T: 'static` so the `Schema` reflection can use
`TypeId::of::<T>()`. Refined types in practice are owned (`String`,
`i64`, `Decimal`, `Url`, ...), not borrows; the constraint matches
what consumers actually want.

Proof preservation across operations:

- `try_new` is the **proof-introducing** morphism — the only path from
  a raw `T` to a `Refined<T, R>`.
- `as_inner(&self) -> &T` is a **proof-preserving** observation —
  callers see the inner value through a borrow but cannot reconstruct
  a `Refined<T, R>` from it without going back through `try_new`.
- `into_inner(self) -> T` is the **proof-erasing** morphism — the
  caller takes ownership of the inner value, but reconstruction
  requires `try_new` and pays the validation cost again.
- `Refined<T, S>::weaken::<W>()` when `S: Implies<W>` is a
  **proof-weakening** morphism — no re-narrowing, but the stronger
  proof becomes the weaker one.

## Named domain types

Domain types are newtypes over `Refined<T, R>`, never type aliases:

```rust
pub struct AttributeName(Refined<String, AttributeNameRule>);
```

The `refinement!` macro (ARCHITECTURE Section 13.1) generates the
newtype, `try_new`, `as_inner`, and `into_inner`. Its error-block form
also generates the named domain error enum, `Display` /
`core::error::Error`, the `ErrorMapper` impl used by construction and
serde ingress, `AsRef`, optional carrier `Display`, and serde glue when
the `serde` feature is enabled. Schema reflection is not emitted by the
macro; it comes from `SchemaRule` impls on the composed rule type.

The discipline is: **the type-alias form is forbidden**. Declaring
`pub type AttributeName = Refined<String, AttributeNameRule>;` leaks
the representation and the rule's vocabulary into the public API and
pins the representation if the inner type later changes (`String` →
`SmolStr` → `Arc<str>` → interned). Newtypes with macro-generated
delegating surfaces are the only way.

## Library primitives

For common shapes, the library provides ready-made rules so users do
not implement `Rule<T>` by hand for every domain type:

```rust
// Numeric range bounds and aliases.
Within<MIN, MAX>, AtLeast<MIN>, AtMost<MAX>,
GreaterThan<MIN>, LessThan<MAX>, EqualTo<N>, NotEqualTo<N>,
NonZero, Positive, Negative

// Float-specific.
Finite, NotNan, NotInfinite,
InClosedRange<MIN_NUMERATOR, MIN_DENOMINATOR, MAX_NUMERATOR, MAX_DENOMINATOR>

// Decimal-specific (rust_decimal feature).
DecimalPrecision<P>, DecimalScale<S>, DecimalPositive,
DecimalInRange<MIN_REPR, MAX_REPR, SCALE>

// String grammar and predicates.
LenChars<MIN, MAX>, LenBytes<MIN, MAX>, NonEmpty, EachChar<P>,
FirstChar<P>, CharLiteral<CH>, CharEither<A, B>, CharExcept<A, B>,
AsciiGraphic, AsciiAlphanumeric, AsciiAlphabetic, AsciiUppercase,
AsciiLowercase, AsciiDigit, IdentChar, IdentStart, IdentDashChar,
NonControl

// Feature-gated string and pattern rules.
HexChar, HexFixedLower<LEN>, HexFixedAny<LEN>, HexFixedNormalized<LEN>,
PrintableLine, PrintableMultiline, PrintableChar,
BoundedLine<MAX>, BoundedText<MAX>, Pattern<RE>

// Collections.
LenItems<MIN, MAX>, AllItems<R>, UniqueByKey<T, F>, Sorted<T, K>,
NoneOf<P>, AnyOf<P>

// Closed provider token sets.
closed_set! { pub enum ActivityStatus { Active = "active" } }

// Composition, transformers, and schema reflection.
And<A, B>, Or<A, B>, MapErr<R, M>,
Not<R>, Xor<A, B>, // numeric carriers
All<(R1, ..., RN)>, Any<(R1, ..., RN)>,
Trim<R>, AsciiLowercase<R>, AsciiUppercase<R>, SchemaRule<T>
```

Domain types newtype over these:

```rust
pub struct Port(Refined<u16, AtLeast<1>>);
pub struct Percent(Refined<u8, Within<0, 100>>);
// 0.0..=1.0 expressed as numerator/denominator endpoint pairs.
pub struct Probability(
    Refined<f64, And<InClosedRange<0, 1, 1, 1>, NotNan>>,
);
```

## Composition: normalisation and validation in one pipeline

Real refinements often combine canonicalisation with validation. A doc
title trims whitespace, then checks the result is 1..=200 printable
line characters. The macro accepts the composed rule type as the single
determinant:

```rust
use whittle::{All, refinement};
use whittle::primitive::{EachChar, LenChars, NonEmpty, PrintableLine};
use whittle::transform::Trim;

refinement! {
    pub DocTitle: String,
        All<(Trim<NonEmpty>, LenChars<1, 200>, EachChar<PrintableLine>)>;
}
```

The operands run in tuple order. Canonicalisation steps come first
because bounds and character checks should see the carried value —
trimming to `""` first lets `NonEmpty` reject what would otherwise have
been `" "` (a single space).

For cases the structural vocabulary cannot express, write a hand-written
`Rule<T>` impl and use it as the rule type. That absence is visible to
schema tooling: no `SchemaRule` impl exists unless the rule author
provides one.

## Subtyping: implication

When one rule is strictly stronger than another, the upcast from
strong to weak is free, exposed as an explicit method:

```rust
pub trait Implies<Weaker> {}

impl<T: 'static, S: Rule<T>> Refined<T, S> {
    /// Upcast to a refined value carrying the weaker rule.
    /// No re-narrowing; the inner value is moved.
    pub fn weaken<W: Rule<T>>(self) -> Refined<T, W>
    where S: Implies<W>
    { /* move inner, change phantom */ }
}
```

A `weaken` method is used instead of a blanket
`From<Refined<T, S>> for Refined<T, W>` impl because such an impl
overlaps with the reflexive `From<X> for X` in `core::convert` and is
rejected by Rust's coherence checker. `weaken` is the explicit
upcast path.

The contract is: every value the strong rule admits also satisfies
the weak rule; the strong canonical form is canonical-enough for the
weak rule; the weak rule has no observable behaviour that depends on
re-running its narrowing on the upcast value.

Library-supplied implication edges cover the common cases —
`Within<0, 50>: Implies<Within<0, 100>>`, `Positive: Implies<NonZero>`,
`AtLeast<10>: Implies<AtLeast<5>>` — for numeric and length families.
Edges for arbitrary user rules are written by the user; implication is
*not* derived automatically from rule structure. Canonicalising
transformers are especially sensitive because the target rule may need
properties of the carried form, not just accept/reject containment.

Formally, `S: Implies<W>` requires the implementer to discharge
three obligations: (1) every value `S` admits also satisfies `W`
(admissible-set containment); (2) when `W` canonicalises, every
value `S::refine` produces is already in `W::refine`'s range
(canonical compatibility); (3) the weaker rule `W` has no observable
behaviour that depends on re-running its narrowing morphism on the
upcast value (so `weaken` can skip narrowing).

## Contextual rules

Some invariants only make sense relative to a runtime environment.
"This `Vec` index is valid for *this* slice." "This `AttributeName` is
present in *this* schema." "This value is valid for the configuration
loaded at startup." Whittle does not ship contextual carriers today;
ARCHITECTURE Section 15.2 keeps that design as a planned milestone with
an evidence trigger.

```rust
pub trait RuleWith<T: 'static, Env: 'static>: Sized + 'static {
    type Error;
    fn refine_with(env: &Env, raw: T) -> Result<T, Self::Error>;
}
```

Contextual rules do not emit a default schema. Their admitted set
depends on a runtime environment, so the absence of `SchemaRule` is the
audit boundary. Until `RuleWith` lands, dynamic or cross-field
invariants belong in the parent smart constructor. The current
residual-state report renders absence as `opaque (hand-written refine)`,
with no `Opaque` or `ContextOpaque` schema node.

## Schema reflection drives the rest

Library-supplied rules and macro-generated rules whose admitted set fits
the schema vocabulary emit a runtime-introspectable `Schema`
description through `SchemaRule`. The schema is enough to derive:

- **Boundary matrices** that probe schema-derived accept/reject edges
  against the rule's own `refine` implementation.
- **Schema cross-checks** that compare generated strategies with the
  reflected carried set.
- **Residual-state reports** for macro users, including explicit
  absence for hand-written or context-dependent rules whose admitted
  sets are outside the schema vocabulary.
- **Human-readable rule descriptions** for generated documentation and
  audit output.
- **Equality and ordering on schemas** so two refined types with
  identical schemas can be detected as such.

This is the single biggest divergence from `refined`/`witnessed`: the
rule is not just a checker, it's a *reflectable description* of the
admissible state space. The reflection is what makes the derived
test oracles possible. External ecosystem exports such as `schemars`
JSON Schema fragments remain deferred until consumer demand.

## Zero-cost layout

```text
size_of::<BoundedPrintable>()              == size_of::<String>()
size_of::<Option<BoundedPrintable>>()      == size_of::<Option<String>>()
size_of::<Refined<u16, AtLeast<1>>>()      == size_of::<u16>()
size_of::<Refined<f64, NotNan>>()          == size_of::<f64>()
```

No headers, no tags, no length prefixes added by the carrier. Phantom
markers are zero-sized; `#[repr(transparent)]` guarantees layout
identity. Niche optimisations on the inner type are preserved.

The cost of validation is paid at the boundary, once, by `try_new` —
that's the cost the boundary exists to pay. Access after construction
is the same as access on the raw type.

For composition — the `Refined<T, S>::weaken::<W>()` upcast when
`S: Implies<W>` holds — the cost is one struct move of the inner
value. For `Copy` inner
types it's one machine instruction; for owned types it's a (ptr, len,
cap)-triple copy, no deep copy. The compiler usually elides the move
when the conversion is inlined.

## What's dogfooded

The library is built to be dogfooded against multiple Rust consumers
from day one. The first migrations:

- **symbiote**: `BoundaryEvent`, `ModelResultBytes`, `ProcessId`,
  `PortName`, `ReplayCounter`. The `BoundaryEvent` case stresses
  cross-field invariants and is the test of whether `Rule<MyStruct>`
  scales to composite types.
- **incremental-gate**: `RepoRelativePath`, `ChangeSetFile`,
  `CheckpointBranchRef`, `ChangeSetCapacity`. The `RepoRelativePath`
  case stresses canonicalisation (path normalisation); the
  `ChangeSetFile` case stresses enum-subset refinement.
- **axiom-rs**: `AttributeName`, `Schema`, `Predicate`, `Op` — once
  the kernel is mature, the constraint-propagation system in axiom-rs
  becomes a third consumer.

Each migration step surfaces one design pressure. By the time Whittle
reaches a 1.0 release, every structural question has been answered by
real code in real consumers.

## Open questions

Open questions are tracked authoritatively as planned milestones in
[ARCHITECTURE.md](ARCHITECTURE.md) Section 15. Highlights:

- Contextual rules (`RuleWith`) and their carriers.
- Macro-declared implication edges.
- Enum-side subset markers, if overlapping foreign subsets ever need
  them.
- Generic-const-expr implication edges (blocked on stable Rust).
- Ecosystem exports such as `schemars` JSON Schema fragments.

## Minimal example, end to end

```rust
use whittle::All;
use whittle::primitive::{EachChar, LenChars, NonEmpty, PrintableLine};
use whittle::refinement;
use whittle::transform::Trim;

type BoundedPrintableRule =
    All<(Trim<NonEmpty>, LenChars<1, 100>, EachChar<PrintableLine>)>;

refinement! {
    /// A 1..=100 printable-line string. Leading/trailing whitespace is
    /// trimmed before validation.
    pub BoundedPrintable: String, BoundedPrintableRule;
}

fn handle_request(body: &str) -> Result<(), MyError> {
    // Construction routes through the rule.
    let title = BoundedPrintable::try_new(body.to_string())?;
    do_something_with(&title);
    Ok(())
}
```

Serde and property-test integration are shown in the runnable corpus:
`tests/serde-roundtrip.rs`, `tests/proptest-arbitrary.rs`, and
`tests/property-harness.rs`.

That's the whole library, at sketch level: a single declaration, one
constructor surface, a refined value that the rest of the program can
trust.
