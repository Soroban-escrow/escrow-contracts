# Contract Events Reference (`docs/EVENTS.md`)

This document is the authoritative specification for all events emitted by the `bounty_escrow` Soroban contract. It is intended for indexers, analytics services, subgraphs, and client applications that monitor on-chain escrow state transitions.

---

## Overview

All contract events are emitted using Soroban's `#[contractevent]` macro. Events are written to the ledger transaction metadata and can be filtered via Stellar RPC `getEvents` queries.

### Event Structure

A Soroban event record contains:

| Component | Description |
|---|---|
| `contract_id` | Bech32m address of the emitting contract (`CCEKBPYC…`) |
| `type` | Event category (`contract`) |
| `topics` | Vector of `Val` elements (up to 4 topics in Soroban host) used for filtering |
| `data` | Structured payload (single value, tuple, or struct) |
| `ledger` | Ledger sequence number in which the transaction closed |
| `ledgerClosedAt`| ISO 8601 ledger close timestamp |
| `txHash` | 32-byte hex hash of the transaction |

### Topic Layout Rules

Every event in `bounty_escrow` follows a consistent topic prefix layout:

1. **Namespace Topic (`topics[0]`):** Always the `Symbol("bounty")`.
2. **Action Topic (`topics[1]`):** A `Symbol` naming the specific event (e.g. `Symbol("funded")`, `Symbol("released")`).
3. **Entity Topic (`topics[2]`):**
   - For **bounty lifecycle events**, `topics[2]` is the unique `u64` bounty identifier (`id`). This enables indexers to subscribe to all lifecycle changes for a specific bounty by filtering on `topics[2] = <bounty_id>`.
   - For **token configuration events** (`token_set`, `token_removed`), `topics[2]` is the token's `Address`.
   - For **contract-level config events** (`config`), `topics[2]` is the modified parameter name (`Symbol`).
   - For **initialization** (`initialized`), `topics[2..5]` record the core role addresses (`admin`, `relayer`, `arbiter`, `treasury`).
   - For **upgrade & pause events** (`paused`, `unpaused`, `upgraded`, `upgrade_proposed`, `upgrade_cancelled`), no extra topic is tagged; payloads reside in `data`.

### Filtering Guide for Indexers

