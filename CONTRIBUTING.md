# Contributing to `escrow-contracts`

Thank you for contributing to the `bounty_escrow` Soroban contract! This repository holds real funds in escrow for GitHub issue bounties on the Stellar network. Because contract upgrades and storage layouts are immutable or subject to timelocks, maintaining stability, error number continuity, and rigorous testing standards is critical.

Please review these guidelines before submitting a pull request.

---

## Toolchain & Environment Setup

The repository pins all compiler and CLI versions to ensure reproducible builds across developer machines and CI.

### Required Tools

1. **Rust:** Version **1.99.1** pinned via [`rust-toolchain.toml`](rust-toolchain.toml).
   - Target: `wasm32v1-none`
   - Components: `rustfmt`, `clippy`
   - *Note:* If you use `rustup`, navigating into the repository automatically selects Rust 1.99.1 and installs the `wasm32v1-none` target.
2. **Stellar CLI:** Version **28.1.0**.
   - Install via Homebrew: `brew install stellar-cli`
   - Or install via Cargo: `cargo install --locked stellar-cli --version 28.1.0`
3. **Python 3:** Required for contract documentation consistency checks (`scripts/check_contract_docs.py`).
4. **ShellCheck:** For linting all shell scripts (`scripts/*.sh` and `scripts/tests/*.sh`).

### Optional / CI Tools

- `wasm-opt`: From `binaryen`, used by `scripts/build.sh` to optimize the output Wasm (`-Oz`).
- `cargo-deny`: `cargo install cargo-deny` (enforces license and advisory policies via `deny.toml`).
- `cargo-audit`: `cargo install cargo-audit` (checks RustSec advisories).
- `cargo-llvm-cov`: Version `0.9.1`, used to generate test coverage reports.

### Platform Considerations (Windows)

- Clone with LF line endings. Git on Windows with `core.autocrlf=true` can convert shell script line endings to CRLF, causing `$'\r': command not found` errors. Set `git config core.autocrlf input` before cloning.
- WSL2 (Ubuntu) is strongly recommended on Windows for running the bash build, deploy, and smoke scripts.

---

## Critical Build & Test Ordering

> [!IMPORTANT]
> **Always run `./scripts/build.sh` (or `make build`) BEFORE running `cargo test` or `cargo clippy`.**

### Why Build Before Test?

The contract upgrade test in the test suite embeds the compiled contract Wasm binary directly into the test runner binary using Rust's `include_bytes!` macro:

```rust
// contracts/bounty_escrow/src/test/admin.rs
const WASM: &[u8] = include_bytes!("../../../target/wasm32v1-none/release/bounty_escrow.wasm");
```

If you execute `cargo test` or `cargo clippy` on a clean checkout or after `cargo clean` without running the build script first, compilation fails immediately because `target/wasm32v1-none/release/bounty_escrow.wasm` does not exist.

### Recommended Local Workflow

Use the provided `Makefile` to execute checks in the exact order required:

```bash
make check   # Runs: build -> reproducible -> docs -> test-scripts -> fmt -> clippy -> test -> shellcheck
```

To run the full suite including dependency auditing and license checks:

```bash
make ci      # Runs: make check + cargo-deny + cargo-audit
```

Available `make` commands:

| Command | Action |
|---|---|
| `make build` | Builds the optimized Wasm and prints size and SHA-256 |
| `make size` | Builds Wasm and fails if size exceeds 40 KiB gate |
| `make reproducible` | Builds twice from clean targets and asserts identical SHA-256 hashes |
| `make docs` | Validates `README.md` and `docs/CONTRACT.md` against contract source |
| `make fmt` | Runs `cargo fmt --check` |
| `make clippy` | Runs `cargo clippy --all-targets --locked -- -D warnings` |
| `make test` | Builds Wasm first, then runs `cargo test --locked` |
| `make shellcheck` | Lints all shell scripts in `scripts/` |
| `make smoke-local` | Deploys and runs end-to-end smoke test against a local Stellar Quickstart container |

---

## Stability Invariants (Non-Negotiable)

When contributing code to `contracts/bounty_escrow/`, you must adhere to these architectural rules:

### 1. Never Renumber `Error`

The `Error` enum in [`contracts/bounty_escrow/src/errors.rs`](contracts/bounty_escrow/src/errors.rs) defines contract errors with explicit `#[repr(u32)]` numeric discriminants:

```rust
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    // ...
    InvalidLimits = 31,
}
```

- **Client Binding Stability:** External clients, frontend apps, indexers, and generated SDK bindings map error variants directly to these numeric codes.
- **Rules:**
  - **NEVER** renumber an existing error variant.
  - **NEVER** delete an error variant or reorder the existing enum list.
  - **NEVER** insert a new error variant in the middle.
  - **ALWAYS** append new errors at the end of the enum using the next sequential integer (e.g. `NewError = 32`).

