#!/usr/bin/env bash
# Run every example under examples/ and confirm each one prints an
# `OK:` summary line on its final line of stdout. `cargo test
# --examples` only compiles example harnesses; it does not run them,
# so this script is the actual release-time check that the demos
# still work end-to-end.

set -euo pipefail

cd "$(dirname "$0")/.."

shopt -s nullglob
for ex in examples/*.rs; do
    name="$(basename "$ex" .rs)"
    echo "running $name..."
    output=$(cargo run --example "$name" --all-features --quiet 2>&1)
    echo "$output"
    if ! grep -q '^OK:' <<<"$output"; then
        echo "FAIL: $name did not print an OK: line" >&2
        exit 1
    fi
done

echo "all examples passed"
