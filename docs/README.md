# Whittle Documentation

This directory contains the project specification for Whittle, a Rust
parse-don't-validate library: take untrusted raw input at the boundary
of a system, narrow it to a refined value through a single declared
rule, and hand the rest of the program a value whose invariants are
guaranteed by construction.

## Contents

- [IDEA.md](IDEA.md) — authoritative project specification (RFC-style):
  goals, scope, product model, normative requirements, non-goals,
  reliability and security considerations.
- [ARCHITECTURE.md](ARCHITECTURE.md) — concrete architecture of the
  implemented library, derived from IDEA.md: workspace layout, gates,
  dependency direction, the `Rule` trait and `Refined` carrier,
  library-supplied primitive rules and composition, closed sets,
  implication and subtyping, the declarative and procedural macros,
  testing architecture, and the planned milestones with their
  evidence triggers.
- [DESIGN.md](DESIGN.md) — high-level narrative sketch that summarises
  the normative documents at a single sitting's worth of reading;
  intended as on-ramp, not as authoritative source.

[IDEA.md](IDEA.md) is authoritative for goals, scope, non-goals, and
invariants. [ARCHITECTURE.md](ARCHITECTURE.md) is authoritative for
concrete technology and structural choices only when those choices
preserve IDEA.md's requirements; if the two conflict,
[IDEA.md](IDEA.md) takes precedence. [DESIGN.md](DESIGN.md) is
historical and explanatory; if it conflicts with either of the
normative documents, the normative documents win.
