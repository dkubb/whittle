# whittle

Parse-don't-validate types in Rust. Narrow values at the boundary,
trust them downstream.

`Refined<T, R>` is a `T` whose construction goes through a `Rule<T>`
marker. If `Refined::try_new` returns `Ok`, the carrier's existence
is the proof that the rule held — every function that takes
`Refined<T, R>` already knows the invariant and never re-checks.

## Why

Primitive types remember nothing. The `String` came from a parsed
HTTP header. The `i32` was supposed to be a percentage. Every callee
either re-validates "just in case" or trusts and breaks at runtime.

```rust
// Conventional Rust: every layer either re-validates, or trusts and
// hopes. The signature does not tell you which.
fn apply_discount(percent: u8, price: u64) -> u64 {
    assert!(percent <= 100, "invalid percent");
    price - (price * u64::from(percent) / 100)
}
```

With whittle the type rules out the bad inputs at the boundary, so
the body of the function is total:

```rust
use whittle::Refined;
use whittle::primitive::Within;

type Percent = Refined<u8, Within<0, 100>>;

// No assert. The type witnesses `0 <= percent <= 100`.
fn apply_discount(percent: Percent, price: u64) -> u64 {
    let p = u64::from(*percent.as_inner());
    price - (price * p / 100)
}

let p: Percent = Refined::try_new(15_u8).unwrap();
assert_eq!(apply_discount(p, 100), 85);
```

The only place a `Percent` can be constructed is through `try_new`,
which runs the rule. Past that gate, the value is trusted.

## Install

```toml
[dependencies]
whittle = "0.0"
```

`whittle` is `#![no_std]` with `extern crate alloc`. Default features
are empty; opt in to `serde`, `proptest`, `hex`, `unicode`,
`decimal`, `chrono`, or `regex` as needed. The `regex` feature is the
only feature that pulls in `std`.

## A minute of code

```rust
use whittle::Refined;
use whittle::primitive::{NonEmpty, StringError};

// Admit.
let name: Refined<String, NonEmpty> =
    Refined::try_new("Alice".to_string()).unwrap();
assert_eq!(name.as_inner(), "Alice");

// Reject. The rule's typed error names the failure mode.
let err = Refined::<String, NonEmpty>::try_new(String::new()).unwrap_err();
assert_eq!(err, StringError::Empty);
```

That is the kernel: a marker rule, a carrier, `try_new`, and
`as_inner` / `into_inner`. The rest of the surface is deliberately
small: shipped rule families, composition operators, closed-set enums,
schema reflection, macros, and optional `serde` / `proptest` glue.

## The pattern that scales

Real applications wrap `Refined<T, R>` in a hand-written newtype so
the rule composition stays an implementation detail. The newtype
exposes a flat domain error that names the *outcome* a caller cares
about, not the rule machinery underneath.

```rust
use whittle::{And, Refined};
use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};

// Public: the nominal type. The inner `Refined` is private; the
// composition (3..=8 ASCII alphanumeric chars) is anonymous.
pub struct FlightCode(
    Refined<String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>>,
);

// Public: one variant per failure mode the caller can act on. No
// `And`, `Or`, or `StringError` leakage.
#[derive(Debug, PartialEq, Eq)]
pub enum FlightCodeError {
    Length { actual: usize },
    BadChar { offset: usize },
}

impl FlightCode {
    pub fn try_new(raw: String) -> Result<Self, FlightCodeError> {
        Refined::try_new(raw).map(Self).map_err(|e: StringError| match e {
            StringError::CharCountOutOfRange { actual } =>
                FlightCodeError::Length { actual },
            StringError::BadChar { offset } =>
                FlightCodeError::BadChar { offset },
            // The composition only emits the two variants above.
            // The remaining variants are matched together so the
            // compiler tells us if a new variant ever lands.
            StringError::ByteLenOutOfRange { .. }
            | StringError::Empty
            | StringError::BadFirstChar
            | StringError::BadHexLength { .. } => unreachable!(
                "composition emits only CharCountOutOfRange and BadChar"
            ),
        })
    }

    pub fn as_str(&self) -> &str { self.0.as_inner() }
}
```

The newtype is the domain. The `Refined<...>` is implementation. The
rest of the library is a catalogue of rules to plug into that slot.
A runnable version lives in
[`tests/flat-domain-error.rs`](tests/flat-domain-error.rs).

## What ships

Rules group by carrier type. Each family returns one flat error
enum, so an `And<A, B>` composition surfaces a flat enum the newtype
can map 1:1 into its domain variants.

- **Numeric** (signed and unsigned integers, `usize`, `isize`) —
  `Within<MIN, MAX>`, `AtLeast`, `AtMost`, `GreaterThan`, `LessThan`
  (closed and open bounds compose to all four PostgreSQL range
  shapes), `EqualTo<N>` and `NotEqualTo<N>` (singleton and
  exclusion), and the sign aliases `NonZero`, `Positive`, `Negative`.
