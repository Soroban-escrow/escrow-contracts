#!/usr/bin/env bash
# Regenerates TypeScript client bindings from the current Wasm contract ABI.
# #114: includes single-argument InitParams and FundBountyParams entry points.
# Usage: OUTPUT_DIR=path/to/output ./scripts/bindings.sh
# The default output directory points at the downstream escrow-app client.
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build.sh
OUTPUT_DIR="${OUTPUT_DIR:-../skillescrow-app/packages/contract-client}"
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/bounty_escrow.wasm \
  --output-dir "$OUTPUT_DIR" --overwrite
