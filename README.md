# whittle

Parse-don't-validate types in Rust: narrow values at the boundary,
trust them downstream.

Whittle gives you `Refined<T, R>` — a `T` whose construction goes
through a `Rule<T>` that says what makes the value admissible. If
`Refined::try_new` returns `Ok`, the carrier's existence is the proof
that the rule held. Every downstream function that takes
`Refined<T, R>` already knows the invariant. No re-checking, no
"trust me, I validated it three layers up."

## The problem it solves

Primitive types remember nothing. The `String` came from a parsed
HTTP header. The `i32` was supposed to be a percentage. The `Vec<u8>`
was supposed to be non-empty. Every callee either re-validates "just
in case" or trusts and breaks at runtime.

```rust
// Conventional Rust: every layer re-validates, or someone forgets.
fn apply_discount(percent: u8, price: u64) -> u64 {
    assert!(percent <= 100, "invalid percent");
    price - (price * u64::from(percent) / 100)
}
```

With whittle the same function is total — the type rules out the bad
inputs before the body runs:

```rust
use whittle::Refined;
use whittle::primitive::Within;

type Percent = Refined<u8, Within<0, 100>>;

fn apply_discount(percent: Percent, price: u64) -> u64 {
    let p = u64::from(*percent.as_inner());  // already in 0..=100
    price - (price * p / 100)
}

let p = Refined::try_new(15_u8).unwrap();
let off = apply_discount(p, 100);
```

The boundary is the *only* place a `Percent` can be constructed.
Construction goes through the rule. After that the value is trusted.

## Quick start

```toml
[dependencies]
whittle = "0.0"
```

Three minutes from zero to a first refined value:

```rust
use whittle::Refined;
use whittle::primitive::{NonEmpty, StringError};

let name: Refined<String, NonEmpty> =
    Refined::try_new("Alice".to_string()).unwrap();
assert_eq!(name.as_inner(), "Alice");

let err = Refined::<String, NonEmpty>::try_new(String::new()).unwrap_err();
assert_eq!(err, StringError::Empty);
```

That is the whole API surface: a marker rule, a carrier, `try_new`,
and `as_inner` / `into_inner`. Everything else is rules that ship
with the crate, or rules you write yourself.

## The pattern that scales: nominal newtype + flat domain error

Real applications wrap `Refined<T, R>` in a hand-written newtype so
the rule composition stays an implementation detail. The newtype
exposes a flat domain error that names the *outcome* a caller cares
about, not the rule machinery underneath.

```rust
use whittle::{And, Refined};
use whittle::primitive::{AsciiAlphanumeric, EachChar, LenChars, StringError};

// Public: the nominal type. Inner Refined is private; the composition
// (3..=8 ASCII alphanumeric chars) is anonymous.
pub struct FlightCode(
    Refined<String, And<LenChars<3, 8>, EachChar<AsciiAlphanumeric>>>,
);

// Public: a flat enum with one variant per failure mode the caller
// can act on. No "BadComposition" or "AndError" leakage.
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
            other => unreachable!("unexpected: {other:?}"),
        })
    }

    pub fn as_str(&self) -> &str { self.0.as_inner() }
}
```

That is the load-bearing whittle pattern. Once you see it the rest
of the library is a catalogue of rules to plug into the `Refined<...>`
slot. A runnable version lives in
[`tests/flat-domain-error.rs`](tests/flat-domain-error.rs).

## What ships

Library rules, grouped by carrier type. Each family returns a single
flat error enum.

- **Numeric** (every signed and unsigned integer type) —
  `Within<MIN, MAX>`, `AtLeast<MIN>`, `AtMost<MAX>`, `NonZero`,
  `Positive`, `Negative`.
- **Float** (`f32`, `f64`) — `NotNan`, `NotInfinite`, `Finite`,
  `InClosedRange<MN, MD, XN, XD>` (ratio-encoded const generics).
- **String** — `LenChars`, `LenBytes`, `NonEmpty`, `EachChar<P>`,
  `FirstChar<P>`, plus fixed-length hex variants behind `hex`.
- **Collection** (`Vec<T>`) — `LenItems`, `AllItems<R>`, `UniqueByKey`,
  `Distinct`, `Sorted`, `NoneOf<P>`, `AnyOf<P>`.
- **Path** (`String`) — `RelativePath`.
- **Decimal** (`rust_decimal::Decimal`, feature `decimal`) —
  `DecimalPositive`, `DecimalScale<S>`, `DecimalPrecision<P>`,
  `DecimalInRange<MIN_REPR, MAX_REPR, SCALE>`.
- **Date** (`chrono::NaiveDate`, feature `chrono`) — `DateAtLeast`,
  `DateAtMost`, `DateInRange`.
- **DateTime** (`chrono::DateTime<Utc>`, feature `chrono`) —
  `DateTimeAtLeast`, `DateTimeAtMost`, `DateTimeInRange`.
- **Composition** — `And<A, B>`, `Or<A, B>`.
- **Transformers** (`Rule<String>`) — `Trim<R>`, `AsciiLowercase<R>`,
  `AsciiUppercase<R>` normalise input before delegating to `R`.

Built-in `CharPredicate` impls: `AsciiAlphanumeric`, `IdentChar`,
`IdentStart`, `IdentDashChar`, `NonControl`, `HexChar` (feature
`hex`), `PrintableLine`, `PrintableMultiline`, `PrintableChar`
(feature `unicode`).

## Cargo features

All features are additive. Default is `[]`. The kernel is `#![no_std]`
with `extern crate alloc`.

- **`serde`** — `Serialize` / `Deserialize` for `Refined<T, R>`.
  Deserialisation routes through `try_new`, so bad payloads are
  rejected with the rule's own error.
- **`proptest`** — `Arbitrary` for `Refined<T, R>`. Every generated
  value satisfies the rule by construction; no `prop_assume!`
  filtering needed downstream.
- **`hex`** — hex `CharPredicate` and fixed-length hex string rules.
  No extra deps.
- **`unicode`** — Unicode-category-based predicates like
  `PrintableChar`. Pulls in `unicode-general-category`.
- **`decimal`** — the Decimal rule family. Pulls in `rust_decimal`.
- **`chrono`** — the Date and DateTime rule families. Pulls in
  `chrono` (no `clock`, `no_std`-compatible).

## When to reach for whittle

- A domain newtype around a primitive (identifier, percentage,
  bounded length, hex hash, relative path, non-empty list, ...).
- A hand-rolled `try_new` / `from_str` validator that returns ad-hoc
  errors and scatters the same predicate across modules.
- Serde payloads that should refuse invalid input instead of accepting
  it and panicking later.
- `proptest::Arbitrary` strategies that should emit valid domain
  values without `prop_assume!` filtering downstream.

## When not to use it

- The invariant is dynamic — depends on a runtime config, another
  field, or a database row. Whittle rules are pure functions on a
  single value; cross-field invariants belong in a smart constructor
  on the parent struct.
- The carrier should mutate in place after construction. Whittle
  exposes only `into_inner` → mutate → `try_new`; there is no `as_mut`.
- You want a `&str` carrier. Whittle requires `T: 'static`; every
  string primitive is `Rule<String>`.

## Learn more

- [`tests/`](tests/) — runnable integration tests double as examples,
  one file per pattern, indexed in [`tests/README.md`](tests/README.md).
- [`docs/IDEA.md`](docs/IDEA.md) — authoritative project specification.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — concrete architecture
  derived from `IDEA.md`.
- [`SKILL.md`](SKILL.md) — patterns, anti-patterns, and the process for
  adding a new domain type. Written for both humans and LLMs.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
