# Whittle Idea Requirements

Status of This Memo

This document is an internal project specification written in an RFC-style
Markdown form. The document borrows structure and editorial discipline from
RFC 7322 and uses RFC 2026 as process vocabulary for maturity, review, and
applicability.

This document is authoritative for the Whittle project, a Rust library that
narrows a value's state space at construction so the type system can carry
invariants downstream without runtime re-checking. When this document
conflicts with any other project artifact, this document takes precedence
for goals, scope, non-goals, and invariants. The companion
[ARCHITECTURE.md](ARCHITECTURE.md) document is authoritative for concrete
architecture and technology choices only when those choices preserve this
document's requirements.

Abstract

This document defines the goals, scope, product model, non-goals, and
invariants for a Rust library that lets a single declaration drive runtime
construction, runtime canonicalisation, deserialization, and downstream
trust for a constrained type. The library implements the externally
observable behaviour a refinement-type system would provide — predicate-
and normalisation-bearing rules attached to values and propagated through
explicit subtyping — without committing to a compile-time proof-discharge
mechanism (no SMT solver, no rustc plugin). The result is a library that
can stand at the boundary of any Rust system, accept untrusted input, and
hand the rest of the program values whose invariants are guaranteed by
construction.

Table of Contents

- [Section 1: Introduction](#1-introduction)
- [Section 2: Requirements Language](#2-requirements-language)
- [Section 3: Scope](#3-scope)
- [Section 4: Product Model](#4-product-model)
- [Section 5: Normative Requirements](#5-normative-requirements)
  - [5.1 Type Narrowing](#51-type-narrowing-as-the-primary-operation)
  - [5.2 Smart Constructor](#52-smart-constructor-as-the-only-construction-path)
  - [5.3 Deserialization Gating](#53-deserialization-gating)
  - [5.4 Zero-Cost Layout](#54-zero-cost-layout)
  - [5.5 Per-Rule Typed Errors](#55-per-rule-typed-errors)
  - [5.6 Library-Supplied Primitives](#56-library-supplied-primitive-rules)
  - [5.7 Implication and Subtyping](#57-implication-and-subtyping)
  - [5.8 Contextual Rules](#58-contextual-rules)
  - [5.9 Reflectable Schema](#59-reflectable-schema)
  - [5.10 Declarative Refinement Macro](#510-declarative-refinement-macro)
  - [5.11 Derived Property Generators](#511-derived-property-generators)
  - [5.12 Bidirectional Codecs](#512-bidirectional-codecs)
  - [5.13 Bounded Inputs](#513-bounded-inputs)
  - [5.14 Testability](#514-testability)
- [Section 6: Non-Goals](#6-non-goals)
- [Section 7: Reliability and Security Considerations](#7-reliability-and-security-considerations)
- [Section 8: References](#8-references)

## 1. Introduction

A great deal of Rust code spends its first lines turning untrusted input
into typed values that the rest of the code can trust. The conventions are
familiar — newtype wrappers, `try_new` constructors, sealed fields,
validation passes — but they are ad-hoc per type and the rules cannot be
inspected, composed, or derived from. Existing crates address parts of the
problem: `branded` gives nominal newtypes without validation; `refined`
gives validation but not canonicalisation; `witnessed` adds contextual
witnesses but no normalisation, no macro, no library primitives, and no
property-test integration.

Whittle is a library for the whole problem. A single declaration — through
a derive or a declarative macro — defines:

- a refined type that wraps a raw inner type;
- the smart constructor that narrows raw input into the refined type's
  admissible state space, canonicalising as it goes;
- the typed error variants the constructor returns on rejection;
- the `Deserialize` impl that routes through the constructor so wire
  payloads cannot bypass the invariant;
- the reflectable schema describing the rule, which drives shipped
  boundary matrices, schema cross-checks, residual-state reports,
  human-readable descriptions, and equality/order; external exports
  such as JSON Schema remain follow-ups when consumer demand appears.

The kernel is parse-don't-validate with **type narrowing**: every rule is a
morphism that maps a larger raw state space into a smaller admissible one,
with the carrier's bytes unchanged after construction. The library is
designed to be **dogfooded** against multiple Rust consumers from day one;
its design pressure comes from real code, not from abstract elegance.

Whittle does not attempt to be a compiler-assisted refinement-type system
in the Liquid Haskell sense. There is no SMT solver, no type-level proof
discharge, no rustc extension required. What Whittle delivers is the
externally observable behaviour such a system would provide, implemented
with ordinary Rust constructors that run at the boundary.

## 2. Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP 14
[RFC2119] [RFC8174] when, and only when, they appear in all capitals, as
shown here.

Lowercase uses of these words have their ordinary English meanings.

## 3. Scope

Whittle is a Rust library, not a service or daemon. It does not own a
network listener, does not retain persistent state, and does not require
an async runtime in its core.

The library targets consumers that need parse-don't-validate at the
boundary of their system: any Rust program that turns untrusted input
(JSON, HTTP request bodies, environment variables, command-line arguments,
file contents, third-party API responses) into typed values whose
invariants the rest of the program relies on.

The library is intended to be dogfooded against multiple existing Rust
projects from day one, including projects whose smart-constructor
conventions predate Whittle. Migration cost MUST be small enough that
adopting Whittle in a single named domain type costs less than writing
that type's bespoke smart constructor by hand.

Whittle is a single-purpose library: it narrows state spaces and carries
proofs of narrowing. It does not provide HTTP frameworks, serialization
formats, database adapters, or any other concern beyond the narrowing
boundary itself. Optional Cargo features integrate Whittle with adjacent
ecosystems (`serde`, `proptest`, `quickcheck`, `schemars`, `sqlx`) without
adding any of those as required dependencies.

The library makes no compile-time proof-discharge claims. Every invariant
Whittle carries is enforced by runtime validation at construction.
Downstream code that holds a refined value relies on Rust's ordinary
trust-the-type discipline; the proof is "this type exists, therefore its
constructor ran successfully," not a type-level theorem. [Amended
2026-06-11: in the state-space-minimization formal vocabulary, this is
a predicative encoding at rank 2 (constructor/parser) of the encoding
order. Structurally `S(Refined<T, R>) ≅ S(T)` — the carrier is
`#[repr(transparent)]` over `T` — while the admissible set is
`C = { t ∈ T : R::refine(t) = Ok(…) }`. `try_new` is the boundary
morphism: it restricts the reachable set to `C`, so `I_reach = ∅` even
though `I_repr = S(T) \ C ≠ ∅`. `into_inner` is proof-erasing; `weaken`
(Section 5.7) is the proof-preserving implication morphism. The claim
is therefore boundary discharge, not structural discharge, which is why
Section 5.2's single-construction-path requirement is load-bearing.]

The library is single-developer in origin but is intended for ecosystem
adoption. Public API decisions favour stability and consumer convenience
over the author's preferences.

## 4. Product Model

The product model contains these concepts:

- raw input type;
- admissible state space;
- narrowing morphism;
- rule;
- refined value;
- rule error;
- canonicalisation step;
- predicate step;
- contextual rule;
- contextual environment;
- implication edge;
- implication contract;
- declarative refinement macro;
- derive macro for refined newtypes;
- reflectable schema;
- property strategy;
- library-supplied rule (numeric, string-grammar, collection, decimal,
  float-finiteness).

The model intentionally distinguishes:

- the **rule**, which is a compile-time type that knows how to narrow;
- the **refined value**, which is the runtime carrier of a successfully
  narrowed value;
- the **schema**, which is a reflectable runtime description of the rule
  used by shipped introspection and test oracles (boundary matrices,
  schema cross-checks, residual-state reports, descriptions, and
  equality/order), with ecosystem exports kept as deferred integrations.

[Amended 2026-06-11: the model's names map onto the
state-space-minimization formal vocabulary — the admissible state space
is `C`; the narrowing morphism, realised by the smart constructor, is
the boundary morphism through which all trust increases (`I_reach = ∅`
over the predicative carrier, whose `I_repr` stays non-empty); the
implication edge is the proof-preserving morphism realised by `weaken`;
`into_inner` is the proof-erasing projection back to the raw input
type.]

A consumer-facing **named domain type** is the public face — a Rust
newtype that wraps a refined value and exposes a delegating surface
(`AsRef`, `Display`, `Deserialize`, ...). The rule itself is the
implementation substrate; the library MUST design the named-domain-
type API so consumers need not name the rule type to use the domain
type.

## 5. Normative Requirements

### 5.1. Type Narrowing as the Primary Operation

The library MUST implement **type narrowing** as the core operation: a
rule is a morphism from a raw input type's state space into a (typically
smaller) admissible state space. The morphism MAY canonicalise inputs
(map multiple raw inputs to one canonical form) but MUST NOT widen the
state space.

The trait method that defines a rule's narrowing morphism MUST consume
its input by value, not by reference. Rules whose narrowing is purely
a predicate (the input passes unchanged when admissible) are permitted;
the consume-and-rebuild signature does not force allocation when none
is needed.

The narrowing morphism MAY canonicalise admissible inputs (its range
MAY be a proper subset of its admissible inputs) but MUST NOT have a
range outside the admissible set. Rule implementers MUST discharge a
soundness obligation: for every `R: Rule<T>` and every raw input `x`,
if `R::refine(x)` returns `Ok(y)` then `y` MUST be admissible under
`R`. The library MUST verify this obligation by property test for
every library-supplied rule; user-defined rules SHOULD discharge it
by property test as well.

Pure-predicate rules MAY be marked by an additional trait (named
`PureFilter` in the architecture document) so schema composition can
exploit the information-preserving property. Future external consumers
such as codec inversion or JSON Schema generation MAY use the same
marker. The kernel does not require this marker.

### 5.2. Smart Constructor as the Only Construction Path

A refined value MUST be constructible only through `try_new` (or its
named-newtype delegate). The inner field of the refined-value carrier
MUST be private to the carrier's defining module. Public accessors
MUST NOT return a mutable reference to the inner value. Public
accessors MAY return an owned copy via `into_inner`, but `into_inner`
is a proof-erasing morphism: downstream code cannot reconstruct a
refined value from the copy without re-running the narrowing morphism
through `try_new`.

Constructors MUST come in this shape:

- `Refined<T, R>::try_new(raw: T) -> Result<Self, R::Error>` — the
  external surface that calls into the rule's narrowing morphism;
- a corresponding `try_new` on every named domain newtype that delegates
  to the underlying refined value's constructor.

`try_new` and its named-newtype delegate MUST be the sole external
construction surface for refined values; no other public path MAY
produce a `Refined<T, R>`. Existence of a `Refined<T, R>` is itself
the proof its rule was satisfied at construction; the library MUST
NOT introduce a separate `Witness` or proof-token type carrying the
proof alongside the value.

Infallible constructors MUST NOT be exposed for rules whose narrowing
can fail. If a rule's narrowing morphism is the identity on its input
type (the raw input type and admissible state space coincide), no
`try_new` is required, and ordinary Rust newtype construction is
sufficient — but such types are not refined values and do not
implement the rule trait.

### 5.3. Deserialization Gating

Implementations of `serde::Deserialize` (and any other deserialization
path the library exposes) MUST route through `try_new`. There MUST NOT
be any admissible code path that produces a refined value without
running the narrowing morphism the constructor runs.

Tests MUST include, for every constrained type, at least one
deserialization fixture proving that an invariant-violating wire payload
is rejected with a typed error, not silently accepted.

### 5.4. Zero-Cost Layout

The unconditional refined-value carrier's runtime layout MUST be
identical to the layout of its raw inner type. The carrier MUST NOT
add headers, tags, length prefixes, or other per-value bytes. Phantom-
typed markers used to carry the rule identity MUST be zero-sized.

Niche optimisations available on the raw type MUST be preserved by
the carrier. Field access on a refined value MUST cost the same as
field access on its raw inner type — there MUST NOT be hidden
re-validation, bookkeeping, or indirection at access time.

The borrowed contextual carrier (Section 5.8) MUST also satisfy this
layout requirement: its proof carrier is a `PhantomData<&'a Env>`
which is zero-sized. The owned contextual carrier (Section 5.8) is
the only carrier where the requirement does not apply, because the
owned carrier stores the environment value to preserve construction-
time identity. Section 5.8 documents this trade-off as the intended
behaviour for that carrier.

This requirement constrains the carrier, not the constructor. The
constructor pays the cost of running the narrowing morphism once at
the boundary; that cost is the contract this library exists to
enforce.

### 5.5. Per-Rule Typed Errors

Each rule MUST define its own error type as an associated type. Errors
MUST distinguish the specific failure mode (length below minimum, length
above maximum, character outside admissible class, value outside
admissible range, non-finite float, etc.) with enough structured detail
for a caller to construct a useful error message or branch on the cause.

Stringified-error patterns (a single error variant carrying a formatted
message) are NOT RECOMMENDED for library-provided rules. User-defined
rules MAY use any error type that implements `std::error::Error`.

Named domain newtypes MUST expose their error under a named alias so
that call sites need not name the rule type. The exposed name MUST live
in the same module as the newtype.

### 5.6. Library-Supplied Primitive Rules

The library MUST provide a set of rule primitives covering at least:

- numeric range bounds for signed and unsigned integers
  (`Within<MIN, MAX>`, `AtLeast<MIN>`, `AtMost<MAX>`);
- non-zero, positive, negative markers for numeric types;
- finiteness and non-NaN markers for floating-point types;
- precision and scale bounds for fixed-point decimal types;
- length bounds for collections and strings;
- per-element character-class predicates for strings;
- non-empty markers for collections and strings;
- closed-set parses for enumerated types: a wire string admitted
  against a single declared injective string ↔ variant table and
  parsed into the enum itself, with the parse, the wire form, the
  typed error's expected set, and the derived integrations all
  determined by that one table. [Amended 2026-06-11: the original
  bullet required subset-of-sum markers (accept only listed
  variants of an enum). The requirement is split: (a) closed-set
  parse into a full enum is MUST and is satisfied by the
  `closed_set!` family; (b) enum-side subset markers over an
  existing enum are deferred — a smaller local enum with a
  `From<Local> for Foreign` impl strictly dominates in every
  observed case, so (b) waits for the documented hard case:
  overlapping subsets of one (typically foreign) enum where
  per-subset local enums explode combinatorially.];
- binary composition operators (`And`, `Or`); n-ary composition is
  expressible by nesting (the declarative macro performs the nesting
  on the user's behalf) so a single domain type can layer length,
  character class, and additional predicates without bespoke code.

Library-supplied rules MUST themselves satisfy every requirement in
this document, including the typed-error and reflectable-schema rules.

### 5.7. Implication and Subtyping

When one rule is logically stronger than another (every value the
stronger rule admits also satisfies the weaker rule), the library MUST
make the implication expressible through an explicit trait.

Implementations of the implication trait MUST be either library-supplied
for documented common cases (numeric range narrowing, collection-length
narrowing) or explicitly written by the user. The library MUST NOT
attempt to infer arbitrary implications from const expressions or
generic constraints; the trade-off between completeness and predictable
compile times falls on the side of predictability.

An explicit upcast from the stronger `Refined` to the weaker
`Refined` MUST be provided whenever the implication trait holds. The
upcast MUST NOT re-run either rule's narrowing morphism. The
architecture document specifies the concrete shape of this upcast: a
blanket `From` impl overlaps with the reflexive `From<X> for X` in
`core::convert` and is rejected by Rust's coherence checker, so the
upcast is exposed as an explicit `Refined<T, S>::weaken::<W>()`
method rather than a `From` impl.

The implication contract MUST satisfy three properties. Let `adm(R)`
denote the admissible state space of a rule `R` and let `R::refine`'s
range denote the set of values `R::refine` produces on admissible
inputs. For `S: Implies<W>`, implementations MUST establish:

1. `adm(S) ⊆ adm(W)` — every value the stronger rule admits also
   satisfies the weaker rule;
2. when `W` canonicalises, every value in the range of `S::refine`
   MUST already be in `W::refine`'s range — the stronger rule's
   canonical form is canonical-enough for the weaker rule;
3. the weaker rule has no observable behaviour that depends on
   re-running its narrowing morphism on the upcast value.

Implementers MUST document compliance with this contract; users MUST
NOT add implication impls that violate it. The implication trait
MUST be irreflexive at the user level (no implementer declares a
self-edge such as `MyRule: Implies<MyRule>`); the library MAY derive
transitive edges from declared direct edges, but doing so is OPTIONAL.

### 5.8. Contextual Rules

The library MUST provide a contextual companion to the unconditional
rule trait so invariants that depend on a runtime environment (a
container, a configuration value, a precomputed state, an external
authority) are expressible without escaping into ad-hoc code.

Contextual rules MUST distinguish two carrier shapes:

- a **borrowed** carrier whose proof is tied to a specific borrowed
  environment's lifetime; the carrier holds a `PhantomData<&'a Env>`
  (zero-sized) and the proof remains valid only for that lifetime.
  The borrowed carrier preserves a lifetime proof, not a value-
  identity proof: if the environment is internally mutable through
  shared references, the proof's identity claim weakens accordingly,
  and rules whose validity depends on a specific environment instance
  MUST use the owned carrier;
- an **owned** carrier whose proof is tied to construction against a
  specific environment value; the carrier stores the owned
  environment so that proof identity ties to the constructed-against
  instance. The proof is a **construction-time snapshot**: if the
  stored environment is internally mutable, subsequent mutations MAY
  invalidate the proof against the *current* state even though it
  remained valid against the *original* state.

Documentation MUST warn callers that owned contextual refinement is
a construction-time snapshot; consumers needing live-environment
guarantees MUST use the borrowed carrier *and* an environment type
whose relevant state cannot mutate through shared references.

Contextual rules MUST satisfy the other requirements of this
document (typed errors, deserialization gating where applicable,
schema reflection where applicable). The unconditional zero-cost
layout requirement of Section 5.4 applies to the borrowed carrier
but not to the owned carrier, whose env-storage is intentional.

### 5.9. Reflectable Schema

Every library-supplied rule with an expressible structural vocabulary
MUST emit a runtime-introspectable schema description. Macro-generated
refinements inherit schema reflection when their composed rule provides
it; opaque custom `refine` bodies express schema absence by omitting the
schema trait. The schema MUST be sufficient to drive:

- boundary matrices and schema cross-checks that compare the structural
  description with the executable rule;
- residual-state reporting for rules whose admitted set is absent or
  only partially explainable by the structural vocabulary;
- generation of a human-readable rule description for error messages,
  documentation, and debugging output;
- equality and ordering on schemas, so two refined types whose schemas
  are equal can be detected as such.

The schema SHOULD preserve enough structure for future property-strategy
derivation and JSON Schema export when those integrations have a real
consumer.

The schema's representation is normative only in its required surface;
the concrete enum and helper types live in the architecture document
and are revisable so long as the surface above is preserved.

User-defined rules SHOULD provide a schema. Rules that do not provide
one MUST still satisfy the construction-time narrowing contract but MAY
forgo the derived integrations.

### 5.10. Declarative Refinement Macro

The library MUST provide a declarative macro that generates, from a
single source of truth, every artifact a refined named type needs:

- the public newtype with private inner field;
- the rule type or composed rule;
- the typed error enum with the variants the rule's failure modes
  require;
- the `Deserialize` impl routed through `try_new`;
- the read-only delegating surface (`AsRef`, `Display`, `Debug`, ...);
- the schema reflection when the composed rule provides it;
- the implication edges declared in the macro input.

[Conformance note 2026-06-11: the `refinement!` error-block form
generates the first five artifacts from one declaration — the
newtype, the typed error enum (with `Display` / `core::error::Error`
impls and a single `ErrorMapper` impl as the one mapping
determinant), the `Deserialize` impl routed through the rule (so
ingress rejections carry the domain diagnostics), and the read-only
delegating surface (`AsRef`, opt-in `Display`, derive passthrough).
Schema reflection itself ships through `SchemaRule` on the composed rule
rather than through macro generation (ARCHITECTURE §15.1);
macro-declared implication edges remain planned (ARCHITECTURE §15.3), so
this section is not yet fully satisfied.]

The narrowing morphism MUST be expressed as named, ordered steps.
Type-level composition satisfies this requirement: transformer rules
(`Trim<R>`, `AsciiLowercase<R>`, ...) and validation rules
(`LenChars<MIN, MAX>`, `EachChar<P>`, ...) compose in declaration
order, and the composed rule type IS the pipeline. A macro-level step
vocabulary is NOT required; the macro accepts a composed rule type as
its single source of truth for the morphism. [Amended 2026-06-10: the
original text required a macro-input step DSL (`trim`, `min_chars`,
...); the type-level form expresses the same ordered, named, auditable
pipeline without a second language, so the DSL form is no longer
required.]

An escape hatch MUST be supported for cases the structured vocabulary
cannot express: a hand-written `Rule` implementation. This is visibly
distinct from library compositions by construction — audit and tooling
can distinguish a composed rule type (which can produce schema
metadata) from a hand-written `refine` body (which cannot) without
macro-level marking.

The library MUST NOT present a hand-written rule as equivalent to a
named composition. Silent business-policy normalisation inside an
opaque `refine` body is the failure mode the structured vocabulary
exists to prevent; users who write a custom rule are accepting
responsibility for canonicalisation's correctness.

### 5.11. Derived Property Generators

When the property-test integration feature is enabled, every refined
named type whose rule provides a schema MUST implement the integration's
`Arbitrary` trait (or equivalent strategy provider). The implementation
MUST generate only admissible values; the rejection-sampling approach
("generate any value, then filter through `refine`") is NOT RECOMMENDED
as the default and MUST NOT be the only available implementation.

Strategies MUST generate only admissible values. Direct per-rule
strategies (an `ArbitraryRule` trait implemented alongside each rule)
are the sanctioned mechanism; the carrier's `Arbitrary` implementation
MUST surface a strategy that emits an inadmissible value as a test-time
defect, not silently filter it. Once a rule carries a structural schema,
its direct generator SHOULD be cross-checked against that schema; direct
implementations remain sanctioned for rules whose admissible state space
exceeds the schema vocabulary. [Amended 2026-06-10: the original text
required derivation from the reflectable schema; per-rule direct
strategies are the shipped mechanism. Schema-derived generation remains
the destination once boundary-biased derivation proves equal-or-better
per family; a schema constructor landing does not by itself require
replacing that family with derived generation.]

For rules with no structural description (opaque custom `refine`,
contextual opacity), the library MUST NOT emit a default `Arbitrary`
impl that uses rejection sampling. Users MAY provide a hand-written
strategy; in its absence, the corresponding refined type does NOT
implement `Arbitrary`. The library MUST NOT block manual
implementations.

### 5.12. Bidirectional Codecs

When a rule is a pure predicate (it does not canonicalise), the
library's encode direction MUST be the identity on the inner value.
When a rule canonicalises, the encode direction MUST return the
canonical inner value unchanged.

The round-trip law is: for every refined value `x`,
`decode(encode(x)) = Ok(x)`. This holds because the canonical form is
stable under re-narrowing: `R::refine(x.inner) = Ok(x.inner)` for
every refined value `x`, by the soundness obligation and the
canonicalisation idempotence required by Section 5.14.

For raw inputs whose canonicalisation is non-trivial, the dual
direction `decode(raw)` and `decode(encode(decode(raw)))` MUST agree
— i.e. canonicalisation is idempotent on raw inputs that admit
canonical forms.

The bidirectional codec story is REQUIRED for the `serde` integration
(`Serialize` MUST be derivable transparently from the inner value's
`Serialize`) and OPTIONAL for ad-hoc encode/decode pairs.

### 5.13. Bounded Inputs

Inputs that flow through the narrowing morphism MUST be subject to the
bounds declared by the rule's schema. The library MUST NOT accept an
input whose size or shape exceeds the rule's declared bound; doing so
MUST result in a typed rejection, not silent truncation.

This requirement applies recursively: a rule that narrows a collection
MUST enforce both the collection's length bound and the per-element
rule's bound.

### 5.14. Testability

The library MUST be testable without contacting any external service,
network, or live database. Library-supplied rules MUST have property
tests proving:

- the narrowing morphism rejects inputs outside the admissible state
  space and accepts inputs inside it;
- the morphism is idempotent on admissible inputs (re-running narrows
  to the same canonical form);
- canonicalisation, where present, is deterministic (the same raw
  input always produces the same canonical output);
- implication edges, where declared, preserve admissibility.

The macro MUST have trybuild-style tests for successful expansion over a
representative set of refinement shapes, plus compile-failure tests for
documented misuse cases (malformed pipeline, contradictory steps,
references to undefined named steps).

Production code MUST NOT contain test-only branches, test-only
environment variables, or behaviour switches whose only purpose is to
make tests pass.

## 6. Non-Goals

Whittle is not:

- a compiler-assisted refinement-type system in the Liquid Haskell,
  F\*, or Dafny sense;
- a runtime symbolic theorem prover or constraint solver;
- a replacement for every Rust newtype (newtypes whose only purpose
  is nominal distinction without validation, like `branded`'s use
  case, are explicitly out of scope);
- a wrapper for invariants the standard library already proves with
  a smart-constructor type (`NonZeroU16` and kin): `Refined` is for
  invariants the standard library cannot express [Amended
  2026-06-11: added from dogfooding evidence — wrapping a stdlib
  smart-constructor type in `Refined<_, Within<…>>` added proof
  surface and unreachable bridges without shrinking the state
  space];
- a serialization format, an HTTP framework, a database adapter, or
  any other ecosystem layer above the narrowing boundary;
- a context-passing effect system (the contextual-rule story handles
  one narrow case; full effect tracking is not in scope);
- a derive macro for unbounded user-supplied trait derivations;
- a benchmarking harness for refined-value performance, even though
  the zero-cost layout claim implies benchmark obligations on the
  implementation.

Future extensions MAY include some of these capabilities only when the
architecture document explicitly admits them in a later milestone.

## 7. Reliability and Security Considerations

The primary reliability concerns are: silent acceptance of an
invariant-violating value through a deserialization path that bypassed
the rule; silent canonicalisation that changes a value's meaning
("trim a leading space" applied to a case-sensitive identifier); a
broken implication edge that converts a stronger refinement to a weaker
one whose narrowing has not actually been satisfied; a contextual rule
whose environment is not the environment the refined value will later
be used against.

The requirements in Section 5 are reliability requirements.
Implementations that weaken them are not conformant with this document.

Whittle sits at the security boundary of its consumers. Untrusted input
crosses the constructor surface; trusted values cross every other
surface. The constructor MUST therefore be robust against pathological
inputs that exhaust resources (very long strings, deeply nested
collections, unbounded enumeration). The library MUST surface
resource-bound failures as typed rejection, not as panic, hang, or
abort.

Deserialization MUST NOT execute arbitrary user-supplied code as part
of narrowing. The closure escape hatch admitted by Section 5.10 is a
build-time concern, not a runtime one; the closure is part of the
binary and is subject to ordinary Rust safety. Whittle MUST NOT provide
a mechanism for evaluating rules supplied at runtime from untrusted
sources.

## 8. References

### 8.1. Normative References

[RFC2119] Bradner, S., "Key words for use in RFCs to Indicate
Requirement Levels", BCP 14, RFC 2119, March 1997,
<https://www.rfc-editor.org/rfc/rfc2119.html>.

[RFC8174] Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119
Key Words", BCP 14, RFC 8174, May 2017,
<https://www.rfc-editor.org/rfc/rfc8174.html>.

### 8.2. Informative References

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

[EFFECT-SCHEMA] "Effect Schema: a TypeScript schema library with
derived codecs, property generators, and JSON Schema",
<https://effect.website/docs/schema/introduction/>.

[LIQUID] Vazou, N. et al., "Liquid Haskell: refinement types for Haskell",
<https://ucsd-progsys.github.io/liquidhaskell/>.

[NUTYPE] "nutype: A proc-macro for creating newtypes with sanitization
and validation",
<https://crates.io/crates/nutype>.
