# whittle development recipes.
#
# Each recipe runs one gate from the verify set used in `ci`. The
# recipe set tracks `docs/ARCHITECTURE.md` §5 ("Toolchain and Gates")
# — extend here when the spec gains a new gate.

# Default: list recipes.
default:
    @just --list

# Run every gate in `ci` order: fmt-check, lint, test,
# test-default-build, doc-build, docs.
ci: fmt-check lint test test-default-build doc-build docs

# Markdown lint over the committed Markdown set. `git ls-files` is
# the single determinant: tracked files are linted, local-only
# working files (e.g. the gitignored docs/DOGFOODING.md scratchpad)
# are not, and newly tracked Markdown joins the gate automatically.
docs:
    mado check $(git ls-files '*.md')

# Rustfmt drift check; fails if anything is unformatted.
fmt-check:
    cargo fmt --all --check

# Clippy across the full workspace, all features, all targets.
lint:
    cargo clippy --workspace --all-features --all-targets

# Unit, doc, and integration tests across the workspace.
test:
    cargo test --workspace --all-features

# Default-features test compile. Every other gate runs
# --all-features, so a test that uses a feature-gated item without
# a cfg gate breaks only here; `scripts/hooks/pre-push` enforces
# the same threshold before a branch leaves the machine.
test-default-build:
    cargo test -p whittle-core --no-run

# Build the rustdoc tree without dependencies. Part of `ci`: the
# workspace denies `rustdoc::all`, so a broken intra-doc link fails
# this gate instead of surviving until someone runs `cargo doc` by
# hand.
doc-build:
    cargo doc --workspace --all-features --no-deps
