#!/usr/bin/env bash
# Mainnet deploy. Refuses without explicit human approval.
set -euo pipefail
if [[ "${1:-}" != "--i-have-human-approval" ]]; then
  echo "refusing: pass --i-have-human-approval (see BUILD_SPEC 19.4)" >&2; exit 1
fi
read -r -p "Type DEPLOY TO MAINNET to continue: " A
[[ "$A" == "DEPLOY TO MAINNET" ]] || { echo "aborted" >&2; exit 1; }
cd "$(dirname "$0")/.."
: "${ADMIN:?}" "${ARBITER:?}" "${TREASURY:?}" "${RELAYER:?}" "${MAINNET_USDC:?}"
./scripts/build.sh
W=target/wasm32v1-none/release/bounty_escrow.wasm
ID=$(stellar contract deploy --wasm "$W" --source "$ADMIN" --network mainnet)
addr() { stellar keys address "$1"; }
stellar contract invoke --id "$ID" --source "$ADMIN" --network mainnet -- init \
  --admin "$(addr "$ADMIN")" --relayer "$(addr "$RELAYER")" --arbiter "$(addr "$ARBITER")" \
  --treasury "$(addr "$TREASURY")" --fee_bps 300 --grace_secs 259200 \
  --min_duration_secs 86400 --max_duration_secs 7776000
# max_amount starts at 500 USDC (7 decimals); raising it needs the human.
stellar contract invoke --id "$ID" --source "$ADMIN" --network mainnet -- set_token \
  --admin "$(addr "$ADMIN")" --token "$MAINNET_USDC" --min 10000000 --max 5000000000
mkdir -p deployments
printf '{"network":"mainnet","contract_id":"%s"}\n' "$ID" > deployments/mainnet.json
