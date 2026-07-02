# Progress (contracts)
Fresh build on a new machine, local only (no remote, no deploys).
- Phase 0/1 contract: done locally. 59 tests (fees, fund, release, refund, dispute, admin, proptest invariant), clippy clean, wasm 21,273 bytes.
- Testnet deployed (see deployments/testnet.json); smoke: fund->release and fund->dispute->resolve 70/30 passed with native XLM (scripts/smoke-testnet.sh). Refund path covered by unit tests only (grace is 72 h). USDC not allowlisted yet: verify the issuer from Circle docs first.
- Not done: CI untested, actions not SHA-pinned.
