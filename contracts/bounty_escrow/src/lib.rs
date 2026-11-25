#![no_std]

mod admin;
mod bounty;
mod errors;
mod events;
mod fees;
mod math;
mod storage;
mod types;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, token, Address, BytesN, ContractExecutable, Env, Symbol,
    Vec,
};

pub use errors::Error;
use math::{add, sub};
pub use types::{
    Bounty, Config, DataKey, FundBountyParams, InitParams, SplitPayout, Status, TokenLimits,
    Totals, WinnerShare,
};
use types::{
    ClaimRecord, DisputeRecord, PendingUpgrade, RefundSettlement, RelayerLimit, ReleaseSettlement,
    ResolveSettlement,
};

/// Top-ups per bounty. Bounds storage and event volume.
const MAX_TOP_UPS: u32 = 10;
const MAX_WINNERS: u32 = 5;
/// Largest id batch accepted by the permissionless `bump` keeper call.
pub const MAX_BUMP_LIMIT: u32 = 50;

#[contract]
pub struct BountyEscrow;

fn require_admin(env: &Env, admin: &Address) -> Result<Config, Error> {
    let c = storage::get_config(env)?;
    if *admin != c.admin {
        return Err(Error::Unauthorized);
    }
    admin.require_auth();
    Ok(c)
}

fn changed(env: &Env, field: Symbol) {
    events::ConfigChanged { field }.publish(env);
}

/// Records a winner credit that could not be paid now. The contract keeps the funds until `claim`.
fn defer(env: &Env, token: &Address, net: i128) -> Result<(), Error> {
    let total = storage::get_claimable_total(env, token);
    storage::set_claimable_total(env, token, add(total, net)?);
    Ok(())
}

