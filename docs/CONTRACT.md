# bounty_escrow contract

Soroban escrow for GitHub issue bounties. soroban-sdk 29.0.1, target `wasm32v1-none`.

## Lifecycle
`Open -> Released | Refunded | Disputed -> Resolved`, plus `Disputed -> Open` when a dispute expires. `Open` bounties can be extended (`extend_deadline`) and topped up (`top_up`). One active bounty per `(repo_id, issue)`.

## Entry points
| Function | Auth | Notes / errors |
|---|---|---|
| `init(admin, relayer, arbiter, treasury, fee_bps, grace_secs, min_dur, max_dur)` | admin | once (`AlreadyInitialized`); `fee_bps <= 1000` (`InvalidFee`); `min <= max` (`InvalidDeadline`) |
| `fund_bounty(maintainer, repo_id, issue, token, amount, deadline, relayer_allowed) -> id` | maintainer | `Paused`, `TokenNotAllowed`, `AmountTooSmall/Large`, `InvalidDeadline`, `IssueAlreadyFunded`. Fee bps snapshotted. |
| `release(caller, id, winner)` | maintainer, or relayer if `relayer_allowed` | `InvalidStatus`, `Unauthorized`, `RelayerNotAllowed`, `SelfPayout` (winner is the maintainer or the contract itself). Allowed after deadline until refunded. Pays `amount - fee` to winner, `fee` to treasury. If the winner's transfer fails (for example, no trustline), the release still succeeds and the net becomes a claim (`deferred = true`). |
| `refund(maintainer, id)` | maintainer | only at/after `deadline + grace` (`DeadlineNotReached`); full amount, no fee; works while paused |
| `open_dispute(caller, id)` | maintainer or arbiter | Open only; freezes release and refund. One dispute per bounty (`DisputeAlreadyUsed`). |
| `expire_dispute(caller, id)` | anyone | Disputed only; `now >= opened_at + dispute_timeout` (`DisputeNotExpired`). Returns the bounty to `Open` with no judgement. Default timeout 14 days. |
| `extend_deadline(maintainer, id, new_deadline)` | maintainer | Open only; `new_deadline > deadline` and `<= now + max_duration` (`InvalidDeadline`). Emits `deadline_extended`. |
| `top_up(maintainer, id, extra)` | maintainer | Open only; `Paused`, `TokenNotAllowed` if the token was removed, `AmountTooSmall` if `extra <= 0`, `AmountTooLarge` if the new total exceeds the token's `max_amount`, `TopUpLimitReached` after 10 top-ups. The fee snapshot is unchanged. Emits `topped_up`. |
| `claim(winner, id)` | the recorded winner | A deferred payout must exist (`NothingToClaim`), be unclaimed (`AlreadyClaimed`), and belong to the caller (`Unauthorized`). Pays the net. Emits `claimed`. |
| `resolve(arbiter, id, winner: Option, winner_bps)` | arbiter | Disputed only. `portion = floor(amount*bps/10000)`, fee taken from portion, remainder to maintainer. `InvalidSplit`, `SelfPayout`, `ConflictOfInterest` (winner is the arbiter or the treasury). Works while paused. A winner transfer that fails is deferred to `claim` instead of reverting. |
| `resolve_with_evidence(arbiter, id, winner, winner_bps, evidence_hash)` | arbiter | Same rules and errors as `resolve`. Also emits `evidence` with the 32-byte `evidence_hash`. See "Resolution evidence". |
| `get_bounty`, `active_bounty`, `get_config`, `get_totals`, `get_token`, `get_dispute_timeout` | none | read only |
| `set_token/remove_token/set_fee/set_relayer/set_arbiter/set_treasury/set_grace/set_durations/set_dispute_timeout` | admin | emit `config` event |
| `set_admin(admin, new_admin)` | admin and new_admin | two-party auth |
| `pause/unpause` | admin | blocks `fund_bounty` only |
| `upgrade(admin, wasm_hash)` | admin | emits `upgraded` |

## Errors
AlreadyInitialized=1 NotInitialized=2 Unauthorized=3 Paused=4 TokenNotAllowed=5 AmountTooSmall=6 AmountTooLarge=7 InvalidDeadline=8 IssueAlreadyFunded=9 BountyNotFound=10 InvalidStatus=11 DeadlineNotReached=12 SelfPayout=13 InvalidSplit=14 InvalidFee=15 Overflow=16 RelayerNotAllowed=17 TopUpLimitReached=18 DisputeNotExpired=19 DisputeAlreadyUsed=20 NothingToClaim=21 AlreadyClaimed=22 ConflictOfInterest=23 UnexpectedTransferAmount=24. Never renumber.

## Events
Topics `("bounty", <name>, id)`: `funded`, `released` (`deferred`), `refunded`, `disputed`, `resolved` (`winner_deferred`), `evidence` (`evidence_hash`), `deadline_extended`, `topped_up`, `dispute_expired`, `claimed`, `config`, `paused`, `unpaused`, `upgraded`. Config events include the changed value; pause, unpause and upgrade events include the acting admin. Data carries every field the indexer needs.

## Fee math
`fee = floor(amount*bps/10000)` (dust goes to the receiver); `portion = floor(amount*winner_bps/10000)` (dust stays with the maintainer). Checked arithmetic, `Overflow` on failure.

## Accounting invariant
Per token: `funded == released_net + fees + refunded + held`, and `held + claimable == token.balance(contract)`, where `claimable` is the sum of unclaimed deferred payouts (`DataKey::ClaimableTotal`).

