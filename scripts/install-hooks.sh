#!/usr/bin/env bash
set -euo pipefail

# Install the repo's tracked git hooks into .git/hooks.
#
# .git/hooks/ is not version-controlled, so the canonical hook bodies
# live in scripts/hooks/ and this script copies them into place. Run it
# once after cloning:
#
#     bash scripts/install-hooks.sh
#
# Installed hooks:
#   pre-commit — cargo fmt --all -- --check (fast; per commit)
#   pre-push   — cargo coverage 100% gate   (slow; once per push)

cd "$(git rev-parse --show-toplevel)"

SRC_DIR="scripts/hooks"
DST_DIR=".git/hooks"

mkdir -p "$DST_DIR"

for hook_path in "$SRC_DIR"/*; do
    hook_name="$(basename "$hook_path")"
    dst="$DST_DIR/$hook_name"
    cp "$hook_path" "$dst"
    chmod +x "$dst"
    echo "installed: $dst" >&2
done

echo "install-hooks: done." >&2