/// Removes a credit once it is paid out through `claim`.
fn undefer(env: &Env, token: &Address, net: i128) -> Result<(), Error> {
    let total = storage::get_claimable_total(env, token);
    storage::set_claimable_total(env, token, sub(total, net)?);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReleaseRole {
    Maintainer,
    Relayer,
}

fn authorize_release(caller: &Address, b: &Bounty, cfg: &Config) -> Result<ReleaseRole, Error> {
    // #101: the maintainer check runs first, so if `cfg.relayer` has been
    // set to this same address, that release takes the maintainer path —
    // `relayer_allowed` and the relayer rate limit are not consulted at all.
    // `SelfPayout` and every other release check still apply as normal.
    if *caller == b.maintainer {
        return Ok(ReleaseRole::Maintainer);
    }
    if *caller != cfg.relayer {
        return Err(Error::Unauthorized);
    }
    if !b.relayer_allowed {
        return Err(Error::RelayerNotAllowed);
    }
    Ok(ReleaseRole::Relayer)
}

fn close_bounty(env: &Env, bounty: &mut Bounty, status: Status, winner: Option<Address>) {
    bounty.status = status;
    bounty.winner = winner;
    storage::set_bounty_state(env, bounty);
    storage::clear_active(env, bounty.repo_id, bounty.issue);
}

fn charge_relayer(env: &Env, token: &Address, amount: i128) -> Result<(), Error> {
    let Some(limit) = storage::get_relayer_limit(env, token) else {
        return Ok(());
    };
    let now = env.ledger().timestamp();
    let mut usage = storage::get_relayer_usage(env, token);
    if now >= usage.window_start.saturating_add(limit.window_secs) {
        usage.window_start = now;
        usage.released = 0;
    }
    usage.released = add(usage.released, amount)?;
    if usage.released > limit.cap {
        return Err(Error::RelayerLimitExceeded);
    }
    storage::set_relayer_usage(env, token, &usage);
    Ok(())
}

#[contractimpl]
impl BountyEscrow {
    /// Initializes contract configuration with roles, fee basis points, grace period, and duration limits.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Contract must not already be initialized.
    /// - `fee_bps` must not exceed `MAX_FEE_BPS` (1,000 bps = 10%).
    /// - `min_duration_secs` must not exceed `max_duration_secs`.
    ///
    /// # Errors
    /// - `Error::AlreadyInitialized`: Contract was already initialized.
    /// - `Error::InvalidFee`: `fee_bps` exceeds maximum permitted ceiling (1,000 bps).
    /// - `Error::InvalidDeadline`: `min_duration_secs` is greater than `max_duration_secs`.
    pub fn init(env: Env, params: InitParams) -> Result<(), Error> {
        admin::init(env, params)
    }

    /// Escrows tokens for a GitHub issue bounty and transitions status to `Open`.
    ///
    /// # Caller
    /// Requires authorization from `maintainer`.
    ///
    /// # Checks
    /// - Contract must not be paused.
    /// - `token` must be allowlisted with configured limits.
    /// - `amount` must be between `limits.min_amount` and `limits.max_amount` inclusive.
    /// - `deadline` must fall within `[now + min_duration_secs, now + max_duration_secs]`.
    /// - No active bounty may already exist for the `(repo_id, issue)` pair.
    /// - Snapshots `fee_bps` and `grace_secs` into the bounty record at creation time.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Paused`: Funding is paused by admin.
    /// - `Error::TokenNotAllowed`: Token is not on the allowlist.
    /// - `Error::AmountTooSmall`: Amount is below minimum token threshold.
    /// - `Error::AmountTooLarge`: Amount exceeds maximum token threshold.
    /// - `Error::InvalidDeadline`: Deadline is outside allowed min/max duration window.
    /// - `Error::IssueAlreadyFunded`: An active bounty already exists for this repo and issue.
    /// - `Error::Overflow`: Arithmetic overflow calculating timestamp windows or token totals.
    pub fn fund_bounty(env: Env, params: FundBountyParams) -> Result<u64, Error> {
        bounty::fund_bounty(env, params)
    }

    /// Releases an open bounty to a single recipient (`winner`), deducting the snapshotted platform fee.
    ///
    /// # Caller
    /// Requires authorization from `caller`. Must be either the `maintainer`, or the configured
    /// `relayer` if `relayer_allowed` was enabled at funding.
    ///
    /// # Checks
    /// - Bounty status must be `Status::Open`.
    /// - If called by relayer: bounty must have `relayer_allowed == true` and transfer must not exceed rolling relayer rate cap.
    /// - `winner` cannot be the maintainer or the contract address (`SelfPayout`).
    /// - If the token transfer to `winner` fails (e.g. no trustline), payout is deferred to `claim` instead of reverting.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty with `bounty_id` does not exist.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Open`.
    /// - `Error::Unauthorized`: Caller is not maintainer or authorized relayer.
    /// - `Error::RelayerNotAllowed`: Called by relayer, but maintainer opted out of relayer release.
    /// - `Error::RelayerLimitExceeded`: Relayer release amount exceeds rolling per-token quota.
    /// - `Error::SelfPayout`: Winner is the maintainer or the contract itself.
    /// - `Error::Overflow`: Arithmetic overflow calculating fee, net, or balance totals.
    pub fn release(
        env: Env,
        caller: Address,
        bounty_id: u64,
        winner: Address,
    ) -> Result<ReleaseSettlement, Error> {
        bounty::release(env, caller, bounty_id, winner)
    }

    /// Releases an open bounty divided among multiple winners according to basis-point shares.
    ///
    /// # Caller
    /// Requires authorization from `caller`. Must be either the `maintainer`, or the configured
    /// `relayer` if `relayer_allowed` was enabled at funding.
    ///
    /// # Checks
    /// - Bounty status must be `Status::Open`.
    /// - `winners` must contain between 1 and 5 unique winner addresses.
    /// - No winner address may be the maintainer or the contract address (`SelfPayout`).
    /// - Winner shares must sum to exactly 10,000 bps (100%).
    /// - If called by relayer: checks relayer authorization and rolling rate cap.
    /// - Integer rounding dust from division is allocated to the first winner.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::InvalidStatus`: Bounty is not `Status::Open`.
    /// - `Error::InvalidWinners`: `winners` list is empty, exceeds 5 entries, or has duplicates.
    /// - `Error::Unauthorized`: Caller is neither the maintainer nor authorized relayer.
    /// - `Error::RelayerNotAllowed`: Called by relayer on a bounty without relayer permissions.
    /// - `Error::RelayerLimitExceeded`: Relayer release volume exceeds rolling cap.
    /// - `Error::SelfPayout`: Any winner is the maintainer or the contract address.
    /// - `Error::InvalidSplit`: Total basis points do not equal 10,000.
    /// - `Error::Overflow`: Arithmetic overflow calculating payouts or totals.
    pub fn release_split(
        env: Env,
        caller: Address,
        bounty_id: u64,
        winners: Vec<WinnerShare>,
    ) -> Result<(), Error> {
        bounty::release_split(env, caller, bounty_id, winners)
    }

    /// Refunds the entire bounty amount back to the maintainer without fees after the deadline and grace period have elapsed.
    ///
    /// # Caller
    /// Requires authorization from `maintainer`.
    ///
    /// # Checks
    /// - Caller must be the original bounty maintainer.
    /// - Bounty status must be `Status::Open`.
    /// - Current ledger timestamp must be `>= deadline + grace_secs` (using the grace period snapshotted at funding).
    /// - Operates even when the contract is paused.
    ///
    /// # Errors
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is not the bounty maintainer.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Open`.
    /// - `Error::DeadlineNotReached`: Current time is before `deadline + grace_secs`.
    /// - `Error::Overflow`: Arithmetic overflow on timestamp addition or accounting totals.
    pub fn refund(
        env: Env,
        maintainer: Address,
        bounty_id: u64,
    ) -> Result<RefundSettlement, Error> {
        bounty::refund(env, maintainer, bounty_id)
    }

    /// Returns the exact ledger timestamp at which a bounty becomes eligible for `refund`.
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// - Bounty must exist in storage.
    /// - Uses the `grace_secs` snapshotted into the bounty at funding time.
    ///
    /// # Errors
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Overflow`: Arithmetic overflow calculating `deadline + grace_secs`.
    pub fn refundable_at(env: Env, bounty_id: u64) -> Result<u64, Error> {
        bounty::refundable_at(env, bounty_id)
    }

    /// Freezes release and refund operations on an open bounty, transitioning status to `Disputed`.
    ///
    /// # Caller
    /// Requires authorization from `caller`. Must be either the `maintainer` or the configured `arbiter`.
    ///
    /// # Checks
    /// - Bounty status must be `Status::Open`.
    /// - Bounty must not have been previously disputed (one dispute permitted per bounty lifecycle).
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is neither the maintainer nor the configured arbiter.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Open`.
    /// - `Error::DisputeAlreadyUsed`: Bounty was already disputed previously.
    pub fn open_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        bounty::open_dispute(env, caller, bounty_id)
    }

    /// Resolves an active dispute by allocating funds between a winner and the maintainer.
    ///
    /// # Caller
    /// Requires authorization from `arbiter`.
    ///
    /// # Checks
    /// - Caller must match the configured arbiter address.
    /// - Bounty status must be `Status::Disputed`.
    /// - `winner_bps` must be `<= 10_000` (100%).
    /// - If `winner_bps > 0`, a `winner` address is required and cannot be maintainer, contract, arbiter, or treasury.
    /// - Platform fee is deducted from the winner's portion; remainder reverts to maintainer.
    /// - If winner token transfer fails, net portion is deferred to `claim`.
    /// - Operates even when the contract is paused.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is not the configured arbiter.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Disputed`.
    /// - `Error::InvalidSplit`: `winner_bps > 10_000` or `winner_bps > 0` with `None` winner.
    /// - `Error::SelfPayout`: Winner is the maintainer or contract address.
    /// - `Error::ConflictOfInterest`: Arbiter designated itself or the treasury as winner.
    /// - `Error::Overflow`: Arithmetic overflow calculating portions, fees, or totals.
    pub fn resolve(
        env: Env,
        arbiter: Address,
        bounty_id: u64,
        winner: Option<Address>,
        winner_bps: u32,
    ) -> Result<ResolveSettlement, Error> {
        bounty::resolve(env, arbiter, bounty_id, winner, winner_bps)
    }

    /// Resolves an active dispute identical to `resolve`, while anchoring a 32-byte evidence bundle hash on-chain.
    ///
    /// # Caller
    /// Requires authorization from `arbiter`.
    ///
    /// # Checks
    /// - Same authorization, status, split, and conflict-of-interest checks as `resolve`.
    /// - Emits an `evidence` event carrying `evidence_hash`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is not the configured arbiter.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Disputed`.
    /// - `Error::InvalidSplit`: `winner_bps > 10_000` or `winner_bps > 0` with `None` winner.
    /// - `Error::SelfPayout`: Winner is the maintainer or contract address.
    /// - `Error::ConflictOfInterest`: Arbiter designated itself or the treasury as winner.
    /// - `Error::Overflow`: Arithmetic overflow calculating portions, fees, or totals.
    pub fn resolve_with_evidence(
        env: Env,
        arbiter: Address,
        bounty_id: u64,
        winner: Option<Address>,
        winner_bps: u32,
        evidence_hash: BytesN<32>,
    ) -> Result<ResolveSettlement, Error> {
        bounty::resolve_with_evidence(env, arbiter, bounty_id, winner, winner_bps, evidence_hash)
    }

    /// Extends the deadline of an open bounty to a later timestamp.
    ///
    /// # Caller
    /// Requires authorization from `maintainer`.
    ///
    /// # Checks
    /// - Caller must be the bounty maintainer.
    /// - Bounty status must be `Status::Open`.
    /// - `new_deadline` must be strictly greater than current `deadline` (deadlines can only be extended).
    /// - `new_deadline` must not exceed `now + max_duration_secs`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is not the maintainer.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Open`.
    /// - `Error::InvalidDeadline`: `new_deadline <= deadline` or exceeds maximum duration from current time.
    /// - `Error::Overflow`: Arithmetic overflow adding max duration to current timestamp.
    pub fn extend_deadline(
        env: Env,
        maintainer: Address,
        bounty_id: u64,
        new_deadline: u64,
    ) -> Result<(), Error> {
        bounty::extend_deadline(env, maintainer, bounty_id, new_deadline)
    }

    /// Adds additional tokens to an existing open bounty while preserving its initial snapshotted fee.
    ///
    /// # Caller
    /// Requires authorization from `maintainer`.
    ///
    /// # Checks
    /// - Contract must not be paused.
    /// - Caller must be the bounty maintainer.
    /// - Bounty status must be `Status::Open`.
    /// - `extra` must be strictly greater than 0.
    /// - Token must still be on the allowlist.
    /// - Top-up count must be `< MAX_TOP_UPS` (maximum 10 per bounty).
    /// - New total amount (`amount + extra`) must not exceed `limits.max_amount`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Paused`: Funding and top-up operations are paused.
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::Unauthorized`: Caller is not the bounty maintainer.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Open`.
    /// - `Error::AmountTooSmall`: `extra <= 0`.
    /// - `Error::TokenNotAllowed`: Token has been removed from the allowlist.
    /// - `Error::TopUpLimitReached`: Bounty has reached maximum of 10 top-ups.
    /// - `Error::AmountTooLarge`: Cumulative amount exceeds token's maximum deposit limit.
    /// - `Error::Overflow`: Arithmetic overflow adding funds or updating totals.
    pub fn top_up(env: Env, maintainer: Address, bounty_id: u64, extra: i128) -> Result<(), Error> {
        bounty::top_up(env, maintainer, bounty_id, extra)
    }

    /// Returns a timed-out disputed bounty back to `Status::Open` without deciding a winner.
    ///
    /// # Caller
    /// Callable by anyone; requires caller authorization (`caller.require_auth()`).
    ///
    /// # Checks
    /// - Bounty status must be `Status::Disputed`.
    /// - Dispute record must exist.
    /// - Current ledger timestamp must be `>= opened_at + dispute_timeout`.
    ///
    /// # Errors
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::InvalidStatus`: Bounty status is not `Status::Disputed` or dispute record missing.
    /// - `Error::DisputeNotExpired`: Current timestamp is before dispute timeout expiration.
    /// - `Error::Overflow`: Arithmetic overflow calculating timeout deadline.
    pub fn expire_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        bounty::expire_dispute(env, caller, bounty_id)
    }

    /// Pays out a deferred net amount to a winner whose transfer previously failed (e.g. missing trustline).
    ///
    /// # Caller
    /// Requires authorization from `winner`.
    ///
    /// # Checks
    /// - A deferred claim record must exist for `bounty_id`.
    /// - Claim must not have been previously collected (`claimed == false`).
    /// - Caller must match the recorded `winner` address.
    ///
    /// # Errors
    /// - `Error::BountyNotFound`: Bounty does not exist.
    /// - `Error::NothingToClaim`: No deferred claim record found for this bounty.
    /// - `Error::AlreadyClaimed`: Claim was already collected.
    /// - `Error::Unauthorized`: Caller does not match the recorded claim recipient.
    /// - `Error::Overflow`: Arithmetic overflow updating totals.
    pub fn claim(env: Env, winner: Address, bounty_id: u64) -> Result<(), Error> {
        bounty::claim(env, winner, bounty_id)
    }

    /// Retrieves full details for a bounty by its identifier.
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// - `Error::BountyNotFound`: Bounty does not exist in storage.
    pub fn get_bounty(env: Env, id: u64) -> Result<Bounty, Error> {
        bounty::get_bounty(env, id)
    }

    /// Returns the active bounty for an issue without extending any storage TTL.
    pub fn get_bounty_by_issue(env: Env, repo_id: u64, issue: u64) -> Option<Bounty> {
        bounty::get_bounty_by_issue(env, repo_id, issue)
    }

    /// Queries the active bounty identifier for a GitHub repository and issue pair, if one is open.
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// Returns `None` if no active bounty is open for the specified issue; never reverts.
    pub fn active_bounty(env: Env, repo_id: u64, issue: u64) -> Option<u64> {
        bounty::active_bounty(env, repo_id, issue)
    }

    /// Returns the global contract configuration parameters and role addresses.
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not yet been initialized.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        bounty::get_config(env)
    }

    /// Returns the cumulative token accounting metrics (funded, released_net, fees, refunded, held).
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// Returns zeroed totals if no operations have occurred for the token; never reverts.
    pub fn get_totals(env: Env, token: Address) -> Totals {
        bounty::get_totals(env, token)
    }

    /// Returns allowlist limits (minimum and maximum deposit bounds) for a token address.
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// Returns `None` if the token is not allowlisted; never reverts.
    pub fn get_token(env: Env, token: Address) -> Option<TokenLimits> {
        bounty::get_token(env, token)
    }

    // ---- admin ----

    /// Adds or updates deposit limits for an accepted token contract address.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `min` must be strictly positive (`min > 0`).
    /// - `max` must be greater than or equal to `min`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::AmountTooSmall`: `min <= 0`.
    /// - `Error::AmountTooLarge`: `max < min`.
    pub fn set_token(
        env: Env,
        admin: Address,
        token: Address,
        min: i128,
        max: i128,
        fee_bps: Option<u32>,
    ) -> Result<(), Error> {
        admin::set_token(env, admin, token, min, max, fee_bps)
    }

    /// Removes a token contract address from the allowlist, preventing new funding and top-ups.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `token` must currently exist in the allowlist.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::TokenNotAllowed`: Token is not currently on the allowlist.
    pub fn remove_token(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        admin::remove_token(env, admin, token)
    }

    /// Updates the global platform fee in basis points applied to future bounties.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `fee_bps` must not exceed `MAX_FEE_BPS` (1,000 bps = 10%).
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::InvalidFee`: `fee_bps > 1000`.
    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        admin::set_fee(env, admin, fee_bps)
    }

    /// Updates the authorized relayer address for automated bounty releases.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn set_relayer(env: Env, admin: Address, relayer: Address) -> Result<(), Error> {
        admin::set_relayer(env, admin, relayer)
    }

    /// Updates the authorized arbiter address for opening and resolving disputes.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn set_arbiter(env: Env, admin: Address, arbiter: Address) -> Result<(), Error> {
        admin::set_arbiter(env, admin, arbiter)
    }

    /// Updates the platform treasury address receiving deducted release fees.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), Error> {
        admin::set_treasury(env, admin, treasury)
    }

    /// Updates the default grace period (in seconds) applied to newly funded bounties before refund.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn set_grace(env: Env, admin: Address, grace_secs: u64) -> Result<(), Error> {
        admin::set_grace(env, admin, grace_secs)
    }

    /// Updates the minimum and maximum allowed bounty duration bounds (in seconds).
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `min` must be less than or equal to `max`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::InvalidDeadline`: `min > max`.
    pub fn set_durations(env: Env, admin: Address, min: u64, max: u64) -> Result<(), Error> {
        admin::set_durations(env, admin, min, max)
    }

    /// Transfers admin authority using two-party authentication to prevent lockout from typos.
    ///
    /// # Caller
    /// Requires dual authorization: both the current `admin` and incoming `new_admin` must sign.
    ///
    /// # Checks
    /// - Current admin authorization must match the stored admin.
    /// - Incoming admin authorization is required via `new_admin.require_auth()`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Calling address is not current admin.
    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
        admin::set_admin(env, admin, new_admin)
    }

    /// Pauses new bounty funding and top-ups; releases, refunds, and dispute resolutions remain active.
    ///
    /// Idempotent: calling `pause` while already paused succeeds and emits `Paused`.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn pause(env: Env, admin: Address) -> Result<(), Error> {
        admin::pause(env, admin)
    }

    /// Resumes normal funding and top-up operations after being paused.
    ///
    /// Idempotent: calling `unpause` while already unpaused succeeds and emits `Unpaused`.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    pub fn unpause(env: Env, admin: Address) -> Result<(), Error> {
        admin::unpause(env, admin)
    }

    /// Configures the timeout period (in seconds) after which an unresolved dispute may be expired.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `secs` must be strictly positive (`secs > 0`).
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::InvalidDeadline`: `secs == 0`.
    pub fn set_dispute_timeout(env: Env, admin: Address, secs: u64) -> Result<(), Error> {
        admin::set_dispute_timeout(env, admin, secs)
    }

    /// Returns the dispute timeout window in seconds (defaults to 14 days if unset).
    ///
    /// # Caller
    /// Read-only query; requires no authentication.
    ///
    /// # Checks
    /// None.
    ///
    /// # Errors
    /// None; never reverts.
    pub fn get_dispute_timeout(env: Env) -> u64 {
        bounty::get_dispute_timeout(env)
    }

    /// Configures a rolling rate-limit window and maximum release cap for automated relayer payouts of a token.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `window_secs` must be strictly positive (`window_secs > 0`).
    /// - `cap` must be strictly positive (`cap > 0`).
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::InvalidDeadline`: `window_secs == 0` or `cap <= 0`.
    pub fn set_relayer_limit(
        env: Env,
        admin: Address,
        token: Address,
        window_secs: u64,
        cap: i128,
    ) -> Result<(), Error> {
        admin::set_relayer_limit(env, admin, token, window_secs, cap)
    }

    /// Updates the timelock delay required before proposed contract bytecode upgrades can execute.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - `delay_secs` must be `>= current_delay` (timelock delay can only increase, never decrease).
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::UpgradeDelayCannotDecrease`: `delay_secs` is smaller than the current delay.
    pub fn set_upgrade_delay(env: Env, admin: Address, delay_secs: u64) -> Result<(), Error> {
        admin::set_upgrade_delay(env, admin, delay_secs)
    }

    /// Proposes a new Wasm bytecode hash for contract upgrade subject to the timelock delay.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - Computes execution ETA as `now + upgrade_delay`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::Overflow`: Arithmetic overflow calculating timelock ETA.
    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) -> Result<(), Error> {
        admin::propose_upgrade(env, admin, wasm_hash)
    }

    /// Applies a pending contract bytecode upgrade after the timelock delay has elapsed.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - A pending upgrade proposal must exist.
    /// - Current ledger timestamp must be `>= pending.eta`.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::NoPendingUpgrade`: No upgrade is currently proposed.
    /// - `Error::UpgradeNotReady`: Timelock delay has not yet expired.
    pub fn execute_upgrade(env: Env, admin: Address) -> Result<(), Error> {
        admin::execute_upgrade(env, admin)
    }

    /// Cancels a pending contract upgrade proposal.
    ///
    /// # Caller
    /// Requires authorization from `admin`.
    ///
    /// # Checks
    /// - Caller must be the configured contract admin.
    /// - A pending upgrade proposal must exist.
    ///
    /// # Errors
    /// - `Error::NotInitialized`: Contract has not been initialized.
    /// - `Error::Unauthorized`: Caller is not the admin.
    /// - `Error::NoPendingUpgrade`: No upgrade is currently proposed.
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), Error> {
        admin::cancel_upgrade(env, admin)
    }

    /// Number of bounties ever created.
    pub fn bounty_count(env: Env) -> u64 {
        bounty::bounty_count(env)
    }

    /// Up to `limit` bounties starting at id `start` (capped at storage::MAX_BOUNTIES_LIMIT).
    pub fn get_bounties(env: Env, start: u64, limit: u32) -> Vec<Bounty> {
        bounty::get_bounties(env, start, limit)
    }

    /// Moves the accrued protocol fees of `token` to the treasury. Callable by the
    /// treasury or the admin. Returns the amount transferred.
    pub fn withdraw_fees(env: Env, caller: Address, token: Address) -> Result<i128, Error> {
        admin::withdraw_fees(env, caller, token)
    }

    /// Read-only fee preview for release using the bounty's snapshotted fee_bps.
    pub fn quote_release(env: Env, bounty_id: u64) -> Result<(i128, i128), Error> {
        bounty::quote_release(env, bounty_id)
    }

    /// Read-only fee and split preview for resolve using the bounty's snapshotted fee_bps.
    pub fn quote_resolve(
        env: Env,
        bounty_id: u64,
        winner_bps: u32,
    ) -> Result<(i128, i128, i128), Error> {
        bounty::quote_resolve(env, bounty_id, winner_bps)
    }

    /// Read-only fee preview for pre-funding calculations.
    pub fn quote_fee(_env: Env, amount: i128, fee_bps: u32) -> Result<(i128, i128), Error> {
        bounty::quote_fee(_env, amount, fee_bps)
    }

    /// Permissionless keeper bump that extends TTLs of non-terminal bounties and their active locks/totals.
    pub fn bump(env: Env, bounty_ids: Vec<u64>) -> Result<(), Error> {
        bounty::bump(env, bounty_ids)
    }

    /// Direct, admin-only upgrade (no timelock). Prefer propose_upgrade/execute_upgrade.
    pub fn upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) -> Result<(), Error> {
        admin::upgrade(env, admin, wasm_hash)
    }
}

