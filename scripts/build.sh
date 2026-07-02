#!/usr/bin/env bash
# Optimized wasm build; prints hash and size.
set -euo pipefail
cd "$(dirname "$0")/.."
stellar contract build
W=target/wasm32v1-none/release/bounty_escrow.wasm
echo "size: $(wc -c < "$W") bytes"
echo "sha256: $(shasum -a 256 "$W" | cut -d' ' -f1)"