A deferred payout moves its net from `held` to `claimable` and does not touch `released_net`. `claim` moves it from `claimable` to `released_net`. `top_up` raises `funded` and `held` together. The property test has not been updated for `claimable` or the new operations yet; it still checks `held == balance` and will fail on any sequence that defers a payout.

## TTL
Ledgers are ~5 s (17,280/day). Persistent entries and the instance are extended to 120 d whenever under 30 d remain; worst-case bounty life is 90 d + 3 d grace = 93 d (compile-time asserted). Testnet max entry TTL is 3,110,400 ledgers; re-check on mainnet.

## Resource budgets
`contracts/bounty_escrow/src/test/budgets.rs` runs each state-changing entry point in the table below through `Env::cost_estimate()` and compares the result with `docs/budgets.json`. The test fails if CPU instructions, memory, read/write entries or read/write bytes exceed the snapshot by more than its `tolerance_percent` (10%). Refresh the snapshot deliberately after an intentional change:

```bash
UPDATE_BUDGETS=1 cargo test --package bounty_escrow budgets -- --nocapture
```

The numbers below are the committed budget ceilings, not an on-chain simulation. The local test host runs native Rust, so it underestimates Wasm execution; use `stellar contract invoke --cost` or RPC simulation before a mainnet release.

| Entry point | CPU instructions | Memory bytes | Read entries | Write entries | Read bytes | Write bytes |
|---|---:|---:|---:|---:|---:|---:|
| `init` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `fund_bounty` | 8,000,000 | 8,000,000 | 80 | 80 | 80,000 | 40,000 |
| `release` (fee) | 10,000,000 | 10,000,000 | 100 | 100 | 100,000 | 50,000 |
| `release` (no fee) | 9,000,000 | 9,000,000 | 90 | 90 | 90,000 | 45,000 |
| `refund` | 8,000,000 | 8,000,000 | 80 | 80 | 80,000 | 40,000 |
| `open_dispute` | 5,000,000 | 5,000,000 | 50 | 50 | 50,000 | 25,000 |
| `resolve` (split) | 15,000,000 | 15,000,000 | 150 | 150 | 150,000 | 75,000 |
| `resolve` (zero) | 12,000,000 | 12,000,000 | 120 | 120 | 120,000 | 60,000 |
| `resolve_with_evidence` | 15,000,000 | 15,000,000 | 150 | 150 | 150,000 | 75,000 |
| `top_up` | 8,000,000 | 8,000,000 | 80 | 80 | 80,000 | 40,000 |
| `extend_deadline` | 5,000,000 | 5,000,000 | 50 | 50 | 50,000 | 25,000 |
| `expire_dispute` | 5,000,000 | 5,000,000 | 50 | 50 | 50,000 | 25,000 |
| `set_token` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `remove_token` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_fee` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_relayer` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_arbiter` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_treasury` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_grace` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_durations` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_admin` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `pause` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `unpause` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `set_dispute_timeout` | 3,000,000 | 3,000,000 | 40 | 40 | 40,000 | 20,000 |
| `upgrade` | 6,000,000 | 6,000,000 | 60 | 60 | 60,000 | 30,000 |

Mainnet limits from the `soroban-sdk` 29.0.1 snapshot (2026-07-10), so the headroom is visible:

| Limit | Value |
|---|---:|
| CPU instructions | 400,000,000 |
| Memory bytes | 41,943,040 |
| Disk read entries | 200 |
| Write entries | 200 |
| Ledger entries | 400 |
| Disk read bytes | 200,000 |
| Write bytes | 132,096 |
| Contract events | 16,384 bytes |
| Contract data key | 250 bytes |
| Contract data entry | 65,536 bytes |
| Contract code entry | 131,072 bytes |

CI also runs `scripts/check-wasm-size.sh` after the build. It fails above 40 KiB and writes the size, headroom and SHA-256 to the job summary.

## Trust model
- **Admin** can change config and **upgrade the code** (so can in effect move escrowed funds). Must be a multisig before mainnet.
- **Relayer** can release only bounties whose maintainer opted in, and only to a non-maintainer address; the per-token `max_amount` bounds exposure.
- **Arbiter** can open a dispute on any Open bounty and split it between a winner and the maintainer.
- **Maintainer** can open a dispute on their own bounty, but only once per bounty. An unresolved dispute expires to `Open` after `dispute_timeout`, so a missing arbiter cannot freeze funds. The timeout expiry does not decide who is right: it returns the bounty to the normal rules (refund after `deadline + grace`).
- **Maintainer** can extend the deadline (never shorten it) and top up the amount while the bounty is open, up to the per-token `max_amount` and 10 top-ups.
- Pausing never traps funds: refund and resolve keep working.

## Known gaps
`upgrade` test needs the wasm built first (`./scripts/build.sh` before `cargo test`, as CI does).

## Resolution evidence
`resolve_with_evidence` records a 32-byte `evidence_hash` in the `evidence` event, so the evidence behind a split is anchored on-chain. Suggested digest: SHA-256 over a canonical JSON bundle of the dispute reason, the AI reviews and the arbiter's note, with keys sorted and no insignificant whitespace. The contract does not check the bundle; it stores the hash.

## Deployment verification
`scripts/reproducible-build.sh` builds twice and compares hashes. `scripts/verify-deployment.sh <network>` compares the deployed Wasm with the `wasm_hash` recorded in `deployments/<network>.json`.