/// Shared body of `resolve` and `resolve_with_evidence`.
fn settle(
    env: &Env,
    arbiter: Address,
    bounty_id: u64,
    winner: Option<Address>,
    winner_bps: u32,
    evidence: Option<BytesN<32>>,
) -> Result<ResolveSettlement, Error> {
    arbiter.require_auth();
    let cfg = storage::get_config(env)?;
    if arbiter != cfg.arbiter {
        return Err(Error::Unauthorized);
    }
    let mut b = storage::get_bounty(env, bounty_id).ok_or(Error::BountyNotFound)?;
    if b.status != Status::Disputed {
        return Err(Error::InvalidStatus);
    }
    if winner_bps > 10_000 {
        return Err(Error::InvalidSplit);
    }
    let portion = fees::portion(b.amount, winner_bps)?;
    let mut fee = 0;
    let mut winner_net = 0;
    if winner_bps > 0 {
        match &winner {
            None => return Err(Error::InvalidSplit),
            Some(w) if *w == b.maintainer || *w == env.current_contract_address() => {
                return Err(Error::SelfPayout)
            }
            Some(w) if *w == cfg.arbiter || *w == cfg.treasury => {
                return Err(Error::ConflictOfInterest)
            }
            Some(_) => {}
        }
        fee = fees::fee(portion, b.fee_bps)?;
        winner_net = sub(portion, fee)?;
    }
    let maintainer_amount = sub(b.amount, portion)?;

    let bounty_winner = if winner_bps > 0 { winner.clone() } else { None };
    close_bounty(env, &mut b, Status::Resolved, bounty_winner);

    let tok = token::Client::new(env, &b.token);
    let me = env.current_contract_address();
    let mut winner_deferred = false;
    if let (Some(w), true) = (&winner, winner_net > 0) {
        if !matches!(tok.try_transfer(&me, w, &winner_net), Ok(Ok(()))) {
            winner_deferred = true;
            storage::set_claim(
                env,
                bounty_id,
                &ClaimRecord {
                    winner: w.clone(),
                    net: winner_net,
                    claimed: false,
                },
            );
            defer(env, &b.token, winner_net)?;
        }
    }

    let mut t = storage::get_totals(env, &b.token);
    t.record_resolution(
        b.amount,
        winner_net,
        fee,
        maintainer_amount,
        !winner_deferred,
    )?;
    storage::set_totals(env, &b.token, &t);
    if maintainer_amount > 0 {
        tok.transfer(&me, &b.maintainer, &maintainer_amount);
    }
    events::Resolved {
        id: bounty_id,
        winner: b.winner,
        winner_net,
        fee,
        maintainer_amount,
        winner_deferred,
    }
    .publish(env);
    if let Some(evidence_hash) = evidence {
        events::EvidenceRecorded {
            id: bounty_id,
            arbiter,
            evidence_hash,
        }
        .publish(env);
    }
    Ok(ResolveSettlement {
        winner_net,
        fee,
        maintainer_amount,
        winner_deferred,
    })
}