### 2. Never Reorder `DataKey`

The `DataKey` enum in [`contracts/bounty_escrow/src/types.rs`](contracts/bounty_escrow/src/types.rs) defines storage keys for contract instance and persistent storage:

```rust
#[contracttype]
pub enum DataKey {
    Config,
    Roles,
    Policy,
    Paused,
    NextId,
    Bounty(u64),
    // ...
    PendingUpgrade,
}
```

- **Storage Serialization Stability:** In Soroban, `#[contracttype]` enums are serialized to XDR using their declaration index/discriminant.
- **Rules:**
  - **NEVER** change the order of existing variants in `DataKey`.
  - **NEVER** insert a new variant in between existing variants.
  - If existing variants are reordered, existing storage entries on deployed contracts will deserialize to the wrong keys or fail to deserialize entirely upon contract upgrade.
  - **ALWAYS** append new storage keys to the very end of `DataKey`.

### 3. Maintain the Accounting Invariant

Every state transition involving tokens must strictly satisfy:

$$\text{funded} = \text{released\_net} + \text{fees} + \text{refunded} + \text{held}$$
$$\text{held} + \text{claimable} = \text{contract token balance}$$

- `claimable` is the sum of winner payouts that could not be transferred immediately (e.g. no trustline) and were deferred.
- Any change affecting token flows must be validated against the invariant property test in `contracts/bounty_escrow/src/test/invariants.rs`.

---

## Continuous Integration (CI) Checks

Pull requests trigger `.github/workflows/ci.yml`. Every PR must pass all CI checks:

1. **`test` Job:**
   - Builds Wasm with `./scripts/build.sh`.
   - Asserts Wasm binary size is under 40 KiB (`scripts/check-wasm-size.sh`).
   - Asserts deterministic build reproducibility (`scripts/reproducible-build.sh`).
   - Asserts documentation sync with contract code (`python3 scripts/check_contract_docs.py`).
   - Runs deployment script unit tests (`scripts/tests/deploy-mainnet.test.sh`).
   - Enforces formatting (`cargo fmt --check`).
   - Enforces clippy without warnings (`cargo clippy --all-targets --locked -- -D warnings`).
   - Runs full test suite (`cargo test --locked`), including the resource budget check.
2. **`coverage` Job:** Runs `cargo llvm-cov` and outputs summary to job summary.
3. **`shellcheck` Job:** Lints all shell scripts.
4. **`audit` Job:** Validates Rust dependencies against the RustSec database (exceptions documented in `.cargo/audit.toml`).
5. **`deny` Job:** Validates crate licenses, advisories, and bans against `deny.toml`.
6. **`smoke-local` Job:** Spins up a Stellar Quickstart container, deploys the contract, and executes the end-to-end fund $\to$ release and fund $\to$ dispute $\to$ resolve flows.

---

## Contract Interface Change Process

If your PR introduces or alters any contract entry point, event, error, or data structure:

1. **Update Code:** Apply the changes in `contracts/bounty_escrow/src/`.
2. **Update Documentation:**
   - Update `docs/CONTRACT.md` under `## Entry points`, `## Errors`, and `## Events`.
   - Update `docs/EVENTS.md` if any event schema or topic layout changes.
   - Run `python3 scripts/check_contract_docs.py` to confirm that documentation matches code exactly.
   - If the total count of entry points changed, update the entry point count in `README.md`.
3. **Record in CHANGELOG:**
   - Add an entry under the `## [Unreleased]` section of [`CHANGELOG.md`](CHANGELOG.md).
   - Group entries under `### Added`, `### Changed`, `### Deprecated`, `### Removed`, `### Fixed`, or `### Security`.
   - Clearly highlight any breaking interface changes or storage migration considerations for downstream SDK bindings and indexers.
4. **Resource Budgets:**
   - If CPU instructions, memory bytes, or ledger read/write entries change, refresh the committed budget baseline intentionally:
     ```bash
     UPDATE_BUDGETS=1 cargo test --package bounty_escrow budgets -- --nocapture
     ```
   - Commit the updated [`docs/budgets.json`](docs/budgets.json).

---

## Pull Request Guidelines

- **Atomic Commits:** Follow atomic commit principles: each commit should represent a single, logical change with a descriptive commit message. Avoid bundling unrelated fixes or refactors.
- **Testing:** Include unit tests and/or property tests covering happy paths, boundary conditions, and unauthorized caller attempts (`env.set_auths(&[])`).
- **No Unsafe / Unchecked Panics:** Production contract code must not use `unwrap()`, `expect()`, raw slicing, or unconstrained casting. Strict clippy rules in `Cargo.toml` enforce this at compile time.
