#!/usr/bin/env bash
# Generates TS bindings into ../skillescrow-app/packages/contract-client.
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build.sh
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/bounty_escrow.wasm \
  --output-dir ../skillescrow-app/packages/contract-client --overwrite
