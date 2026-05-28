# Whittle examples

A runnable corpus that demonstrates every public surface of `whittle` — the
`Rule` trait, the `Refined<T, R>` carrier, the library-supplied primitives,
the `And` / `Or` composition operators, the string transformers, the
`refinement!` macro, and the optional `serde` / `proptest` adapters. The
corpus is structured so a reader (human or LLM) can find a working example
of any whittle pattern in under a minute.

## How to run

Each file under this directory is a Cargo example owned by the root
`whittle` crate. Invoke any example by name from the repository root:

```bash
cargo run --example hello-refinement --all-features
cargo run --example flat-domain-error --all-features
cargo run --example airline-domain --all-features
```

`--all-features` enables the `hex`, `unicode`, `serde`, and `proptest`
features that several examples depend on. Examples that do not need a
particular feature still compile under it cleanly.

Every example exits 0 on success and prints one or more `println!` lines.
The final line is always `OK: <one-line summary>`, so a reader can spot-check
that the demonstration actually ran by scanning the last line of output.

The release-time check that every example actually runs (not just
compiles) lives in `examples/verify.sh`. `cargo test --examples` only
compiles example harnesses; the script runs each one with `cargo run
--example` and greps for the `OK:` summary line:

```bash
bash examples/verify.sh
```

## Conventions

- Each example is self-contained; no shared helper modules.
- Each example has a `//!` doc comment explaining what it shows and when
  the pattern is the right tool.
- `assert_eq!` calls inside `main` are the demonstration; `println!` only
  echoes a summary so the run produces observable output.
- `unwrap()` is used freely on demonstrations that are *meant* to succeed
  — it keeps the focus on the API rather than on error plumbing. Real
  domain types should use the flat-error pattern shown in
  `flat-domain-error.rs`.

## Index

### Basics

- **`hello-refinement.rs`** — define a tiny custom `Rule<i32>` and
  construct a `Refined<i32, _>` through `try_new`.
- **`smart-constructor-newtype.rs`** — wrap `Refined` in a named domain
  type via the `refinement!` macro.

### Primitives by domain

- **`numeric-bounds.rs`** — `Within`, `AtLeast`, `AtMost`, `NonZero`,
  `Positive`, `Negative`. `Within` exposes a flat `NumericError`.
- **`string-validation.rs`** — `LenChars`, `LenBytes`, `NonEmpty`,
  `EachChar`, `FirstChar`. Char count vs. byte length on multi-byte
  UTF-8.
- **`collection-validation.rs`** — `LenItems`, `AllItems`, `Distinct`,
  `UniqueByKey`, `Sorted`, `NoneOf`, `AnyOf`. Custom `KeyOf` and
  `Predicate` impls.
- **`float-rules.rs`** — `NotNan`, `NotInfinite`, `Finite`,
  `InClosedRange`. `Finite` is the nominal newtype with a flat
  `FloatError`.
- **`relative-path.rs`** — `RelativePath` admit/reject for every
  `PathError` variant.

### Composition

- **`composition-and.rs`** — `And<R1, R2>` and `AndError`. The
  anti-pattern of leaking `AndError` and the flat-enum fix.
- **`composition-or.rs`** — `Or<R1, R2>` and `OrError`. Same lesson
  applied to the alternation operator.
- **`flat-domain-error.rs`** — the headline pattern: a nominal
  newtype wraps a composed rule and presents a flat domain enum.

### Transformers

- **`hex-and-normalization.rs`** — strict vs. permissive vs.
  normalized hex hashes. The canonical "what is a transformer"
  example.
- **`transformers.rs`** — `AsciiLowercase`, `AsciiUppercase`, `Trim`,
  and their composition. The stored carrier is canonical, not the
  input.

### Serde

- **`serde-roundtrip.rs`** — `Serialize` / `Deserialize` on a user
  struct with `Refined<...>` fields; `#[serde(deny_unknown_fields)]`
  on the outer struct as the recommended pattern.

### Property-based testing

- **`proptest-arbitrary.rs`** — `Refined<T, R>: Arbitrary`; routing a
  narrower strategy through `try_new` when the admissible region is
  sparse under the default sampler.

### Real-world domains

- **`airline-domain.rs`** — `IataAirportCode`, `BookingReference`,
  `FlightCode`, and a parent `Itinerary` struct that composes them.
- **`cargo-package-name.rs`** — `CargoPackageName` as a flat-error
  newtype over `And<FirstChar<...>, EachChar<...>>`.
