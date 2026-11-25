## Description

<!-- Provide a concise description of the changes introduced in this PR. -->

## Related Issues

<!-- Closes #123, Fixes #456, etc. -->
Closes #

## Affected Entry Points & Specifications

<!--
List all contract entry points affected by this pull request.
Consult docs/CONTRACT.md for entry point specifications:
https://github.com/Soroban-escrow/escrow-contracts/blob/main/docs/CONTRACT.md
-->
- [ ] No contract entry points affected (docs/tooling/CI only)
- [ ] Contract entry points affected:

## Pull Request Checklist

Please verify and check each item before submitting:

- [ ] **Tests pass:** `make check` (or `./scripts/build.sh && cargo test --locked`) passes locally.
- [ ] **Auth & boundary testing:** New or updated entry points have unit tests covering authorized callers and unauthorized rejection (`env.set_auths(&[])`).
- [ ] **Accounting invariants:** If token movements were modified, the accounting invariant (`funded == released_net + fees + refunded + held` and `held + claimable == balance`) is preserved and covered by property tests.
- [ ] **Error numbering maintained:**
  - Existing error codes in `contracts/bounty_escrow/src/errors.rs` have **NOT** been renumbered, deleted, or reordered.
  - Any new error codes are strictly appended with the next sequential integer.
  - New error codes are documented in [`docs/CONTRACT.md#errors`](docs/CONTRACT.md#errors).
- [ ] **README sync:**
  - The entry point count in [`README.md`](README.md) matches the contract's public `pub fn` count.
  - Any error changes or contract changes are reflected in [`README.md`](README.md).
- [ ] **Contract docs sync:** `python3 scripts/check_contract_docs.py` passes with no discrepancies between `contracts/bounty_escrow/src/` and [`docs/CONTRACT.md`](docs/CONTRACT.md) / [`README.md`](README.md).
- [ ] **CHANGELOG sync:** Changes are recorded under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md) according to [`CONTRIBUTING.md`](CONTRIBUTING.md).
- [ ] **Code quality & linting:** Code is formatted (`cargo fmt --check`) and clippy passes with no warnings (`cargo clippy --all-targets --locked -- -D warnings`).
- [ ] **Atomic commits:** Commits adhere to atomic commit principles (single, logical changes per commit).