To stream or query events via the Stellar RPC endpoint:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getEvents",
  "params": {
    "startLedger": 1000000,
    "filters": [
      {
        "type": "contract",
        "contractIds": ["CCEKBPYCWKNTXIQLXJHVAF7RYGUZZ6MITA7VVJBOK5BHXUO44ELZ4F4V"],
        "topics": [
          ["AAAABQAAAAYAYm91bnR5"],             // topics[0]: Symbol("bounty")
          ["AAAABQAAAAYAZnVuZGVk"]              // topics[1]: Symbol("funded")
        ]
      }
    ],
    "pagination": { "limit": 100 }
  }
}
```

---

## Event Catalog

| Event Name | Topics Count | Topic Layout | Emitted By | Description |
|---|---|---|---|---|
| [`funded`](#1-funded) | 3 | `["bounty", "funded", id: u64]` | `fund_bounty` | Bounty created and escrow funded |
| [`released`](#2-released) | 3 | `["bounty", "released", id: u64]` | `release` | Single winner paid (or deferred) |
| [`released_split`](#3-released_split) | 3 | `["bounty", "released_split", id: u64]` | `release_split` | Split payout to 1–5 winners |
| [`refunded`](#4-refunded) | 3 | `["bounty", "refunded", id: u64]` | `refund` | Full refund to maintainer |
| [`disputed`](#5-disputed) | 3 | `["bounty", "disputed", id: u64]` | `open_dispute` | Bounty frozen in dispute |
| [`resolved`](#6-resolved) | 3 | `["bounty", "resolved", id: u64]` | `resolve`, `resolve_with_evidence` | Arbiter split resolution |
| [`evidence`](#7-evidencerecorded) | 3 | `["bounty", "evidence", id: u64]` | `resolve_with_evidence` | SHA-256 evidence bundle hash |
| [`claimed`](#8-claimed) | 3 | `["bounty", "claimed", id: u64]` | `claim` | Deferred winner payout claimed |
| [`topped_up`](#9-toppedup) | 3 | `["bounty", "topped_up", id: u64]` | `top_up` | Maintainer added funds |
| [`deadline_extended`](#10-deadlineextended) | 3 | `["bounty", "deadline_extended", id: u64]` | `extend_deadline` | Bounty deadline lengthened |
| [`dispute_expired`](#11-disputeexpired) | 3 | `["bounty", "dispute_expired", id: u64]` | `expire_dispute` | Unresolved dispute timed out |
| [`initialized`](#12-initialized) | 4+ | `["bounty", "initialized", admin, relayer, arbiter, treasury]` | `init` | Contract deployment config initialized |
| [`config`](#13-configchanged) | 3 | `["bounty", "config", field: Symbol]` | Admin setters | Admin updated a configuration field |
| [`token_set`](#14-tokenset) | 3 | `["bounty", "token_set", token: Address]` | `set_token` | Token allowlist limits added/updated |
| [`token_removed`](#15-tokenremoved) | 3 | `["bounty", "token_removed", token: Address]` | `remove_token` | Token removed from allowlist |
| [`paused`](#16-paused) | 2 | `["bounty", "paused"]` | `pause` | Contract paused (blocks new bounties) |
| [`unpaused`](#17-unpaused) | 2 | `["bounty", "unpaused"]` | `unpause` | Contract unpaused |
| [`upgrade_proposed`](#18-upgradeproposed) | 2 | `["bounty", "upgrade_proposed"]` | `propose_upgrade` | Wasm upgrade proposed with timelock |
| [`upgrade_cancelled`](#19-upgradecancelled) | 2 | `["bounty", "upgrade_cancelled"]` | `cancel_upgrade` | Pending upgrade cancelled |
| [`upgraded`](#20-upgraded) | 2 | `["bounty", "upgraded"]` | `execute_upgrade` | Contract Wasm bytecode updated |

---

### 1. `funded`

Emitted when a maintainer calls `fund_bounty` to lock tokens against a GitHub issue.

- **Fires when:** Tokens successfully transfer from maintainer to escrow; bounty enters `Open` status.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("funded")`
  - `topics[2]`: `id: u64` (Unique bounty identifier)
- **Data fields (`Funded`):**

| Field | Type | Description |
|---|---|---|
| `maintainer` | `Address` | Maintainer account who funded the bounty and signs releases/refunds |
| `repo_id` | `u64` | GitHub repository identifier |
| `issue` | `u64` | GitHub issue number |
| `token` | `Address` | Contract address of the allowlisted token SAC |
| `amount` | `i128` | Total amount locked in stroops (token smallest unit) |
| `fee_bps` | `u32` | Snapshotted platform fee in basis points (1 bps = 0.01%) |
| `deadline` | `u64` | Unix timestamp deadline before which the bounty must be completed |
| `relayer_allowed` | `bool` | Whether automated relayer release is permitted upon PR merge |

**Example payload:**
```json
{
  "topics": ["bounty", "funded", 42],
  "data": {
    "maintainer": "GBZX...MAINTAINER",
    "repo_id": 987654321,
    "issue": 124,
    "token": "CAS3J7GYCCXGRMV5AAEDPDC3O2GB3CDY2TXZZBQSD7G2OACJJIZ56625",
    "amount": "1000000000",
    "fee_bps": 300,
    "deadline": 1735689600,
    "relayer_allowed": true
  }
}
```

---

### 2. `released`

Emitted when `release` is called to distribute funds to a single PR author.

- **Fires when:** An `Open` bounty is approved for release by the maintainer or an authorized relayer.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("released")`
  - `topics[2]`: `id: u64`
- **Data fields (`Released`):**

| Field | Type | Description |
|---|---|---|
| `winner` | `Address` | Recipient address (PR author) |
| `net` | `i128` | Net payout amount received by the winner (`amount - fee`) |
| `fee` | `i128` | Platform fee sent to the treasury (`floor(amount * fee_bps / 10000)`) |
| `caller` | `Address` | Address that executed the call (maintainer or relayer) |
| `deferred` | `bool` | `true` if token transfer to winner failed (e.g. no trustline) and funds were queued for `claim`; `false` if immediate transfer succeeded |

**Example payload:**
```json
{
  "topics": ["bounty", "released", 42],
  "data": {
    "winner": "GAWW...CONTRIBUTOR",
    "net": "970000000",
    "fee": "30000000",
    "caller": "GREL...RELAYER",
    "deferred": false
  }
}
```

---

### 3. `released_split`

Emitted when `release_split` is called to distribute funds across multiple contributors.

- **Fires when:** Maintainer or relayer releases an `Open` bounty divided across 1–5 distinct winners.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("released_split")`
  - `topics[2]`: `id: u64`
