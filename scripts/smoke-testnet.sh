#!/usr/bin/env bash
# fund->release and fund->dispute->resolve with native XLM on a Stellar network.
# NETWORK defaults to testnet; use NETWORK=local against a local Quickstart network.
# KEY_PREFIX defaults to se-<network>- and names the required identities.
# Asserts the balance deltas each step must cause, and the get_totals invariant at the end.
# Requires Stellar CLI identities: <prefix>admin, <prefix>arbiter, <prefix>treasury,
# <prefix>relayer, <prefix>maintainer, <prefix>contributor.
# Create them with: stellar keys generate <name> --network <network> --fund
# Exits non-zero on the first mismatch.
set -euo pipefail
cd "$(dirname "$0")/.."
NETWORK=${NETWORK:-testnet}
KEY_PREFIX=${KEY_PREFIX:-se-${NETWORK}-}
RECORD=${DEPLOYMENT_FILE:-deployments/${NETWORK}.json}
ID=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["contract_id"])' "$RECORD")
NATIVE=$(stellar contract id asset --asset native --network "$NETWORK")
a() { stellar keys address "${KEY_PREFIX}$1"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
# Preflight check: verify required identities exist
check_identity() {
  if ! a "$1" > /dev/null 2>&1; then
    fail "identity '${KEY_PREFIX}$1' not found. Create it with: stellar keys generate ${KEY_PREFIX}$1 --network $NETWORK --fund"
  fi
}
# Preflight: check all required identities exist
for identity in admin arbiter treasury relayer maintainer contributor; do
  check_identity "$identity"
done
# A failed call aborts the script. The grep only strips the CLI's info lines.
inv() {
  local src=$1; shift
  local out
  if ! out=$(stellar contract invoke --id "$ID" --source "${KEY_PREFIX}$src" --network "$NETWORK" -- "$@" 2>&1); then
    printf '%s\n' "$out" >&2
    fail "contract call '$1' from $src failed"
  fi
  printf '%s\n' "$out" | grep -v '^ℹ️\|^🌎\|^📅' || true
}
bal() { local out; out=$(stellar contract invoke --id "$NATIVE" --source "${KEY_PREFIX}admin" --network "$NETWORK" -- balance --id "$1" 2>/dev/null) || fail "balance read failed for $1"; tr -d '"' <<<"$out"; }
is_int() { [[ "$1" =~ ^-?[0-9]+$ ]]; }
# assert_eq <label> <expected> <actual>
assert_eq() {
  local label=$1 expected=$2 actual=$3
  is_int "$actual" || fail "$label: '$actual' is not an integer"
  if (( actual != expected )); then fail "$label: expected $expected, got $actual"; fi
  echo "ok   $label = $actual"
}
# assert_delta <label> <expected_delta> <balance_before> <balance_after>
assert_delta() {
  local label=$1 expected=$2 before=$3 after=$4
  if ! is_int "$before" || ! is_int "$after"; then
    fail "$label: non-integer balance (before '$before', after '$after')"
  fi
  local delta=$(( after - before ))
  if (( delta != expected )); then
    fail "$label: expected delta $expected, got $delta (before $before, after $after)"
  fi
  echo "ok   $label: delta $delta"
}
bounty_fee() { inv admin get_bounty --id "$1" | python3 -c 'import json,sys; print(int(json.load(sys.stdin)["fee_bps"]))'; }
# Extract bounty_id from fund_bounty response (returned as a string)
extract_bounty_id() { python3 -c 'import json,sys; print(int(json.load(sys.stdin)))'; }

# #114: pass FundBountyParams as JSON. The i128 amount must be a JSON string,
# whereas u64 and bool fields are represented as JSON numbers and booleans.
fund_params() {
  local issue=$1 relayer_allowed=$2
  jq -nc --arg maintainer "$MAINT" --arg token "$NATIVE" \
    --argjson repo_id 1 --argjson issue "$issue" \
    --arg amount "$AMOUNT" --argjson deadline "$DL" \
    --argjson relayer_allowed "$relayer_allowed" \
    '{maintainer: $maintainer, repo_id: $repo_id, issue: $issue, token: $token, amount: $amount, deadline: $deadline, relayer_allowed: $relayer_allowed}'
}

AMOUNT=100000000
FEE_BPS=300        # fee_bps given to init in deploy-testnet.sh; each bounty snapshots it at fund time
WINNER_BPS=7000    # arbiter split for run B
BPS=10000
# Expected deltas, same integer math as the contract's fees::fee / net / portion (floor division).
FEE_A=$(( AMOUNT * FEE_BPS / BPS ))          # run A: fee to treasury
NET_A=$(( AMOUNT - FEE_A ))                  # run A: net to contributor
PORTION_B=$(( AMOUNT * WINNER_BPS / BPS ))   # run B: winner's share before fee
FEE_B=$(( PORTION_B * FEE_BPS / BPS ))       # run B: fee on the winner's share, to treasury
NET_B=$(( PORTION_B - FEE_B ))               # run B: net to contributor
MAINT_B=$(( AMOUNT - PORTION_B ))            # run B: dust-free remainder back to maintainer

DL=$(( $(date +%s) + 2*86400 ))
ISSUE=$(date +%s)
CONTRIB=$(a contributor); TREAS=$(a treasury); MAINT=$(a maintainer)

echo "== A: fund -> release"
K0=$(bal "$ID")
BOUNTY_A=$(inv maintainer fund_bounty --params "$(fund_params "$ISSUE" true)" | extract_bounty_id)
K1=$(bal "$ID")
assert_delta "A fund: contract received amount" "$AMOUNT" "$K0" "$K1"
assert_eq "A bounty $BOUNTY_A fee snapshot" "$FEE_BPS" "$(bounty_fee "$BOUNTY_A")"
CA0=$(bal "$CONTRIB"); TA0=$(bal "$TREAS"); KA0=$(bal "$ID")
inv relayer release --caller "$(a relayer)" --bounty_id "$BOUNTY_A" --winner "$(a contributor)"
CA1=$(bal "$CONTRIB"); TA1=$(bal "$TREAS"); KA1=$(bal "$ID")
assert_delta "A release: contributor net" "$NET_A" "$CA0" "$CA1"
assert_delta "A release: treasury fee" "$FEE_A" "$TA0" "$TA1"
assert_delta "A release: contract paid out amount" "-$AMOUNT" "$KA0" "$KA1"

echo "== B: fund -> dispute -> resolve ${WINNER_BPS}/10000 to winner"
KB0=$(bal "$ID")
BOUNTY_B=$(inv maintainer fund_bounty --params "$(fund_params "$((ISSUE+1))" false)" | extract_bounty_id)
KB1=$(bal "$ID")
assert_delta "B fund: contract received amount" "$AMOUNT" "$KB0" "$KB1"
assert_eq "B bounty $BOUNTY_B fee snapshot" "$FEE_BPS" "$(bounty_fee "$BOUNTY_B")"
inv arbiter open_dispute --caller "$(a arbiter)" --bounty_id "$BOUNTY_B"
CB0=$(bal "$CONTRIB"); TB0=$(bal "$TREAS"); MB0=$(bal "$MAINT"); KB2=$(bal "$ID")
inv arbiter resolve --arbiter "$(a arbiter)" --bounty_id "$BOUNTY_B" --winner "\"$(a contributor)\"" --winner_bps "$WINNER_BPS"
CB1=$(bal "$CONTRIB"); TB1=$(bal "$TREAS"); MB1=$(bal "$MAINT"); KB3=$(bal "$ID")
assert_delta "B resolve: contributor net" "$NET_B" "$CB0" "$CB1"
assert_delta "B resolve: treasury fee" "$FEE_B" "$TB0" "$TB1"
assert_delta "B resolve: maintainer remainder" "$MAINT_B" "$MB0" "$MB1"
assert_delta "B resolve: contract paid out amount" "-$AMOUNT" "$KB2" "$KB3"

echo "== invariant: get_totals(native)"
TOTALS=$(inv admin get_totals --token "$NATIVE")
TOT=$(printf '%s\n' "$TOTALS" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(*(int(d[k]) for k in ("funded","released_net","fees","refunded","held")))')
read -r FUNDED RELEASED FEES REFUNDED HELD <<<"$TOT"
SUM=$(( RELEASED + FEES + REFUNDED + HELD ))
if (( FUNDED != SUM )); then
  fail "get_totals funded != released_net + fees + refunded + held: expected funded $SUM, got $FUNDED"
fi
echo "ok   funded = released_net + fees + refunded + held = $FUNDED"
# held + claimable == contract balance. claimable is not exposed by get_totals; no payout is deferred in this run.
assert_eq "held equals contract balance (no deferred claims)" "$HELD" "$(bal "$ID")"
echo "smoke-testnet: all assertions passed"