- **Float** (`f32`, `f64`) — `NotNan`, `NotInfinite`, `Finite`,
  plus `InClosedRange` with four `i64` const generics
  (`MIN_NUMERATOR`, `MIN_DENOMINATOR`, `MAX_NUMERATOR`,
  `MAX_DENOMINATOR`) — numerator / denominator pairs since Rust
  2024 lacks `f64` const generics.
- **String** — `LenChars`, `LenBytes`, `NonEmpty`, `EachChar<P>`,
  `FirstChar<P>`, built-in `CharPredicate` markers, and the
  `SchemaChar` opt-in for predicates with reflectable character sets.
- **Collection** (`Vec<T>`) — `LenItems`, `AllItems<R>`,
  `UniqueByKey`, `Distinct`, `Sorted`, `NoneOf<P>`, `AnyOf<P>`.
- **Path** (`String`) — `RelativePath`.
- **Composition** — `And<A, B>`, `Or<A, B>`, plus `Not<R>` and
  `Xor<A, B>` (numeric-only for now). N-ary tuple-based
  `All<(R1, ..., RN)>` and `Any<(R1, ..., RN)>` (arities 2..=8) for
  flat composition without binary nesting.
- **Transformers** (`Rule<String>`) — `Trim<R>`, `AsciiLowercase<R>`,
  `AsciiUppercase<R>`. Normalise input before delegating, so the
  stored carrier is the canonical form.
- **Schema reflection** — `SchemaRule<T>` opt-in descriptions for
  expressible rules; `Schema` values drive boundary matrices, schema
  cross-checks, residual-state reports, human-readable descriptions,
  and schema equality / ordering.
- **Closed sets** — `ClosedSet` plus `closed_set!` for provider wire
  tokens where the enum itself is the constructive target, not a
  `Refined<String, _>` wrapper.
- **Macros** — `refinement!` for nominal newtypes,
  `deserialize_rule!` for default serde gating, `closed_set!`, and
  feature-gated `pattern!` for compile-time-validated regex rules.

Behind Cargo features:

- `hex` — `HexChar`, `HexFixedLower`, `HexFixedAny`, and
  `HexFixedNormalized` (no extra deps).
- `unicode` — `PrintableLine`, `PrintableMultiline`,
  `PrintableChar`, plus `BoundedLine` / `BoundedText`.
- `decimal` — `DecimalPositive`, `DecimalScale<S>`,
  `DecimalPrecision<P>`, `DecimalInRange<...>`.
- `chrono` — `DateAtLeast`, `DateAtMost`, `DateInRange`, plus the
  `DateTime<Utc>` analogues.
- `regex` — `Pattern<const RE: &'static str>` and `pattern!`.
  `Pattern` is a whole-string rule for positional grammars that the
  character-class primitives cannot express ergonomically.
- `serde` — `Serialize` / `Deserialize` for `Refined<T, R>`.
  Deserialisation routes through `try_new`, so bad payloads are
  rejected with the rule's own error.
- `proptest` — `Arbitrary` for `Refined<T, R>`. Every generated
  value satisfies the rule by construction; no `prop_assume!`
  filtering needed downstream. The `whittle::testing` helpers add
  property harnesses and schema-derived boundary matrices.

[`SKILL.md`](SKILL.md) has the full primitive catalogue, predicate
list, and the process for adding a new domain type.

## Reach for whittle when

- You'd reach for a domain newtype around a primitive (identifier,
  percentage, bounded length, hex hash, relative path, non-empty
  list).
- You're hand-rolling `try_new` / `from_str` validators that return
  ad-hoc errors and scattering the same predicate across modules.
- You want serde to refuse invalid input instead of accepting it and
  panicking later.
- You want `proptest::Arbitrary` strategies that emit valid domain
  values without `prop_assume!` filtering downstream.

## Skip whittle when

- The invariant is dynamic — depends on a runtime config, another
  field, or a database row. Whittle rules are pure functions on a
  single value; cross-field invariants belong in a smart constructor
  on the parent struct.
- The carrier should mutate in place after construction. Whittle
  exposes only `into_inner` → mutate → `try_new`; there is no
  `as_mut`.
- You want a `&str` carrier. `Rule<T>` requires `T: 'static`; every
  string primitive is `Rule<String>`.

## Learn more

- [`tests/`](tests/) — every public pattern as a runnable
  integration test, indexed in [`tests/README.md`](tests/README.md).
- [`docs/IDEA.md`](docs/IDEA.md) — authoritative project
  specification.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — concrete
  architecture derived from `IDEA.md`.
- [`SKILL.md`](SKILL.md) — patterns, anti-patterns, primitive
  catalogue, and the process for adding a new domain type. Written
  for both humans and LLMs.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
