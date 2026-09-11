#!/usr/bin/env bash
# Builds the `can-dbc-py` reference decoder (Python, via componentize-py)
# into a `benchpeek:decoder` WASM component.
#
# Requires `componentize-py` on PATH: `pip install componentize-py`.
# `-s`/`--stub-wasi` is required, not optional: the host's Linker has no
# WASI imports registered (see benchpeek-wasm-host), same constraint that
# keeps ascii-kv-wasm on wasm32-unknown-unknown.
#
# Usage: scripts/build-can-dbc-py.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT/target/plugins"
mkdir -p "$OUT_DIR"

cd "$ROOT/plugins/can-dbc-py"
componentize-py -d "$ROOT/wit" -w decoder-plugin componentize app -o "$OUT_DIR/can-dbc-py.wasm" -s
echo "built: $OUT_DIR/can-dbc-py.wasm"
