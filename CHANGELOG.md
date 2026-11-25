# Changelog

All notable changes to the `bounty_escrow` contract interface (entry points, events, errors, data structures, and storage keys) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> [!NOTE]
> This changelog is specifically maintained for **bindings consumers** (TypeScript/Python/Go client SDKs, CLI tooling, and indexers) tracking changes to the on-chain contract ABI, error numbering, storage layouts, and event payloads. For contributing guidelines on updating this changelog, see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## [Unreleased]

### Added
- **`LICENSE`**: Added Apache-2.0 license file, set package license field in `Cargo.toml`, and referenced in `README.md` (#118).
- **Issue & Pull Request Templates**: Added GitHub bug report (`bug_report.md`), feature request (`feature_request.md`), and PR checklist (`pull_request_template.md`) templates referencing `docs/CONTRACT.md` and verifying tests, error numbering, and documentation sync (#122).
- **Public Entry Point Rustdoc**: Wrote comprehensive `///` rustdoc documentation for all 36 public contract entry points in `contracts/bounty_escrow/src/lib.rs` detailing caller authorization, validation checks, and returned errors (#123).
- **Error Reference & Client Recovery Actions**: Documented all 31 error variants in `contracts/bounty_escrow/src/errors.rs` with causes, entry points, and UI recovery actions; added matching error reference table to `docs/CONTRACT.md` and linked from `README.md` (#125).
- **`docs/EVENTS.md`**: Dedicated contract events reference document detailing topic layouts, field schemas, indexing guidelines, and example JSON payloads for all 20 events (#124).
- **`SECURITY.md`**: Vulnerability disclosure policy establishing private reporting channels, 24–48h SLA response times, scope covering contracts and scripts, and safe harbor protections (#119).
- **`CONTRIBUTING.md`**: Contributor documentation detailing pinned toolchains, build-before-test ordering requirements, CI pipelines, error/storage stability invariants, and contract interface changelog requirements (#120).
- **`CHANGELOG.md`**: Initial interface changelog tracking breaking changes, entry points, errors, and events across contract releases (#121).

### Changed
- **BREAKING (interface):** `set_token` takes a fifth argument, `fee_bps: Option<u32>`, a per-token fee override (rejected above `MAX_FEE_BPS` with `InvalidFee`). Bindings must be regenerated.
- **BREAKING (interface):** `release`, `release_split` is unchanged, but `release` now returns `ReleaseSettlement { net, fee, deferred }`, `refund` returns `RefundSettlement { amount }`, and `resolve` and `resolve_with_evidence` return `ResolveSettlement { winner_net, fee, maintainer_amount, winner_deferred }`.
- **Behaviour:** release and resolve fees accrue to `Totals.accrued_fees` and are paid out with `withdraw_fees`, instead of being sent to the treasury immediately. The fee is snapshotted at funding from the token override, or the global `fee_bps` when there is none.
- **Behaviour:** `init` publishes the `initialized` event, and `set_token` rejects a non-positive minimum or `max < min` with `InvalidLimits`.
- **Internal:** the entry-point bodies moved out of `lib.rs` into `admin.rs` and `bounty.rs`. The contract spec is unchanged by that move.
- **Repair:** the default branch did not compile after merge damage to `fund_bounty`, `refund`, `init` and `set_token`. Restored `withdraw_fees`, `bounty_count`, `get_bounties`, `quote_*`, `bump` and `upgrade` from earlier commits.
- Restored and expanded complete contract specification in `docs/CONTRACT.md`, including sequence diagrams, 36 entry points, 31 errors, and worked fee calculation examples.

---

## [0.2.0] - 2026-11-23

Deployed to Testnet: [`CCEKBPYCWKNTXIQLXJHVAF7RYGUZZ6MITA7VVJBOK5BHXUO44ELZ4F4V`](https://stellar.expert/explorer/testnet/contract/CCEKBPYCWKNTXIQLXJHVAF7RYGUZZ6MITA7VVJBOK5BHXUO44ELZ4F4V) (see [`deployments/testnet.json`](deployments/testnet.json)).

### Breaking Interface Changes
- **Dropped `created_at` from `Bounty` struct (#183):** The `created_at` timestamp was removed from the serialized `Bounty` storage struct. The creation timestamp is derivable by indexers directly from the `Funded` event emitted at creation time. Dropping this field shrinks the serialized `Bounty` by ~16 bytes (`u64` + alignment), reducing on-chain storage rent and CPU consumption on every `set_bounty` rewrite.
- **Added `grace_secs` snapshot to `Bounty` struct (#163):** `Bounty` now snapshots `grace_secs` in force at funding time. A later `set_grace` call cannot retroactively alter the refund window for existing live bounties. *Migration notice:* Any pre-existing bounty serialized prior to this change deserializes with `grace_secs = 0` via Soroban's XDR trailing-field default.
- **Added `guardian` to `Config` struct (#175):** Added optional `guardian: Option<Address>` fast-pause key to `Config`. The guardian can trigger `pause()` in emergencies, but cannot unpause or transfer funds.
- **Extended `DataKey` Storage Enum:** Added new storage variants. *Warning: `DataKey` enum variants must never be reordered or inserted before existing variants, as Soroban relies on variant sequence for storage key encoding:*
  - `DataKey::DisputeTimeout` (Instance)
  - `DataKey::Dispute(u64)` (Persistent)
  - `DataKey::TopUps(u64)` (Persistent)
  - `DataKey::Claim(u64)` (Persistent)
  - `DataKey::ClaimableTotal(Address)` (Persistent)
  - `DataKey::RelayerLimit(Address)` (Persistent)
  - `DataKey::RelayerUsage(Address)` (Persistent)
  - `DataKey::UpgradeDelay` (Instance)
  - `DataKey::PendingUpgrade` (Instance)

### Added Entry Points
- `refundable_at(bounty_id: u64) -> u64`: Public view returning the exact Unix timestamp when an open bounty becomes refundable (`deadline + grace_secs`).
- `top_up(maintainer: Address, bounty_id: u64, extra: i128)`: Allows maintainers to deposit additional tokens into an open bounty (up to 10 top-ups and within token max limits).
- `extend_deadline(maintainer: Address, bounty_id: u64, new_deadline: u64)`: Allows maintainers to lengthen an open bounty's deadline.
- `expire_dispute(caller: Address, bounty_id: u64)`: Returns an unresolved disputed bounty back to `Open` after the dispute timeout has elapsed.
- `claim(winner: Address, bounty_id: u64)`: Allows a recipient of a deferred payout (e.g. initial transfer failed due to missing trustline) to collect their tokens.
- `resolve_with_evidence(arbiter: Address, bounty_id: u64, winner: Option<Address>, winner_bps: u32, evidence_hash: BytesN<32>)`: Resolves a dispute and anchors the SHA-256 digest of off-chain arbitration evidence on-chain.
- `set_dispute_timeout(admin: Address, secs: u64)` / `get_dispute_timeout() -> u64`: Configures and reads the dispute expiration period (defaults to 14 days).
- `set_relayer_limit(admin: Address, token: Address, window_secs: u64, cap: i128)`: Configures rolling-window volume caps for relayer-mediated releases.
- `set_upgrade_delay(admin: Address, delay: u64)`: Configures the timelock delay for contract bytecode upgrades (must be ≥ `max_duration + grace`).
- `propose_upgrade(admin: Address, wasm_hash: BytesN<32>)`: Initiates a timelocked contract code upgrade.
- `execute_upgrade(admin: Address)`: Finalizes an upgrade once the timelock delay has elapsed.
- `cancel_upgrade(admin: Address)`: Cancels a pending upgrade proposal.

### Added Error Codes (Stable Numbering: 18–31)
New error codes appended to `Error` enum (`contracts/bounty_escrow/src/errors.rs`). Existing error numbers 1–17 were strictly preserved:
- `18 TopUpLimitReached`: Exceeded maximum of 10 top-ups per bounty.
- `19 DisputeNotExpired`: Dispute timeout has not yet elapsed.
- `20 DisputeAlreadyUsed`: Bounty has already consumed its single permitted dispute.
- `21 NothingToClaim`: No deferred payout exists for the specified bounty.
- `22 AlreadyClaimed`: Deferred payout has already been claimed.
- `23 ConflictOfInterest`: Arbiter designated itself or the treasury as dispute resolution winner.
- `24 RelayerLimitExceeded`: Relayer release amount exceeds rolling time-window quota.
- `25 InvalidWinners`: Invalid winner addresses or share weights in `release_split`.
- `26 UpgradeNotReady`: Upgrade timelock delay has not yet matured.
- `27 NoPendingUpgrade`: No upgrade proposal is currently pending.
- `28 UpgradeDelayCannotDecrease`: Proposed upgrade delay cannot be shorter than existing delay.
- `29 InvalidKey`: Non-zero repository or issue ID required (#169).
- `30 ContractAddressRejected`: Contract address rejected as recipient (#168).
- `31 InvalidLimits`: Token minimum funding limit exceeds maximum funding limit (#170).

### Added Events
- `Initialized` (`["bounty", "initialized", admin, relayer, arbiter, treasury]`): Emitted upon initial contract deployment (#167).
- `EvidenceRecorded` (`["bounty", "evidence", id]`): Emitted by `resolve_with_evidence` with 32-byte evidence hash.
- `ToppedUp` (`["bounty", "topped_up", id]`): Emitted when additional tokens are deposited into an active bounty.
- `DeadlineExtended` (`["bounty", "deadline_extended", id]`): Emitted when an open bounty's deadline is lengthened.
- `DisputeExpired` (`["bounty", "dispute_expired", id]`): Emitted when an expired dispute is returned to `Open`.
- `Claimed` (`["bounty", "claimed", id]`): Emitted when a deferred winner payout is claimed.
- `TokenSet` (`["bounty", "token_set", token]`): Emitted when a token's allowlist limits are configured.
- `TokenRemoved` (`["bounty", "token_removed", token]`): Emitted when a token is removed from the allowlist.
- `UpgradeProposed` (`["bounty", "upgrade_proposed"]`): Emitted when a contract upgrade is proposed.
- `UpgradeCancelled` (`["bounty", "upgrade_cancelled"]`): Emitted when an upgrade proposal is cancelled.
- `Upgraded` (`["bounty", "upgraded"]`): Emitted when a contract bytecode upgrade is executed.

---

## [0.1.0] - 2026-11-20

### Added
- Initial implementation of the `bounty_escrow` Soroban contract for GitHub issue bounties.
- Core lifecycle entry points: `init`, `fund_bounty`, `release`, `release_split`, `refund`, `open_dispute`, `resolve`.
- Admin configuration entry points: `set_token`, `remove_token`, `set_fee`, `set_relayer`, `set_arbiter`, `set_treasury`, `set_grace`, `set_durations`, `set_admin`, `pause`, `unpause`.
- View entry points: `get_bounty`, `active_bounty`, `get_config`, `get_totals`, `get_token`.
- Core error codes 1–17 (`AlreadyInitialized` through `RelayerNotAllowed`).
- Core events: `funded`, `released`, `released_split`, `refunded`, `disputed`, `resolved`, `config`, `paused`, `unpaused`.
- Invariant accounting: `funded == released_net + fees + refunded + held`.