- **Data fields (`ReleasedSplit`):**

| Field | Type | Description |
|---|---|---|
| `payouts` | `Vec<SplitPayout>` | List of `{ winner: Address, amount: i128 }` structs for each winner (dust assigned to first) |
| `fee` | `i128` | Total platform fee deducted and transferred to treasury |
| `caller` | `Address` | Address of caller (maintainer or relayer) |

**Example payload:**
```json
{
  "topics": ["bounty", "released_split", 42],
  "data": {
    "payouts": [
      { "winner": "GAWW...CONTRIBUTOR1", "amount": "485000000" },
      { "winner": "GBWW...CONTRIBUTOR2", "amount": "485000000" }
    ],
    "fee": "30000000",
    "caller": "GBZX...MAINTAINER"
  }
}
```

---

### 4. `refunded`

Emitted when `refund` is called after a bounty has expired.

- **Fires when:** `now >= deadline + grace_secs`, bounty is in `Open` status, and maintainer requests full refund.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("refunded")`
  - `topics[2]`: `id: u64`
- **Data fields (`Refunded`):**

| Field | Type | Description |
|---|---|---|
| `amount` | `i128` | Exact amount returned in full to the maintainer (no fee charged) |

**Example payload:**
```json
{
  "topics": ["bounty", "refunded", 42],
  "data": {
    "amount": "1000000000"
  }
}
```

---

### 5. `disputed`

Emitted when `open_dispute` freezes an active bounty.

- **Fires when:** Maintainer or designated arbiter opens a dispute on an `Open` bounty.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("disputed")`
  - `topics[2]`: `id: u64`
- **Data fields (`Disputed`):**

| Field | Type | Description |
|---|---|---|
| `caller` | `Address` | Address that raised the dispute (maintainer or arbiter) |

**Example payload:**
```json
{
  "topics": ["bounty", "disputed", 42],
  "data": {
    "caller": "GBZX...MAINTAINER"
  }
}
```

---

### 6. `resolved`

Emitted when an arbiter resolves a disputed bounty via `resolve` or `resolve_with_evidence`.

- **Fires when:** Arbiter splits the disputed funds between a winner and the maintainer.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("resolved")`
  - `topics[2]`: `id: u64`
- **Data fields (`Resolved`):**

| Field | Type | Description |
|---|---|---|
| `winner` | `Option<Address>` | Selected winner address (or `null` if 0% winner basis points) |
| `winner_net` | `i128` | Payout received by the winner after fee deduction |
| `fee` | `i128` | Platform fee taken strictly from the winner's portion |
| `maintainer_amount`| `i128` | Remaining unawarded portion returned to the maintainer |
| `winner_deferred` | `bool` | `true` if winner transfer failed and is deferred to `claim` |

**Example payload:**
```json
{
  "topics": ["bounty", "resolved", 42],
  "data": {
    "winner": "GAWW...CONTRIBUTOR",
    "winner_net": "485000000",
    "fee": "15000000",
    "maintainer_amount": "500000000",
    "winner_deferred": false
  }
}
```

---

### 7. `evidence` (`EvidenceRecorded`)

Emitted exclusively by `resolve_with_evidence` immediately following the `resolved` event.

- **Fires when:** Arbiter provides an off-chain cryptographic digest of dispute arbitration evidence.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("evidence")`
  - `topics[2]`: `id: u64`
- **Data fields (`EvidenceRecorded`):**

| Field | Type | Description |
|---|---|---|
| `arbiter` | `Address` | Signing arbiter address |
| `evidence_hash` | `BytesN<32>` | SHA-256 hash of the canonical JSON dispute evidence bundle |

**Example payload:**
```json
{
  "topics": ["bounty", "evidence", 42],
  "data": {
    "arbiter": "GARB...ARBITER",
    "evidence_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }
}
```

---

### 8. `claimed`

Emitted when a contributor claims a deferred payout using `claim`.

