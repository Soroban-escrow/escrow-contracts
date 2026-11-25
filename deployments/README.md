# deployments/

Each file in this directory records one live deployment of `bounty_escrow`. The filename matches the
Stellar network name (e.g. `testnet.json`, `local.json`).

`local.json` is ignored by git (see `.gitignore`). All other files are committed so that
`smoke-testnet.sh` and `verify-deployment.sh` can locate the contract without extra configuration.

---

## Schema

```jsonc
{
  "network":     "testnet",                                  // Stellar network name passed to --network
  "contract_id": "CCEKBPYC…ZF4V",                           // Bech32m contract address returned by stellar contract deploy
  "wasm_hash":   "abc123…" | null,                           // SHA-256 hex of the uploaded Wasm; set by scripts/verify-deployment.sh, null until verified
  "commit":      "a1b2c3…" | null                            // Git commit SHA the Wasm was built from; null when not recorded
}
```

### Field reference

| Field | Type | Producer | Consumers | Notes |
|---|---|---|---|---|
| `network` | string | `scripts/deploy-testnet.sh` | `scripts/smoke-testnet.sh` | Matches the `--network` flag accepted by the Stellar CLI. |
| `contract_id` | string (Bech32m) | `scripts/deploy-testnet.sh` | `scripts/smoke-testnet.sh`, `scripts/verify-deployment.sh`, `scripts/bindings.sh` | The deployed contract address. `smoke-testnet.sh` reads it with `python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["contract_id"])'`. |
| `wasm_hash` | string (hex SHA-256) or `null` | `scripts/verify-deployment.sh` | `scripts/verify-deployment.sh` | Populated after a successful verification run. `null` means not yet verified. |
| `commit` | string (hex SHA) or `null` | not yet automated | — | Reserved for the git commit the Wasm was built from. `null` until a build script writes it. |

---

## Producer: `scripts/deploy-testnet.sh`

Writes the file on a successful deploy:

```bash
printf '{"network":"%s","contract_id":"%s"}\n' "$NETWORK" "$ID" > "$RECORD"
```

The script refuses to overwrite an existing record unless `OVERWRITE_DEPLOYMENT=1` is set, to prevent
accidentally orphaning a live contract.

`DEPLOYMENT_FILE` overrides the default path (`deployments/<network>.json`).

## Consumer: `scripts/smoke-testnet.sh`

Reads `contract_id` to know which contract to invoke:

```bash
ID=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["contract_id"])' "$RECORD")
```

Respects the same `NETWORK` and `DEPLOYMENT_FILE` environment variables as the deploy script.

## Consumer: `scripts/verify-deployment.sh`

Fetches the on-chain Wasm hash and compares it with the value stored under `wasm_hash`. If the
hashes match it also updates `wasm_hash` in the file.

---

## Adding a new network

1. Run `NETWORK=<name> ./scripts/deploy-testnet.sh` (or the equivalent mainnet script).
2. The script creates `deployments/<name>.json` automatically.
3. Commit the file if the network is shared (testnet, mainnet). Leave `local.json` untracked.
