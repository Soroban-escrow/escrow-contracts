# Operations runbook

Step-by-step commands for the operator keys around `bounty_escrow`. All commands run from the repository root and use the [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli) (28.1.0 in CI). Set up once per session:

```bash
NETWORK=testnet                              # or local
KEY_PREFIX=se-${NETWORK}-
ADMIN=${KEY_PREFIX}admin
ID=$(jq -r .contract_id deployments/${NETWORK}.json)
NATIVE=$(stellar contract id asset --asset native --network "$NETWORK")
addr() { stellar keys address "$1"; }
ADMIN_ADDR=$(addr "$ADMIN")

# invoke <signing-identity> <function> [args...]
invoke() {
  local src=$1; shift
  stellar contract invoke --id "$ID" --source "$src" --network "$NETWORK" -- "$@"
}
```

Identities are named `<KEY_PREFIX>{admin,arbiter,treasury,relayer,maintainer,contributor}`. Create testnet/local keys with `stellar keys generate <name> --network "$NETWORK" --fund`; on mainnet use the KMS/HSM-backed signer and import only the public address into the CLI keystore.

## Who signs what

| Action | `--source` (signer) | Argument that must also match config |
|---|---|---|
| `pause`, `unpause` | admin | `--admin` = current admin address |
| `set_relayer`, `set_arbiter`, `set_treasury`, `set_fee`, `set_grace`, `set_durations`, `set_token`, `remove_token`, `set_dispute_timeout`, `set_relayer_limit` | admin | `--admin` |
| `set_admin` | current admin **and** incoming admin (two-party auth) | `--admin` = current, `--new_admin` = incoming |
| `set_upgrade_delay`, `propose_upgrade`, `execute_upgrade`, `cancel_upgrade` | admin | `--admin` |
| `refund`, `extend_deadline`, `top_up`, `fund_bounty` | maintainer | `--maintainer` |
| `release`, `release_split`, `expire_dispute` | maintainer or configured relayer (`expire_dispute`: anyone) | `--caller` |
| `open_dispute` | maintainer or arbiter | `--caller` |
| `resolve`, `resolve_with_evidence` | arbiter | `--arbiter` |
| `claim` | recorded winner | `--winner` |
| Reads (`get_*`, `active_bounty`) | nobody — add `--send=no` and omit `--source` | — |

Admin, relayer, arbiter and treasury must be four different accounts before mainnet. `require_admin` checks the `--admin` argument against stored config and then calls `require_auth`, so both the value and the signature must be right.

