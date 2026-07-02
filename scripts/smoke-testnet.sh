#!/usr/bin/env bash
# fund->release and fund->dispute->resolve with native XLM on testnet.
set -euo pipefail
cd "$(dirname "$0")/.."
ID=$(python3 -c 'import json;print(json.load(open("deployments/testnet.json"))["contract_id"])')
NATIVE=$(stellar contract id asset --asset native --network testnet)
a() { stellar keys address "se-testnet-$1"; }
inv() { local src=$1; shift; stellar contract invoke --id "$ID" --source "se-testnet-$src" --network testnet -- "$@" 2>&1 | grep -v '^ℹ️\|^🌎\|^📅'; }
bal() { stellar contract invoke --id "$NATIVE" --source se-testnet-admin --network testnet -- balance --id "$1" 2>/dev/null; }
DL=$(( $(date +%s) + 2*86400 ))
ISSUE=$(date +%s)
echo "== A: fund -> release"
inv maintainer fund_bounty --maintainer "$(a maintainer)" --repo_id 1 --issue "$ISSUE" --token "$NATIVE" --amount 100000000 --deadline "$DL" --relayer_allowed true
inv relayer release --caller "$(a relayer)" --bounty_id 1 --winner "$(a contributor)"
echo "winner bal: $(bal "$(a contributor)")  treasury: $(bal "$(a treasury)")"
echo "== B: fund -> dispute -> resolve 70/30"
inv maintainer fund_bounty --maintainer "$(a maintainer)" --repo_id 1 --issue "$((ISSUE+1))" --token "$NATIVE" --amount 100000000 --deadline "$DL" --relayer_allowed false
inv arbiter open_dispute --caller "$(a arbiter)" --bounty_id 2
inv arbiter resolve --arbiter "$(a arbiter)" --bounty_id 2 --winner "\"$(a contributor)\"" --winner_bps 7000
echo "contract held: $(bal "$ID")"
inv admin get_totals --token "$NATIVE"
