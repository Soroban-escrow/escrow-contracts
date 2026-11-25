use soroban_sdk::contracterror;

/// Stable numbering: never renumber after deploy. Pinned by
/// `test::errors::error_codes_are_pinned` (#98) — update that test and the
/// `docs/CONTRACT.md` error table together with any change here.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// Contract configuration has already been initialized.
    ///
    /// Code: 1
    /// Entry points: `init`
    /// Recovery: Hide or disable the initialization form; the contract is already set up.
    AlreadyInitialized = 1,

    /// Contract configuration has not yet been initialized.
    ///
    /// Code: 2
    /// Entry points: All stateful entry points (`fund_bounty`, `release`, `refund`, admin functions, etc.)
    /// Recovery: Contract setup is required by the admin before any operations can proceed.
    NotInitialized = 2,

    /// The calling address lacks the required role authorization or signature verification failed.
    ///
    /// Code: 3
    /// Entry points: `release`, `release_split`, `refund`, `open_dispute`, `resolve`, `resolve_with_evidence`, `extend_deadline`, `top_up`, `claim`, and admin functions.
    /// Recovery: Prompt the user to switch their connected wallet to the authorized role account (admin, maintainer, arbiter, or winner).
    Unauthorized = 3,

    /// Contract funding and top-up operations are temporarily paused by the admin.
    ///
    /// Code: 4
    /// Entry points: `fund_bounty`, `top_up`
    /// Recovery: Disable deposit and top-up actions; note that refunds and dispute settlements remain active.
    Paused = 4,

    /// The specified token contract address is not present in the allowlist.
    ///
    /// Code: 5
    /// Entry points: `fund_bounty`, `top_up`, `remove_token`
    /// Recovery: Prompt the user to select an accepted asset from the supported token list (e.g. XLM or USDC).
    TokenNotAllowed = 5,

    /// The amount provided is below the token's configured minimum deposit limit (or `<= 0`).
    ///
    /// Code: 6
    /// Entry points: `fund_bounty`, `top_up`, `set_token`
    /// Recovery: Prompt the user to enter an amount meeting or exceeding the minimum deposit requirement.
    AmountTooSmall = 6,

    /// The bounty amount or cumulative total exceeds the token's maximum deposit limit.
    ///
    /// Code: 7
    /// Entry points: `fund_bounty`, `top_up`, `set_token`
    /// Recovery: Prompt the user to lower the amount beneath the token's maximum deposit cap.
    AmountTooLarge = 7,

    /// Deadline timestamp is out of range: outside the min/max duration window, earlier than current deadline, or invalid duration configuration.
    ///
    /// Code: 8
    /// Entry points: `init`, `fund_bounty`, `extend_deadline`, `set_durations`, `set_dispute_timeout`, `set_relayer_limit`
    /// Recovery: Highlight the date picker with permitted bounds (between `min_duration` and `max_duration` from current time; future-only for extensions).
    InvalidDeadline = 8,

    /// An active (`Open`) bounty already exists for the specified GitHub repository and issue.
    ///
    /// Code: 9
    /// Entry points: `fund_bounty`
    /// Recovery: Display notice that the issue is already funded; link to the active bounty or offer to top up instead.
    IssueAlreadyFunded = 9,

    /// No bounty record was found matching the supplied bounty ID.
    ///
    /// Code: 10
    /// Entry points: `release`, `release_split`, `refund`, `refundable_at`, `open_dispute`, `resolve`, `resolve_with_evidence`, `extend_deadline`, `top_up`, `expire_dispute`, `claim`, `get_bounty`
    /// Recovery: Display a 404 "Bounty Not Found" state; prompt user to verify the bounty ID.
    BountyNotFound = 10,

    /// Bounty is not in the required status for the requested operation.
    ///
    /// Code: 11
    /// Entry points: `release`, `release_split`, `refund`, `open_dispute`, `resolve`, `resolve_with_evidence`, `extend_deadline`, `top_up`, `expire_dispute`
    /// Recovery: Refresh on-chain bounty status; disable incompatible action buttons and show a badge explaining the current status.
    InvalidStatus = 11,

    /// Attempted refund before the bounty deadline plus snapshotted grace period has elapsed.
    ///
    /// Code: 12
    /// Entry points: `refund`
    /// Recovery: Display a countdown timer showing the remaining time until the bounty becomes refundable (`refundable_at`).
    DeadlineNotReached = 12,

    /// The recipient (winner) address matches the maintainer or the contract address.
    ///
    /// Code: 13
    /// Entry points: `release`, `release_split`, `resolve`, `resolve_with_evidence`
    /// Recovery: Prompt the user to enter a valid contributor recipient address; maintainer cannot pay themselves.
    SelfPayout = 13,

    /// Winner share percentages do not sum to exactly 10,000 bps (100%), `winner_bps > 10_000`, or winner address is missing when percentage is positive.
    ///
    /// Code: 14
    /// Entry points: `release_split`, `resolve`, `resolve_with_evidence`
    /// Recovery: Validate split inputs in the UI; ensure shares sum to 100% and recipient address is provided.
    InvalidSplit = 14,

    /// Configured fee basis points exceed the maximum allowable ceiling (1,000 bps = 10%).
    ///
    /// Code: 15
    /// Entry points: `init`, `set_fee`
    /// Recovery: Constrain fee input between 0% and 10% (0–1,000 bps) and alert admin if out of range.
    InvalidFee = 15,

    /// Mathematical overflow occurred during fee calculation, basis points split, timestamp arithmetic, or balance accounting.
    ///
    /// Code: 16
    /// Entry points: Internal math across funding, releases, splits, refunds, and timelocks
    /// Recovery: Report an unexpected arithmetic overflow error; advise reviewing input numbers or contacting technical support.
    Overflow = 16,

    /// Automated relayer release attempted on a bounty where the maintainer opted out (`relayer_allowed == false`).
    ///
    /// Code: 17
    /// Entry points: `release`, `release_split`
    /// Recovery: Inform user that automated relayer release is disabled for this bounty; prompt maintainer to sign the release directly.
    RelayerNotAllowed = 17,

    /// Bounty has already reached the maximum limit of 10 top-up deposits.
    ///
    /// Code: 18
    /// Entry points: `top_up`
    /// Recovery: Disable top-up button; explain that at most 10 top-ups are permitted per bounty to conserve on-chain storage.
    TopUpLimitReached = 18,

    /// Attempted to expire a dispute before the configured dispute timeout window has elapsed.
    ///
    /// Code: 19
    /// Entry points: `expire_dispute`
    /// Recovery: Display a countdown timer showing when the dispute reaches its expiration timestamp.
    DisputeNotExpired = 19,

    /// A dispute was already opened previously for this bounty.
    ///
    /// Code: 20
    /// Entry points: `open_dispute`
    /// Recovery: Inform caller that each bounty may only enter dispute once during its lifecycle to prevent griefing loops.
    DisputeAlreadyUsed = 20,

    /// No deferred claim record exists for this bounty ID.
    ///
    /// Code: 21
    /// Entry points: `claim`
    /// Recovery: Display notice that no deferred payout exists; prompt user to inspect transaction history for direct payout.
    NothingToClaim = 21,

    /// The deferred payout has already been claimed by the winner.
    ///
    /// Code: 22
    /// Entry points: `claim`
    /// Recovery: Show "Already Claimed" status badge with link to the claiming transaction.
    AlreadyClaimed = 22,

    /// The arbiter named itself or the platform treasury address as resolution winner.
    ///
    /// Code: 23
    /// Entry points: `resolve`, `resolve_with_evidence`
    /// Recovery: Block recipient selection in dispute resolution form; remind arbiter that self or treasury awards are forbidden.
    ConflictOfInterest = 23,

    /// Relayer release volume has exceeded the configured rolling rate-limit quota for the token.
    ///
    /// Code: 24
    /// Entry points: `release`, `release_split`
    /// Recovery: Display relayer rate limit warning; advise waiting for the rolling window to reset or ask maintainer to release manually.
    RelayerLimitExceeded = 24,

    /// Winners list for split release is empty, exceeds 5 recipients, or contains duplicate addresses.
    ///
    /// Code: 25
    /// Entry points: `release_split`
    /// Recovery: Validate split list UI; prompt user to provide between 1 and 5 unique recipient addresses.
    InvalidWinners = 25,

    /// Attempted to execute an upgrade before the mandatory timelock delay has elapsed.
    ///
    /// Code: 26
    /// Entry points: `execute_upgrade`
    /// Recovery: Display a timelock countdown showing when the pending contract upgrade becomes executable.
    UpgradeNotReady = 26,

    /// Attempted to execute or cancel an upgrade when no upgrade proposal exists.
    ///
    /// Code: 27
    /// Entry points: `execute_upgrade`, `cancel_upgrade`
    /// Recovery: Inform admin that there is no active upgrade proposal in the queue.
    NoPendingUpgrade = 27,

    /// Proposed upgrade delay is shorter than the currently established delay.
    ///
    /// Code: 28
    /// Entry points: `set_upgrade_delay`
    /// Recovery: Validate input to enforce that timelock delay can only increase or remain the same, never decrease.
    UpgradeDelayCannotDecrease = 28,

    /// Repository ID or issue number is 0.
    ///
    /// Code: 29
    /// Entry points: `fund_bounty`
    /// Recovery: Validate GitHub repository ID and issue number inputs; ensure positive non-zero integers.
    /// Note: Currently unreachable in contract code; retained for ABI stability (see `test::errors::every_error_variant_is_mapped_to_a_test`).
    InvalidKey = 29,

    /// A contract address was provided where a standard user account address is required.
    ///
    /// Code: 30
    /// Entry points: Address validation checks
    /// Recovery: Prompt user to enter a standard Stellar account address (G...) rather than a contract address (C...).
    /// Note: Currently unreachable in contract code; retained for ABI stability (see `test::errors::every_error_variant_is_mapped_to_a_test`).
    ContractAddressRejected = 30,

    /// Configured token limits are invalid (`min <= 0` or `max < min`).
    ///
    /// Code: 31
    /// Entry points: `set_token`
    /// Recovery: Validate token configuration form; ensure minimum deposit is positive and maximum deposit is greater than or equal to minimum.
    InvalidLimits = 31,
}
