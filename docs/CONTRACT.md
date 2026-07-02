# bounty_escrow contract

Soroban escrow for GitHub issue bounties. soroban-sdk 29.0.1, target `wasm32v1-none`.

## Lifecycle
`Open -> Released | Refunded | Disputed -> Resolved`. One active bounty per `(repo_id, issue)`.

## Entry points
| Function | Auth | Notes / errors |
|---|---|---|
| `init(admin, relayer, arbiter, treasury, fee_bps, grace_secs, min_dur, max_dur)` | admin | once (`AlreadyInitialized`); `fee_bps <= 1000` (`InvalidFee`); `min <= max` (`InvalidDeadline`) |
| `fund_bounty(maintainer, repo_id, issue, token, amount, deadline, relayer_allowed) -> id` | maintainer | `Paused`, `TokenNotAllowed`, `AmountTooSmall/Large`, `InvalidDeadline`, `IssueAlreadyFunded`. Fee bps snapshotted. |
| `release(caller, id, winner)` | maintainer, or relayer if `relayer_allowed` | `InvalidStatus`, `Unauthorized`, `RelayerNotAllowed`, `SelfPayout` (winner is the maintainer or the contract itself). Allowed after deadline until refunded. Pays `amount - fee` to winner, `fee` to treasury. |
| `refund(maintainer, id)` | maintainer | only at/after `deadline + grace` (`DeadlineNotReached`); full amount, no fee; works while paused |
| `open_dispute(caller, id)` | maintainer or arbiter | Open only; freezes release and refund |
| `resolve(arbiter, id, winner: Option, winner_bps)` | arbiter | Disputed only. `portion = floor(amount*bps/10000)`, fee taken from portion, remainder to maintainer. `InvalidSplit`, `SelfPayout`. Works while paused. |
| `get_bounty`, `active_bounty`, `get_config`, `get_totals`, `get_token` | none | read only |
| `set_token/remove_token/set_fee/set_relayer/set_arbiter/set_treasury/set_grace/set_durations` | admin | emit `config` event |
| `set_admin(admin, new_admin)` | admin and new_admin | two-party auth |
| `pause/unpause` | admin | blocks `fund_bounty` only |
| `upgrade(admin, wasm_hash)` | admin | emits `upgraded` |

## Errors
AlreadyInitialized=1 NotInitialized=2 Unauthorized=3 Paused=4 TokenNotAllowed=5 AmountTooSmall=6 AmountTooLarge=7 InvalidDeadline=8 IssueAlreadyFunded=9 BountyNotFound=10 InvalidStatus=11 DeadlineNotReached=12 SelfPayout=13 InvalidSplit=14 InvalidFee=15 Overflow=16 RelayerNotAllowed=17. Never renumber.

## Events
Topics `("bounty", <name>, id)`: `funded`, `released`, `refunded`, `disputed`, `resolved`, `config`, `paused`, `unpaused`, `upgraded`. Data carries every field the indexer needs.

## Fee math
`fee = floor(amount*bps/10000)` (dust goes to the receiver); `portion = floor(amount*winner_bps/10000)` (dust stays with the maintainer). Checked arithmetic, `Overflow` on failure.

## Accounting invariant
Per token: `funded == released_net + fees + refunded + held`, and `held == token.balance(contract)`. Checked after every step by the property test.

## TTL
Ledgers are ~5 s (17,280/day). Persistent entries and the instance are extended to 120 d whenever under 30 d remain; worst-case bounty life is 90 d + 3 d grace = 93 d (compile-time asserted). Testnet max entry TTL is 3,110,400 ledgers; re-check on mainnet.

## Trust model
- **Admin** can change config and **upgrade the code** (so can in effect move escrowed funds). Must be a multisig before mainnet.
- **Relayer** can release only bounties whose maintainer opted in, and only to a non-maintainer address; the per-token `max_amount` bounds exposure.
- **Arbiter** can open a dispute on any Open bounty and split it between a winner and the maintainer.
- Pausing never traps funds: refund and resolve keep working.

## Known gaps
`upgrade` test needs the wasm built first (`./scripts/build.sh` before `cargo test`, as CI does).