- **Fires when:** A winner whose payout was previously deferred executes `claim(winner, id)` successfully.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("claimed")`
  - `topics[2]`: `id: u64`
- **Data fields (`Claimed`):**

| Field | Type | Description |
|---|---|---|
| `winner` | `Address` | Payout claimant address |
| `net` | `i128` | Net tokens transferred to claimant from claimable balance |

**Example payload:**
```json
{
  "topics": ["bounty", "claimed", 42],
  "data": {
    "winner": "GAWW...CONTRIBUTOR",
    "net": "970000000"
  }
}
```

---

### 9. `topped_up`

Emitted when a maintainer adds more funds to an open bounty via `top_up`.

- **Fires when:** Extra tokens are transferred from maintainer to escrow for an active bounty (up to 10 top-ups).
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("topped_up")`
  - `topics[2]`: `id: u64`
- **Data fields (`ToppedUp`):**

| Field | Type | Description |
|---|---|---|
| `extra` | `i128` | Additional token amount deposited |
| `new_amount` | `i128` | New cumulative bounty funding amount |

**Example payload:**
```json
{
  "topics": ["bounty", "topped_up", 42],
  "data": {
    "extra": "500000000",
    "new_amount": "1500000000"
  }
}
```

---

### 10. `deadline_extended`

Emitted when a maintainer lengthens an active bounty deadline via `extend_deadline`.

- **Fires when:** Maintainer submits a new deadline greater than the existing deadline.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("deadline_extended")`
  - `topics[2]`: `id: u64`
- **Data fields (`DeadlineExtended`):**

| Field | Type | Description |
|---|---|---|
| `old_deadline` | `u64` | Prior Unix timestamp deadline |
| `new_deadline` | `u64` | Updated Unix timestamp deadline |

**Example payload:**
```json
{
  "topics": ["bounty", "deadline_extended", 42],
  "data": {
    "old_deadline": 1735689600,
    "new_deadline": 1738368000
  }
}
```

---

### 11. `dispute_expired`

Emitted when an expired dispute is returned to `Open` via `expire_dispute`.

- **Fires when:** Current timestamp exceeds `dispute_opened_at + dispute_timeout` and caller resets the bounty.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("dispute_expired")`
  - `topics[2]`: `id: u64`
- **Data fields (`DisputeExpired`):** Empty tuple `()`.

**Example payload:**
```json
{
  "topics": ["bounty", "dispute_expired", 42],
  "data": null
}
```

---

### 12. `initialized`

Emitted once when the contract is deployed and initialized via `init`.

- **Fires when:** Contract constructor `init` registers initial roles, fees, and policy bounds.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("initialized")`
  - `topics[2]`: `admin: Address`
  - `topics[3]`: `relayer: Address`
  - `topics[4]`: `arbiter: Address`
  - `topics[5]`: `treasury: Address`
- **Data fields (`Initialized`):**

| Field | Type | Description |
|---|---|---|
| `fee_bps` | `u32` | Default basis points platform fee |
| `grace_secs` | `u64` | Default grace duration after deadline |
| `min_duration_secs`| `u64` | Minimum allowed bounty lifetime |
| `max_duration_secs`| `u64` | Maximum allowed bounty lifetime |
| `paused` | `bool` | Initial pause state |

**Example payload:**
```json
{
  "topics": [
    "bounty",
    "initialized",
    "GADM...ADMIN",
    "GREL...RELAYER",
    "GARB...ARBITER",
    "GTRS...TREASURY"
  ],
  "data": {
    "fee_bps": 300,
    "grace_secs": 259200,
    "min_duration_secs": 86400,
    "max_duration_secs": 7776000,
    "paused": false
  }
}
```

---

### 13. `config` (`ConfigChanged`)

Emitted when an admin updates any global contract configuration setting.

- **Fires when:** Admin invokes `set_fee`, `set_relayer`, `set_arbiter`, `set_treasury`, `set_grace`, `set_durations`, `set_admin`, `set_dispute_timeout`, or `set_upgrade_delay`.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("config")`
  - `topics[2]`: `field: Symbol` (Field name: `fee_bps`, `relayer`, `arbiter`, `treasury`, `grace_secs`, `durations`, `admin`, `dispute_timeout`, `upgrade_delay`)
