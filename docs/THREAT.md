# Threat model

Scope: the `bounty_escrow` Soroban contract and the operator keys around it. The contract holds token bounties and enforces *whether* a payment is allowed; the off-chain app decides *who* won by watching GitHub. Compromise of that app is an input-integrity risk, not a contract bug, and is called out explicitly below.

This contract has not been externally audited and the deployed admin is not yet a multisig — see [Before mainnet](../README.md#before-mainnet). Roles, powers and bounds are also summarised in the [README trust model](../README.md#trust-model).

## Trust assumptions

- Stellar consensus and the Stellar Asset Contract (SAC) behave as specified. A token that charges transfer fees or rebases balances can break the accounting invariant; only verified SACs should be allowlisted.
- The four operator roles (admin, relayer, arbiter, treasury) are distinct accounts, and `admin` becomes a multisig before mainnet.
- The winner is chosen off-chain. The contract cannot verify that a PR was merged on GitHub.
- Read-only simulations (`--send=no`) are trusted for inspection, but state-changing decisions are made from committed ledger state.

## Per-role compromise scenarios

| Role | Compromise scenario | Impact | Mitigation / bound in code |
|---|---|---|---|
| **Admin** | Admin key stolen, or a single admin acts alone | Rewrites fee, grace, durations, relayer, arbiter, treasury, the token allowlist, or itself; can pause new funding; can propose and execute a code upgrade | Upgrade waits at least `max_duration + grace` and the delay can only increase; `set_admin` needs the incoming admin's signature too; fee capped at 10% (`MAX_FEE_BPS`); `pause` never blocks `release`/`refund`/`resolve`; multisig is a mainnet prerequisite; see [runbook](RUNBOOK.md) |
| **Relayer** | Relayer key stolen or the merge-watching app is fooled | Releases opted-in bounties to an arbitrary non-maintainer address, including for a PR that was never merged | Only bounties funded with `relayer_allowed = true` are eligible; the caller must equal `config.relayer`; a rolling per-token cap (`set_relayer_limit`) bounds aggregate exposure; the maintainer can opt out entirely and sign the release; rotate immediately with `set_relayer` |
| **Arbiter** | Arbiter key stolen or arbiter colludes with a claimant | Opens a dispute on any `Open` bounty and splits a disputed bounty 100% to a colluding winner, taking the whole pot | Cannot touch an undisputed bounty; `winner_bps ≤ 10_000`; winner may not be the maintainer, the contract, the arbiter or the treasury (`SelfPayout`, `ConflictOfInterest`); only one dispute per bounty; `expire_dispute` returns an ignored dispute to `Open` after the timeout; `resolve_with_evidence` anchors a SHA-256 digest on-chain |
| **Maintainer** | Maintainer key stolen, or the maintainer acts in bad faith | Refunds their own bounty after the deadline, releases to a colluding winner, or inflates the pot with top-ups | Cannot pay themselves or the contract (`SelfPayout`); `refund` only at/after `deadline + grace` using the grace snapshotted at funding; one dispute per bounty; `extend_deadline` can only lengthen and is capped by `max_duration`; top-ups are capped by the token `max_amount` and 10 per bounty |
| **Winner / contributor** | Malicious account calls `claim` | Tries to claim a deferred payout that is not theirs, or claim twice | `claim` requires the recorded winner's signature, an existing unclaimed record, and the caller to equal the recorded winner; the amount was fixed when the payout was deferred |
| **Off-chain app / merge watcher** (external) | App is compromised or reads a fake merge event | Drives the relayer to release to an illegitimate "winner"; the contract will pay because it cannot verify GitHub | Maintainers who do not trust the automation set `relayer_allowed = false` and release manually; disputed bounties go to the arbiter; treat the app as a privileged integration and protect its release key like the relayer key |

Pausing is deliberately narrow: it blocks `fund_bounty` and `top_up` only, so a compromised admin cannot trap funds by pausing. `refund`, `release`, `resolve` and `claim` keep working.

## Token-level risks

| Risk | Scenario | Impact | Mitigation |
|---|---|---|---|
| Malicious, fee-on-transfer or rebasing token | Admin allowlists a token whose transfer deducts a fee or whose balances change externally | `held + claimable` no longer equals the contract's real balance; the accounting invariant breaks | Allowlist only verified SACs (native XLM and a USDC id checked against Circle's docs); `deploy-mainnet.sh` verifies the USDC id and cap; `smoke-testnet.sh` asserts exact balance deltas |
| Blacklisted or trustline-less recipient | A winner cannot receive the token | Payout would revert and could freeze the release | `release`/`settle` use `try_transfer`; on failure the net is stored as a claim and the transaction still succeeds, so the winner can collect later with `claim` |
| Token removed from the allowlist | Admin calls `remove_token` while bounties are open | No new funding or top-ups for that token | Existing `release`, `refund` and `resolve` do not require the token to still be allowlisted; see the [runbook](RUNBOOK.md) |
| Fee changed after funding | Admin calls `set_fee` | Could retroactively increase the fee | `fee_bps` is snapshotted into the `Bounty` at funding; `top_up` keeps the snapshot; worked vectors in [CONTRACT.md](CONTRACT.md#worked-examples) |
| Rounding dust on tiny amounts | Integer division floors `amount * bps / 10_000` | At most one stroop of precision per division | Dust is explicitly assigned to the receiver, the maintainer or the first split winner rather than burned; every documented number is asserted by `fees::tests::worked_examples` (see [#126](https://github.com/Soroban-escrow/escrow-contracts/issues/126)) |
| Unit confusion | An integrator passes whole tokens instead of stroops | Amounts off by 10^7, rejected by min/max or wildly overfunding | All limits and docs are in the token's smallest unit; min/max bounds are enforced in `fund_bounty` and `top_up` |

## Network-level risks

| Risk | Scenario | Impact | Mitigation |
|---|---|---|---|
| Release vs refund ordering | A release transaction and a refund race at the deadline boundary | Whichever lands first decides the outcome | Single `status` gate (`Open`) plus the `now ≥ deadline + grace` check; integrators should act well before the boundary; a maintainer can freeze the bounty with `open_dispute` |
| Transaction reordering / MEV | An observer front-runs a large release or an upgrade | Unfair ordering, or users reacting to an upgrade they did not see coming | Upgrades are proposed with a public `eta` at least `max_duration + grace` away and emit `upgrade_proposed`; `cancel_upgrade` is available until execution; no order-dependent fee logic exists |
| Timestamp drift | A validator reports a slightly different ledger timestamp | A refund or expiry becomes valid earlier or later than expected | The grace window absorbs small drift; `refundable_at` exposes the exact timestamp at which refund becomes valid; do not rely on the last few ledgers of a window |
| Ledger halt or TTL expiry | Entries expire during an incident | Bounty state becomes unreadable even though funds remain | Entries and the instance are extended to 120 days whenever under 30 days remain; worst-case bounty life is 93 days; testnet limits are checked at compile time, re-check mainnet settings |
| Congestion during an upgrade | `execute_upgrade` cannot be submitted in time | Delayed response, or a proposal expires awkwardly | `pause` and `set_relayer` are instant and independent of the upgrade queue; `execute_upgrade` can be retried after its `eta`; `cancel_upgrade` resets the queue |
| Malicious upgrade | Admin executes a wasm that steals all held funds | Total loss of all bounties | Timelock gives users an exit window; the delay can only increase; build reproducibly and compare the on-chain hash with `verify-deployment.sh`; multisig + external audit before mainnet |
| Compromised RPC or indexer | Stale or forged reads drive operator decisions | Operators act on the wrong state | `inspect.sh` and all runbook reads use `--send=no`; confirm critical state on-chain and with `verify-deployment.sh`; smoke tests assert balance deltas |

## Residual risks

- No external audit has been performed; the admin is not yet a multisig.
- The contract cannot verify GitHub events, so relayer-path correctness depends on the off-chain app and its key hygiene.
- A malicious or upgradeable token SAC on the allowlist can invalidate the accounting invariant; allowlisting is an admin trust decision.
- `expire_dispute` returns a bounty to `Open` without judging the merits, so a missing arbiter delays resolution but cannot decide who is right.

## Related issues and references

- [#126 — worked fee and split examples](https://github.com/Soroban-escrow/escrow-contracts/issues/126)
- [#127 — lifecycle sequence diagrams](https://github.com/Soroban-escrow/escrow-contracts/issues/127)
- [#128 — threat model](https://github.com/Soroban-escrow/escrow-contracts/issues/128)
- [#129 — operations runbook](https://github.com/Soroban-escrow/escrow-contracts/issues/129)
- [#135 — scheduled cargo audit](https://github.com/Soroban-escrow/escrow-contracts/issues/135); failed scheduled runs open an issue labelled `security` automatically (see the [audit job](../.github/workflows/ci.yml))
- [#163 — snapshot grace at funding](https://github.com/Soroban-escrow/escrow-contracts/issues/163)
- [#164](https://github.com/Soroban-escrow/escrow-contracts/issues/164) and [#165](https://github.com/Soroban-escrow/escrow-contracts/issues/165) — fee and portion input validation
- [All security-labelled issues](https://github.com/Soroban-escrow/escrow-contracts/issues?q=label%3Asecurity)
- [README: Before mainnet](../README.md#before-mainnet) · [CONTRACT.md](CONTRACT.md) · [RUNBOOK.md](RUNBOOK.md)