> **ABI migration (#114):** Newly built Wasm accepts `init --params '{...}'` and `fund_bounty --params '{...}'` JSON struct arguments. The deployment and smoke scripts handle this; older positional arguments are incompatible. See [contract ABI migration](CONTRACT.md#abi-change-grouped-input-structs-114) and regenerate the downstream TypeScript bindings before upgrading a deployment.

## Scenario 0 — Inspect state before and after any change (read-only)

No signing, nothing submitted. All three are safe during an incident.

```bash
bash scripts/inspect.sh "$NETWORK"
stellar contract invoke --id "$ID" --network "$NETWORK" --send=no -- get_config
stellar contract invoke --id "$ID" --network "$NETWORK" --send=no -- get_totals --token "$NATIVE"
```

Confirm the running Wasm matches the recorded build before assuming the code behaves as documented:

```bash
bash scripts/verify-deployment.sh "$NETWORK"
```

`verify-deployment.sh` needs `deployments/<network>.json` to contain a valid 64-hex `wasm_hash`; see the upgrade scenario below.

## Scenario 1 — Pause and unpause funding

**Signer: admin.** Use when a vulnerability is suspected in funding or top-ups.

```bash
invoke "$ADMIN" pause --admin "$ADMIN_ADDR"
```

Verify:

```bash
stellar contract invoke --id "$ID" --network "$NETWORK" --send=no -- get_config
```

Scope: `pause` blocks `fund_bounty` and `top_up` only. `release`, `release_split`, `refund`, `resolve`, `claim` and `expire_dispute` keep working, so pausing cannot trap funds. It does **not** stop a relayer release already in flight — rotate the relayer (Scenario 2) for that.

To resume:

```bash
invoke "$ADMIN" unpause --admin "$ADMIN_ADDR"
```

## Scenario 2 — Rotate the relayer

**Signer: admin.** Use on relayer key compromise or loss. The change takes effect immediately on the next transaction; the old key cannot release any bounty afterwards, including bounties that were funded with `relayer_allowed = true`.

1. Prepare the new signer and capture its address.

```bash
NEW_RELAYER=${KEY_PREFIX}relayer-2
stellar keys generate "$NEW_RELAYER" --network "$NETWORK" --fund   # testnet / local only
NEW_RELAYER_ADDR=$(addr "$NEW_RELAYER")
echo "new relayer: $NEW_RELAYER_ADDR"
```

On mainnet, import the KMS/HSM key instead and set `NEW_RELAYER_ADDR` to its public address; do not generate a funded key.

2. Update config. **Signer: admin.**

```bash
invoke "$ADMIN" set_relayer --admin "$ADMIN_ADDR" --relayer "$NEW_RELAYER_ADDR"
```

3. Verify and update the operator's records.

```bash
stellar contract invoke --id "$ID" --network "$NETWORK" --send=no -- get_config
```

Existing bounties do not need to be re-funded: `config.relayer` is read at release time. Maintainers who set `relayer_allowed = false` are unaffected and keep releasing themselves. If the old key is suspected compromised, also review recent `released` events (a release is irreversible).

The same pattern rotates the arbiter or treasury:

```bash
invoke "$ADMIN" set_arbiter --admin "$ADMIN_ADDR" --arbiter "$(addr "${KEY_PREFIX}arbiter-2")"
invoke "$ADMIN" set_treasury --admin "$ADMIN_ADDR" --treasury "$(addr "${KEY_PREFIX}treasury-2")"
```

`set_fee` only affects bounties funded afterwards — each bounty snapshots its fee at funding:

```bash
invoke "$ADMIN" set_fee --admin "$ADMIN_ADDR" --fee_bps 300
```

## Scenario 3 — Upgrade the contract

There is no plain `upgrade` entry point. Upgrades are a three-step flow: `propose_upgrade` → wait out the timelock → `execute_upgrade`. **Signer for every step: admin.**

1. Build and prove reproducibility.

```bash
./scripts/build.sh
./scripts/reproducible-build.sh
```

2. Compute the 32-byte hash and record it, with the commit, in the deployment record. `verify-deployment.sh` checks against this value.

```bash
WASM=target/wasm32v1-none/release/bounty_escrow.wasm
WASM_HASH=$(shasum -a 256 "$WASM" | cut -d' ' -f1)   # Linux: sha256sum "$WASM" | cut -d' ' -f1
echo "$WASM_HASH"

python3 - "$NETWORK" "$WASM_HASH" "$(git rev-parse HEAD)" <<'PY'
import json, sys
network, wasm_hash, commit = sys.argv[1:4]
path = f"deployments/{network}.json"
with open(path) as f:
    rec = json.load(f)
rec["wasm_hash"] = wasm_hash
rec["commit"] = commit
with open(path, "w") as f:
    json.dump(rec, f, indent=2)
    f.write("\n")
PY
```

3. Propose. The `eta` is `now + max_duration + grace` (or the configured upgrade delay) and is emitted in the `upgrade_proposed` event; capture it from the transaction output.

```bash
invoke "$ADMIN" propose_upgrade --admin "$ADMIN_ADDR" --wasm_hash "0x$WASM_HASH"
```

The CLI expects a 32-byte hex argument. If your CLI version rejects the `0x` prefix, pass `"$WASM_HASH"` instead.

4. Wait until `date +%s` is at or past the recorded `eta`. Do not skip this: the timelock is the users' exit window and `execute_upgrade` returns `UpgradeNotReady` before then. `set_upgrade_delay` can only increase the delay.

5. Execute, then verify the on-chain hash and config.

```bash
invoke "$ADMIN" execute_upgrade --admin "$ADMIN_ADDR"
bash scripts/verify-deployment.sh "$NETWORK"
bash scripts/inspect.sh "$NETWORK"
```

To abort before execution:

```bash
invoke "$ADMIN" cancel_upgrade --admin "$ADMIN_ADDR"
```

An emergency `pause` is instant and does not wait for the upgrade queue; use Scenario 1 while the timelock runs.

## Scenario 4 — Rotate the admin (two-party)

**Signers: current admin and incoming admin.** The contract requires `new_admin.require_auth()` in addition to the current admin's signature, so a typo cannot lock the contract.

```bash
NEW_ADMIN=${KEY_PREFIX}admin-2
NEW_ADMIN_ADDR=$(addr "$NEW_ADMIN")
invoke "$ADMIN" set_admin --admin "$ADMIN_ADDR" --new_admin "$NEW_ADMIN_ADDR"
```

Verify the incoming address twice before signing. Provide the incoming admin's authorization alongside the `--source` signature as required by your signer setup (for a multisig, as the additional signer). Afterwards update `ADMIN`/`ADMIN_ADDR` in the session and confirm:

```bash
stellar contract invoke --id "$ID" --network "$NETWORK" --send=no -- get_config
```

## Scenario 5 — Incident response checklist

1. **Detect** — watch `paused`, `upgrade_proposed`, `config`, `released` and `resolved` events; the scheduled `cargo audit` job opens a `security`-labelled issue on a new advisory.
2. **Assess** — run Scenario 0 and record `get_config`, `get_totals` and the affected `get_bounty` values. Compare the on-chain hash with the recorded build.
3. **Contain** — `pause` to stop new funding (Scenario 1); rotate a compromised relayer or arbiter (Scenario 2); `cancel_upgrade` if a proposal is hostile. A dispute freezes a single bounty via `open_dispute`.
4. **Eradicate** — build reproducibly, record the hash, propose and, after the timelock, execute the fixed Wasm (Scenario 3).
5. **Recover** — `verify-deployment.sh`, `inspect.sh`, then `unpause`. Reconcile `get_totals`: `funded == released_net + fees + refunded + held`, and `held + claimable == the contract's token balance`.
6. **Communicate** — publish the `eta` and the recorded commit; the timelock is the window for users to settle or exit.

## Scenario 6 — Fresh deployment and smoke test

```bash
./scripts/deploy-testnet.sh    # deploy + init + allowlist native XLM, writes deployments/testnet.json
./scripts/smoke-testnet.sh     # fund → release and fund → dispute → resolve
```

The deploy script refuses to overwrite an existing record; set `OVERWRITE_DEPLOYMENT=1` to deploy a new contract deliberately. Init and allowlist amounts are env-overridable (`FEE_BPS`, `GRACE_SECS`, `MIN_DURATION_SECS`, `MAX_DURATION_SECS`, `TOKEN_MIN`, `TOKEN_MAX`, `TESTNET_USDC`) — see [Deploying your own](../README.md#deploying-your-own).

## Related

[Contract reference](CONTRACT.md) · [Threat model](THREAT.md) · [Before mainnet](../README.md#before-mainnet)
