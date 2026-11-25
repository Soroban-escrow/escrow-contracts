#!/usr/bin/env bash
# Deploys to a Stellar network and writes deployments/<network>.json.
# NETWORK defaults to testnet; use NETWORK=local for a local Quickstart network.
# Needs stellar identities (env overridable): ADMIN, ARBITER, TREASURY, RELAYER.
# KEY_PREFIX defaults to se-<network>- and names those identities.
# TESTNET_USDC (optional) is the SAC contract id to allowlist in addition to native XLM.
# Init parameters are env overridable too; defaults are the values this script has always used.
# See README.md ("Deploying your own") for the list.
set -euo pipefail
cd "$(dirname "$0")/.."
NETWORK=${NETWORK:-testnet}
KEY_PREFIX=${KEY_PREFIX:-se-${NETWORK}-}
# Refuse before deploying anything: a second deploy would orphan the recorded contract.
RECORD=${DEPLOYMENT_FILE:-deployments/${NETWORK}.json}
if [ -f "$RECORD" ] && [ "${OVERWRITE_DEPLOYMENT:-}" != "1" ]; then
  echo "refusing to overwrite $RECORD: $(cat "$RECORD")" >&2
  echo "set OVERWRITE_DEPLOYMENT=1 to deploy a new contract and replace that record" >&2
  exit 1
fi
ADMIN=${ADMIN:-${KEY_PREFIX}admin}; ARBITER=${ARBITER:-${KEY_PREFIX}arbiter}
TREASURY=${TREASURY:-${KEY_PREFIX}treasury}; RELAYER=${RELAYER:-${KEY_PREFIX}relayer}
TESTNET_USDC=${TESTNET_USDC:-}  # optional: verify the USDC SAC id from Circle/Stellar docs first
FEE_BPS=${FEE_BPS:-300}
GRACE_SECS=${GRACE_SECS:-259200}
MIN_DURATION_SECS=${MIN_DURATION_SECS:-86400}
MAX_DURATION_SECS=${MAX_DURATION_SECS:-7776000}
TOKEN_MIN=${TOKEN_MIN:-10000000}
TOKEN_MAX=${TOKEN_MAX:-5000000000}
./scripts/build.sh
W=target/wasm32v1-none/release/bounty_escrow.wasm
echo "effective $NETWORK parameters:"
echo "  fee_bps=$FEE_BPS grace_secs=$GRACE_SECS min_duration_secs=$MIN_DURATION_SECS max_duration_secs=$MAX_DURATION_SECS"
echo "  token min=$TOKEN_MIN max=$TOKEN_MAX"
echo "  usdc=${TESTNET_USDC:-<unset>} admin=$ADMIN arbiter=$ARBITER treasury=$TREASURY relayer=$RELAYER"
ID=$(stellar contract deploy --wasm "$W" --source "$ADMIN" --network "$NETWORK")
addr() { stellar keys address "$1"; }
# #114: init now accepts one contracttype struct as --params (a JSON object).
INIT_PARAMS=$(jq -nc \
  --arg admin "$(addr "$ADMIN")" --arg relayer "$(addr "$RELAYER")" \
  --arg arbiter "$(addr "$ARBITER")" --arg treasury "$(addr "$TREASURY")" \
  --argjson fee_bps "$FEE_BPS" --argjson grace_secs "$GRACE_SECS" \
  --argjson min_duration_secs "$MIN_DURATION_SECS" --argjson max_duration_secs "$MAX_DURATION_SECS" \
  '{admin: $admin, relayer: $relayer, arbiter: $arbiter, treasury: $treasury, fee_bps: $fee_bps, grace_secs: $grace_secs, min_duration_secs: $min_duration_secs, max_duration_secs: $max_duration_secs}')
stellar contract invoke --id "$ID" --source "$ADMIN" --network "$NETWORK" -- init --params "$INIT_PARAMS"
NATIVE=$(stellar contract id asset --asset native --network "$NETWORK")
for T in ${TESTNET_USDC:+"$TESTNET_USDC"} "$NATIVE"; do
  stellar contract invoke --id "$ID" --source "$ADMIN" --network "$NETWORK" -- set_token \
    --admin "$(addr "$ADMIN")" --token "$T" --min "$TOKEN_MIN" --max "$TOKEN_MAX"
done
mkdir -p "$(dirname "$RECORD")"
printf '{"network":"%s","contract_id":"%s"}\n' "$NETWORK" "$ID" > "$RECORD"
echo "contract: $ID"
