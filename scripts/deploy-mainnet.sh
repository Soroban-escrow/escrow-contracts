#!/usr/bin/env bash
# Mainnet deploy. Preflight checks run first; any failure stops before a transaction is sent.
#
#   scripts/deploy-mainnet.sh --dry-run               # checks and plan; no transactions, no files
#   scripts/deploy-mainnet.sh --i-have-human-approval # checks, then deploy after typed confirmation
#
# Environment (stellar CLI key names unless noted):
#   ADMIN, ARBITER, TREASURY, RELAYER   the four roles; all four must be different accounts
#   MAINNET_USDC                        SAC contract id expected for USDC
#   EXPECTED_WASM_SHA256                SHA-256 of the Wasm, from scripts/reproducible-build.sh
# Optional: MAX_AMOUNT (stroops, default 500 USDC), ALLOW_ABOVE_CAP=1, HORIZON_URL, WASM.
set -euo pipefail

dry_run=0
approved=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    --i-have-human-approval) approved=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done
if [ "$dry_run" -eq 0 ] && [ "$approved" -eq 0 ]; then
  echo "refusing: pass --i-have-human-approval (see BUILD_SPEC 19.4), or --dry-run to check only" >&2
  exit 1
fi

cd "$(dirname "$0")/.."
: "${ADMIN:?}" "${ARBITER:?}" "${TREASURY:?}" "${RELAYER:?}" "${MAINNET_USDC:?}" "${EXPECTED_WASM_SHA256:?}"

HORIZON_URL="${HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
# Circle's USDC issuer on Stellar. Confirm against Circle's documentation before changing it.
USDC_ISSUER="GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
MAX_AMOUNT="${MAX_AMOUNT:-5000000000}"   # 500 USDC, 7 decimals
WASM="${WASM:-target/wasm32v1-none/release/bounty_escrow.wasm}"
FEE_BPS=300

addr() { stellar keys address "$1"; }

# Build before checking the hash. A dry run checks whatever file WASM points to.
if [ "$dry_run" -eq 0 ]; then
  ./scripts/build.sh
fi

admin_is_multisig() {
  curl -sf "$HORIZON_URL/accounts/$(addr "$ADMIN")" \
    | jq -e '(.signers | length) > 1 and .thresholds.med_threshold > 1' >/dev/null
}

roles_are_distinct() {
  [ "$(printf '%s\n' "$(addr "$ADMIN")" "$(addr "$ARBITER")" "$(addr "$TREASURY")" "$(addr "$RELAYER")" \
        | sort -u | wc -l | tr -d ' ')" -eq 4 ]
}

usdc_id_matches() {
  [ "$(stellar contract id asset --asset "USDC:$USDC_ISSUER" --network-passphrase "$NETWORK_PASSPHRASE")" = "$MAINNET_USDC" ]
}

cap_respected() {
  [ "$MAX_AMOUNT" -le 5000000000 ] || [ "${ALLOW_ABOVE_CAP:-0}" = 1 ]
}

wasm_hash_matches() {
  [ "$(shasum -a 256 "$WASM" | cut -d' ' -f1)" = "$EXPECTED_WASM_SHA256" ]
}

failures=0
check() {
  local name="$1"; shift
  if "$@"; then
    echo "PASS  $name"
  else
    echo "FAIL  $name"
    failures=$((failures + 1))
  fi
}

echo "preflight:"
check "admin is a multisig account (more than one signer, threshold above 1)" admin_is_multisig
check "admin, arbiter, treasury and relayer are four different accounts" roles_are_distinct
check "MAINNET_USDC matches the id derived from Circle's issuer" usdc_id_matches
check "MAX_AMOUNT is within the 500 USDC cap (or ALLOW_ABOVE_CAP=1)" cap_respected
check "WASM hash matches EXPECTED_WASM_SHA256" wasm_hash_matches
if [ "$failures" -ne 0 ]; then
  echo "error: $failures preflight check(s) failed; nothing was submitted" >&2
  exit 2
fi
echo "all preflight checks passed"

if [ "$dry_run" -eq 1 ]; then
  echo "dry run: would deploy $WASM, init (fee ${FEE_BPS} bps, arbiter $ARBITER, treasury $TREASURY, relayer $RELAYER), set USDC max $MAX_AMOUNT, read back get_config, and write deployments/mainnet.json"
  exit 0
fi

read -r -p "Type DEPLOY TO MAINNET to continue: " A
[[ "$A" == "DEPLOY TO MAINNET" ]] || { echo "aborted" >&2; exit 1; }

ID=$(stellar contract deploy --wasm "$WASM" --source "$ADMIN" --network mainnet)
# Pass the grouped InitParams struct (JSON numbers for u32/u64 fields).
INIT_PARAMS=$(jq -nc \
  --arg admin "$(addr "$ADMIN")" --arg relayer "$(addr "$RELAYER")" \
  --arg arbiter "$(addr "$ARBITER")" --arg treasury "$(addr "$TREASURY")" \
  --argjson fee_bps "$FEE_BPS" \
  '{admin: $admin, relayer: $relayer, arbiter: $arbiter, treasury: $treasury, fee_bps: $fee_bps, grace_secs: 259200, min_duration_secs: 86400, max_duration_secs: 7776000}')
stellar contract invoke --id "$ID" --source "$ADMIN" --network mainnet -- init --params "$INIT_PARAMS"
# max_amount starts at 500 USDC (7 decimals); raising it needs the human.
stellar contract invoke --id "$ID" --source "$ADMIN" --network mainnet -- set_token \
  --admin "$(addr "$ADMIN")" --token "$MAINNET_USDC" --min 10000000 --max "$MAX_AMOUNT"

# Read back what was configured. A mismatch means the contract must not be used.
got="$(stellar contract invoke --id "$ID" --source "$ADMIN" --network mainnet -- get_config)"
if ! jq -e --arg a "$(addr "$ADMIN")" --arg r "$(addr "$RELAYER")" --arg ar "$(addr "$ARBITER")" \
      --arg t "$(addr "$TREASURY")" --argjson f "$FEE_BPS" \
      '.admin == $a and .relayer == $r and .arbiter == $ar and .treasury == $t and .fee_bps == $f' \
      <<<"$got" >/dev/null; then
  echo "error: read-back does not match. Contract $ID is configured and must not be used." >&2
  echo "$got" >&2
  exit 1
fi

mkdir -p deployments
jq -n --arg id "$ID" --arg hash "$EXPECTED_WASM_SHA256" --arg commit "$(git rev-parse HEAD)" \
  '{network: "mainnet", contract_id: $id, wasm_hash: $hash, commit: $commit}' > deployments/mainnet.json
echo "deployed $ID; verified config; wrote deployments/mainnet.json"
