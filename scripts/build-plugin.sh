#!/usr/bin/env bash
# Builds a decoder plugin crate into a `benchpeek:decoder` WASM component.
#
# Usage: scripts/build-plugin.sh <crate-name> [output-name]
# Example: scripts/build-plugin.sh ascii-kv-wasm ascii-kv
set -euo pipefail

CRATE="${1:?usage: build-plugin.sh <crate-name> [output-name]}"
OUT_NAME="${2:-$CRATE}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT/target/plugins"
mkdir -p "$OUT_DIR"

cargo build -p "$CRATE" --target wasm32-unknown-unknown --release --manifest-path "$ROOT/Cargo.toml"

MODULE="$ROOT/target/wasm32-unknown-unknown/release/$(echo "$CRATE" | tr '-' '_').wasm"
OUT="$OUT_DIR/$OUT_NAME.wasm"

wasm-tools component new "$MODULE" -o "$OUT"
echo "built: $OUT"
