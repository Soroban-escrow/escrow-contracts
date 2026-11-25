# Security Policy

The `bounty_escrow` contract holds real tokens in escrow on the Stellar network. We take security vulnerabilities very seriously and are committed to protecting user funds and platform integrity.

---

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues, discussions, or pull requests.**

If you discover a vulnerability or security issue, report it through one of our private disclosure channels:

1. **GitHub Private Vulnerability Advisory (Preferred):**  
   Open a private advisory at:  
   [https://github.com/Soroban-escrow/escrow-contracts/security/advisories/new](https://github.com/Soroban-escrow/escrow-contracts/security/advisories/new)
2. **Security Email:**  
   Email our security team at `security@soroban-escrow.org` with details of the vulnerability. If sensitive, please request our PGP public key before transmitting sensitive proof-of-concept material.

### What to Include in Your Report

To help us investigate and triage quickly, please include:
- A clear description of the vulnerability and its potential impact.
- Step-by-step instructions to reproduce the issue, including environment setup or test scripts (e.g., a Rust unit test or Soroban CLI command sequence).
- Affected contract code, function entry points, or script paths.
- Potential impact on escrow invariants, user balances, or access controls.
- Any suggested fixes, remediations, or defensive mitigations.

---

## Response Times and Service Level Agreements (SLAs)

We are committed to rapid response and transparent coordination:

| Milestone | Target Response Time | Description |
|---|---|---|
| **Initial Acknowledgement** | Within **24 hours** | Confirmation that your report was received and assigned to a security engineer. |
| **Triage & Assessment** | Within **48–72 hours** | Initial validation, severity classification (Critical, High, Medium, Low), and CVSS rating. |
| **Status Updates** | At least every **48 hours** | Regular updates while root cause analysis, test writing, and patching are underway. |
| **Resolution / Patch** | Timeline varies by severity | Critical fixes are fast-tracked; code upgrades respect contract timelock delays (`UpgradeDelay`). |
| **Public Disclosure** | Up to **90 days** | Coordinated disclosure window following remediation, unless mutually agreed earlier. |

---

## Scope

### In Scope

Our security disclosure process covers all code within this repository:

1. **Smart Contracts (`contracts/bounty_escrow/`):**
   - Unauthorized withdrawal, draining, or trapping of escrowed funds.
   - Violations of the core accounting invariants:
     - `funded == released_net + fees + refunded + held`
     - `held + claimable == token.balance(contract)`
   - Authentication bypasses or privilege escalation in entry points (`fund_bounty`, `release`, `release_split`, `refund`, `resolve`, `set_admin`, `pause`, etc.).
   - Re-entrancy, overflow, unchecked arithmetic, or rounding vulnerabilities in fee/payout math.
   - Storage eviction, TTL misconfiguration, or `DataKey` collision/corruption vulnerabilities.
   - Timelock bypasses or governance risks in `propose_upgrade` / `execute_upgrade`.
   - Griefing or state corruption in dispute resolution and claims mechanisms.

2. **Deployment and Operational Scripts (`scripts/`):**
   - Security vulnerabilities, injection flaws, or key exposure in shell scripts (`deploy-testnet.sh`, `deploy-mainnet.sh`, `smoke-testnet.sh`, `verify-deployment.sh`, `reproducible-build.sh`, etc.).
   - Flaws in reproducible build verification or deployment integrity checks.
   - Dry-run validation bypasses or unsafe defaults in deployment procedures.

3. **CI/CD Workflows (`.github/workflows/`):**
   - Supply chain vulnerabilities, untrusted action usage, or secret leakage risks in CI pipelines.

### Out of Scope

The following areas are excluded from vulnerability reporting:

- Distributed Denial of Service (DDoS) against public Stellar Horizon or Soroban RPC nodes.
- Attacks requiring physical access to a developer's unlocked machine.
- Social engineering, phishing, or physical attacks against maintainers or contributors.
- Vulnerabilities in third-party protocols or dependencies (e.g. underlying Stellar network consensus, Soroban SDK bugs) unless an exploitable impact on `bounty_escrow` is demonstrated.
- Issues in already-deprecated or legacy test contracts on ephemeral networks without real funds.

---

## Security Architecture & Invariants

The contract is designed with several defense-in-depth security principles:

- **Funds Can Always Exit:** Pausing blocks new bounty funding (`fund_bounty`), but never freezes withdrawals (`refund` and `resolve` continue to function).
- **Snapshot Fee Protection:** Platform fees (`fee_bps`) and grace durations (`grace_secs`) are snapshotted at funding time. Future admin adjustments cannot retroactively alter terms for existing bounties.
- **Timelocked Upgrades:** Contract bytecode upgrades require a mandatory delay (`UpgradeDelay` ≥ `max_duration + grace`), providing users an exit window before new code executes.
- **Fast-Pause Guardian:** An optional guardian role can pause funding immediately in an emergency, without possessing any power to move tokens or unpause.
- **Two-Party Admin Transfer:** Admin transfer requires signatures from both the existing admin and the prospective admin, preventing accidental lockout due to typos.
- **Strict Clippy & Static Analysis:** The codebase strictly prohibits unhandled panics, unwrap/expect in production code paths, and checked arithmetic is enforced across all token balances.

---

## Safe Harbor Policy

We consider security research conducted in good faith to be an invaluable service to our users. We commit not to pursue legal action against security researchers who:

- Make a good-faith effort to avoid privacy violations, data destruction, and service interruption.
- Report identified vulnerabilities promptly through our designated private disclosure channels.
- Give us reasonable time to remediate the vulnerability prior to public disclosure.
- Do not exploit a vulnerability to extract funds beyond what is necessary to produce a benign proof-of-concept.
