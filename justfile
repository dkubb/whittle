# whittle development recipes.
#
# Each recipe runs one gate from the verify set used in `ci`. The
# recipe set tracks `docs/ARCHITECTURE.md` §5 ("Toolchain and Gates")
# — extend here when the spec gains a new gate.

# Default: list recipes.
default:
    @just --list

# Run every gate in `ci` order: fmt-check, lint, test, docs.
ci: fmt-check lint test docs

# Markdown lint over the committed Markdown set.
docs:
    mado check README.md SKILL.md docs/*.md

# Rustfmt drift check; fails if anything is unformatted.
fmt-check:
    cargo fmt --all --check

# Clippy across the full workspace, all features, all targets.
lint:
    cargo clippy --workspace --all-features --all-targets

# Unit, doc, and integration tests across the workspace.
test:
    cargo test --workspace --all-features

# Build the rustdoc tree without dependencies.
doc-build:
    cargo doc --workspace --all-features --no-deps