- **Data fields (`ConfigChanged`):** Empty tuple `()`.

**Example payload:**
```json
{
  "topics": ["bounty", "config", "fee_bps"],
  "data": null
}
```

---

### 14. `token_set`

Emitted when an admin adds or modifies an allowlisted token via `set_token`.

- **Fires when:** An asset is authorized for escrow deposits with specific min and max amount thresholds.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("token_set")`
  - `topics[2]`: `token: Address`
- **Data fields (`TokenSet`):**

| Field | Type | Description |
|---|---|---|
| `min` | `i128` | Minimum funding amount allowed in stroops |
| `max` | `i128` | Maximum funding amount allowed in stroops |

**Example payload:**
```json
{
  "topics": [
    "bounty",
    "token_set",
    "CAS3J7GYCCXGRMV5AAEDPDC3O2GB3CDY2TXZZBQSD7G2OACJJIZ56625"
  ],
  "data": {
    "min": "10000000",
    "max": "5000000000"
  }
}
```

---

### 15. `token_removed`

Emitted when an admin removes an allowlisted token via `remove_token`.

- **Fires when:** A token is delisted from future bounty funding (existing bounties remain unaffected).
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("token_removed")`
  - `topics[2]`: `token: Address`
- **Data fields (`TokenRemoved`):** Empty tuple `()`.

**Example payload:**
```json
{
  "topics": [
    "bounty",
    "token_removed",
    "CAS3J7GYCCXGRMV5AAEDPDC3O2GB3CDY2TXZZBQSD7G2OACJJIZ56625"
  ],
  "data": null
}
```

---

### 16. `paused`

Emitted when the contract is paused via `pause`.

- **Fires when:** Admin or Guardian invokes `pause()`. Halts new deposits (`fund_bounty`).
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("paused")`
- **Data fields (`Paused`):** Empty tuple `()`.

**Example payload:**
```json
{
  "topics": ["bounty", "paused"],
  "data": null
}
```

---

### 17. `unpaused`

Emitted when the contract is resumed via `unpause`.

- **Fires when:** Admin invokes `unpause()`. Re-enables new deposits.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("unpaused")`
- **Data fields (`Unpaused`):** Empty tuple `()`.

**Example payload:**
```json
{
  "topics": ["bounty", "unpaused"],
  "data": null
}
```

---

### 18. `upgrade_proposed`

Emitted when an admin initiates a timelocked contract upgrade via `propose_upgrade`.

- **Fires when:** A new Wasm bytecode hash is staged for deployment subject to the timelock delay.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("upgrade_proposed")`
- **Data fields (`UpgradeProposed`):**

| Field | Type | Description |
|---|---|---|
| `wasm_hash` | `BytesN<32>` | SHA-256 hash of the proposed Wasm code uploaded to the network |
| `eta` | `u64` | Earliest Unix timestamp at which `execute_upgrade` may be called |

**Example payload:**
```json
{
  "topics": ["bounty", "upgrade_proposed"],
  "data": {
    "wasm_hash": "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90",
    "eta": 1738368000
  }
}
```

---

### 19. `upgrade_cancelled`

Emitted when an admin revokes a proposed upgrade via `cancel_upgrade`.

- **Fires when:** Admin aborts a pending upgrade proposal before execution.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("upgrade_cancelled")`
- **Data fields (`UpgradeCancelled`):**

| Field | Type | Description |
|---|---|---|
| `wasm_hash` | `BytesN<32>` | Hash of the cancelled Wasm bytecode |

**Example payload:**
```json
{
  "topics": ["bounty", "upgrade_cancelled"],
  "data": {
    "wasm_hash": "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"
  }
}
```

---

### 20. `upgraded`

Emitted when a contract upgrade is finalized on-chain via `execute_upgrade`.

- **Fires when:** The contract executable is swapped to the new Wasm hash after the timelock expires.
- **Topics:**
  - `topics[0]`: `Symbol("bounty")`
  - `topics[1]`: `Symbol("upgraded")`
- **Data fields (`Upgraded`):**

| Field | Type | Description |
|---|---|---|
| `wasm_hash` | `BytesN<32>` | Hash of the active contract Wasm bytecode |

**Example payload:**
```json
{
  "topics": ["bounty", "upgraded"],
  "data": {
    "wasm_hash": "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"
  }
}
```
