# escrow-contracts

Soroban escrow contract for SkillEscrow Bounties: maintainers lock tokens on a GitHub issue, merging a PR pays its author, an unmerged bounty can be refunded after the deadline plus a grace period, and disputes go to an arbiter. Companion repo: [escrow-app](https://github.com/Soroban-escrow/escrow-app).

- Contract: `contracts/bounty_escrow` (Rust, soroban-sdk 29.0.1, target `wasm32v1-none`). Entry points, errors, events, fee math and trust model: [docs/CONTRACT.md](docs/CONTRACT.md).
- Testnet deployment: [deployments/testnet.json](deployments/testnet.json) (native XLM allowlisted; USDC not yet).

## Prerequisites
Rust 1.99.1 (pinned in `rust-toolchain.toml`, installs the `wasm32v1-none` target) and the `stellar` CLI 28.1.0.

## Build, test, lint
```sh
./scripts/build.sh          # optimized wasm, prints size and sha256
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                  # 63 tests incl. a property test of the funds invariant
```
Run `./scripts/build.sh` before `cargo test`: the upgrade test embeds the built wasm.

## Deploy
```sh
./scripts/deploy-testnet.sh        # needs stellar identities se-testnet-{admin,arbiter,treasury,relayer}
./scripts/smoke-testnet.sh         # fund->release and fund->dispute->resolve with native XLM
./scripts/bindings.sh              # regenerates the TS client into ../skillescrow-app/packages/contract-client
./scripts/deploy-mainnet.sh        # refuses without --i-have-human-approval and a typed confirmation
```
Mainnet needs an admin multisig, a verified USDC contract id, and an external audit first (see the app repo's `docs/SECURITY_REVIEW.md`).

## Security notes
The admin can upgrade the contract, which in effect controls escrowed funds. The relayer can only release bounties whose maintainer opted in, bounded by the per-token maximum. Pausing blocks only new funding. `cargo audit` ignores one advisory (a test-only dependency); see `.cargo/audit.toml`.
