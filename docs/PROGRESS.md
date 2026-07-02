# Progress (contracts)
Fresh build on a new machine, local only (no remote, no deploys).
- Phase 0/1 contract: done locally. 59 tests (fees, fund, release, refund, dispute, admin, proptest invariant), clippy clean, wasm 21,273 bytes.
- Not done: testnet deploy and smoke run (needs funded testnet keys; run scripts/deploy-testnet.sh), CI untested, actions not SHA-pinned.
