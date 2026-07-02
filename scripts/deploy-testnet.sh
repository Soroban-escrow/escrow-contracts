#!/usr/bin/env bash
# Deploys to testnet and writes deployments/testnet.json.
# Needs stellar identities (env overridable): ADMIN, ARBITER, TREASURY, RELAYER.
# Also needs TESTNET_USDC (SAC contract id, verify from Circle/Stellar docs first).
set -euo pipefail
cd "$(dirname "$0")/.."
ADMIN=${ADMIN:-se-testnet-admin}; ARBITER=${ARBITER:-se-testnet-arbiter}
TREASURY=${TREASURY:-se-testnet-treasury}; RELAYER=${RELAYER:-se-testnet-relayer}
TESTNET_USDC=${TESTNET_USDC:-}  # optional: verify the USDC SAC id from Circle/Stellar docs first
./scripts/build.sh
W=target/wasm32v1-none/release/bounty_escrow.wasm
ID=$(stellar contract deploy --wasm "$W" --source "$ADMIN" --network testnet)
addr() { stellar keys address "$1"; }
stellar contract invoke --id "$ID" --source "$ADMIN" --network testnet -- init \
  --admin "$(addr "$ADMIN")" --relayer "$(addr "$RELAYER")" --arbiter "$(addr "$ARBITER")" \
  --treasury "$(addr "$TREASURY")" --fee_bps 300 --grace_secs 259200 \
  --min_duration_secs 86400 --max_duration_secs 7776000
NATIVE=$(stellar contract id asset --asset native --network testnet)
for T in ${TESTNET_USDC:+"$TESTNET_USDC"} "$NATIVE"; do
  stellar contract invoke --id "$ID" --source "$ADMIN" --network testnet -- set_token \
    --admin "$(addr "$ADMIN")" --token "$T" --min 10000000 --max 5000000000
done
mkdir -p deployments
printf '{"network":"testnet","contract_id":"%s"}\n' "$ID" > deployments/testnet.json
echo "contract: $ID"
