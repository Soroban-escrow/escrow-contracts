#![no_std]

mod errors;
mod events;
mod fees;
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
pub use types::{Bounty, Config, DataKey, SplitPayout, Status, TokenLimits, Totals, WinnerShare};
use types::{ClaimRecord, DisputeRecord, PendingUpgrade, RelayerLimit};

/// Top-ups per bounty. Bounds storage and event volume.
const MAX_TOP_UPS: u32 = 10;
const MAX_WINNERS: u32 = 5;

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

fn add(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

fn sub(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_sub(b).ok_or(Error::Overflow)
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

fn authorize_release(env: &Env, caller: &Address, b: &Bounty, cfg: &Config) -> Result<bool, Error> {
    if *caller == b.maintainer {
        return Ok(false);
    }
    if *caller != cfg.relayer {
        return Err(Error::Unauthorized);
    }
    if !b.relayer_allowed {
        return Err(Error::RelayerNotAllowed);
    }
    Ok(true)
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
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        env: Env,
        admin: Address,
        relayer: Address,
        arbiter: Address,
        treasury: Address,
        fee_bps: u32,
        grace_secs: u64,
        min_duration_secs: u64,
        max_duration_secs: u64,
    ) -> Result<(), Error> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(Error::AlreadyInitialized);
        }
        if fee_bps > fees::MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
        if min_duration_secs > max_duration_secs {
            return Err(Error::InvalidDeadline);
        }
        storage::set_config(
            &env,
            &Config {
                admin,
                relayer,
                arbiter,
                treasury,
                fee_bps,
                grace_secs,
                min_duration_secs,
                max_duration_secs,
                paused: false,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fund_bounty(
        env: Env,
        maintainer: Address,
        repo_id: u64,
        issue: u64,
        token: Address,
        amount: i128,
        deadline: u64,
        relayer_allowed: bool,
    ) -> Result<u64, Error> {
        maintainer.require_auth();
        let cfg = storage::get_config(&env)?;
        if cfg.paused {
            return Err(Error::Paused);
        }
        let limits = storage::get_token(&env, &token).ok_or(Error::TokenNotAllowed)?;
        if amount < limits.min_amount {
            return Err(Error::AmountTooSmall);
        }
        if amount > limits.max_amount {
            return Err(Error::AmountTooLarge);
        }
        let now = env.ledger().timestamp();
        let earliest = now
            .checked_add(cfg.min_duration_secs)
            .ok_or(Error::Overflow)?;
        let latest = now
            .checked_add(cfg.max_duration_secs)
            .ok_or(Error::Overflow)?;
        if deadline < earliest || deadline > latest {
            return Err(Error::InvalidDeadline);
        }
        if storage::active_bounty(&env, repo_id, issue).is_some() {
            return Err(Error::IssueAlreadyFunded);
        }

        let id = storage::next_id(&env);
        storage::set_bounty(
            &env,
            &Bounty {
                id,
                maintainer: maintainer.clone(),
                repo_id,
                issue,
                token: token.clone(),
                amount,
                fee_bps: cfg.fee_bps,
                deadline,
                relayer_allowed,
                status: Status::Open,
                winner: None,
                created_at: now,
            },
        );
        storage::set_active(&env, repo_id, issue, id);
        let mut t = storage::get_totals(&env, &token);
        t.funded = add(t.funded, amount)?;
        t.held = add(t.held, amount)?;
        storage::set_totals(&env, &token, &t);

        token::Client::new(&env, &token).transfer(
            &maintainer,
            env.current_contract_address(),
            &amount,
        );
        events::Funded {
            id,
            maintainer,
            repo_id,
            issue,
            token,
            amount,
            fee_bps: cfg.fee_bps,
            deadline,
            relayer_allowed,
        }
        .publish(&env);
        Ok(id)
    }

    pub fn release(
        env: Env,
        caller: Address,
        bounty_id: u64,
        winner: Address,
    ) -> Result<(), Error> {
        caller.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if authorize_release(&env, &caller, &b, &cfg)? {
            charge_relayer(&env, &b.token, b.amount)?;
        }
        if winner == b.maintainer || winner == env.current_contract_address() {
            return Err(Error::SelfPayout);
        }
        let fee = fees::fee(b.amount, b.fee_bps)?;
        let net = fees::net(b.amount, b.fee_bps)?;

        b.status = Status::Released;
        b.winner = Some(winner.clone());
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);

        let tok = token::Client::new(&env, &b.token);
        let me = env.current_contract_address();
        // A winner with no trustline must not revert the release. The net stays in the contract as a
        // claim the winner can collect once the trustline exists.
        let paid = matches!(tok.try_transfer(&me, &winner, &net), Ok(Ok(())));
        if !paid {
            storage::set_claim(
                &env,
                bounty_id,
                &ClaimRecord {
                    winner: winner.clone(),
                    net,
                    claimed: false,
                },
            );
            defer(&env, &b.token, net)?;
        }

        let mut t = storage::get_totals(&env, &b.token);
        if paid {
            t.released_net = add(t.released_net, net)?;
        }
        t.fees = add(t.fees, fee)?;
        t.held = sub(t.held, b.amount)?;
        storage::set_totals(&env, &b.token, &t);

        if fee > 0 {
            tok.transfer(&me, &cfg.treasury, &fee);
        }
        events::Released {
            id: bounty_id,
            winner,
            net,
            fee,
            caller,
            deferred: !paid,
        }
        .publish(&env);
        Ok(())
    }

    pub fn release_split(
        env: Env,
        caller: Address,
        bounty_id: u64,
        winners: Vec<WinnerShare>,
    ) -> Result<(), Error> {
        caller.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if winners.is_empty() || winners.len() > MAX_WINNERS {
            return Err(Error::InvalidWinners);
        }
        if authorize_release(&env, &caller, &b, &cfg)? {
            charge_relayer(&env, &b.token, b.amount)?;
        }

        let mut total_bps = 0u32;
        for i in 0..winners.len() {
            let entry = winners.get(i).unwrap();
            if entry.winner == b.maintainer || entry.winner == env.current_contract_address() {
                return Err(Error::SelfPayout);
            }
            total_bps = total_bps
                .checked_add(entry.share_bps)
                .ok_or(Error::Overflow)?;
            for j in 0..i {
                if winners.get(j).unwrap().winner == entry.winner {
                    return Err(Error::InvalidWinners);
                }
            }
        }
        if total_bps != 10_000 {
            return Err(Error::InvalidSplit);
        }

        let fee = fees::fee(b.amount, b.fee_bps)?;
        let net = sub(b.amount, fee)?;
        let mut payouts = Vec::new(&env);
        let mut paid = 0i128;
        for i in 0..winners.len() {
            let entry = winners.get(i).unwrap();
            let amount = if i == 0 {
                0
            } else {
                fees::share(net, entry.share_bps)?
            };
            paid = add(paid, amount)?;
            payouts.push_back(SplitPayout {
                winner: entry.winner,
                amount,
            });
        }
        let first = winners.get(0).unwrap();
        payouts.set(
            0,
            SplitPayout {
                winner: first.winner,
                amount: sub(net, paid)?,
            },
        );

        b.status = Status::Released;
        b.winner = None;
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);
        let tok = token::Client::new(&env, &b.token);
        let me = env.current_contract_address();
        for payout in payouts.iter() {
            tok.transfer(&me, &payout.winner, &payout.amount);
        }
        if fee > 0 {
            tok.transfer(&me, &cfg.treasury, &fee);
        }
        let mut totals = storage::get_totals(&env, &b.token);
        totals.released_net = add(totals.released_net, net)?;
        totals.fees = add(totals.fees, fee)?;
        totals.held = sub(totals.held, b.amount)?;
        storage::set_totals(&env, &b.token, &totals);
        events::ReleasedSplit {
            id: bounty_id,
            payouts,
            fee,
            caller,
        }
        .publish(&env);
        Ok(())
    }

    pub fn refund(env: Env, maintainer: Address, bounty_id: u64) -> Result<(), Error> {
        maintainer.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if maintainer != b.maintainer {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        let opens = b
            .deadline
            .checked_add(cfg.grace_secs)
            .ok_or(Error::Overflow)?;
        if env.ledger().timestamp() < opens {
            return Err(Error::DeadlineNotReached);
        }
        b.status = Status::Refunded;
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);
        let mut t = storage::get_totals(&env, &b.token);
        t.refunded = add(t.refunded, b.amount)?;
        t.held = sub(t.held, b.amount)?;
        storage::set_totals(&env, &b.token, &t);

        token::Client::new(&env, &b.token).transfer(
            &env.current_contract_address(),
            &b.maintainer,
            &b.amount,
        );
        events::Refunded {
            id: bounty_id,
            amount: b.amount,
        }
        .publish(&env);
        Ok(())
    }

    pub fn open_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        caller.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if caller != b.maintainer && caller != cfg.arbiter {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        // One dispute per bounty. An expired dispute returns to Open, so this stops it being reopened in a loop.
        if storage::get_dispute(&env, bounty_id).is_some() {
            return Err(Error::DisputeAlreadyUsed);
        }
        storage::set_dispute(
            &env,
            bounty_id,
            &DisputeRecord {
                opened_at: env.ledger().timestamp(),
            },
        );
        b.status = Status::Disputed;
        storage::set_bounty(&env, &b);
        events::Disputed {
            id: bounty_id,
            caller,
        }
        .publish(&env);
        Ok(())
    }

    /// Splits a disputed bounty between a winner and the maintainer.
    ///
    /// The winner may not be the maintainer, the contract (`SelfPayout`), or the
    /// arbiter or the treasury (`ConflictOfInterest`). The arbiter cannot route
    /// money to itself or to the fee account.
    pub fn resolve(
        env: Env,
        arbiter: Address,
        bounty_id: u64,
        winner: Option<Address>,
        winner_bps: u32,
    ) -> Result<(), Error> {
        settle(&env, arbiter, bounty_id, winner, winner_bps, None)
    }

    /// Same as `resolve`, and also emits `EvidenceRecorded` carrying
    /// `evidence_hash`: a SHA-256 digest of the evidence the arbiter relied on
    /// (see docs/CONTRACT.md, "Resolution evidence"). The `resolved` event is
    /// unchanged, so existing indexers keep working.
    pub fn resolve_with_evidence(
        env: Env,
        arbiter: Address,
        bounty_id: u64,
        winner: Option<Address>,
        winner_bps: u32,
        evidence_hash: BytesN<32>,
    ) -> Result<(), Error> {
        settle(
            &env,
            arbiter,
            bounty_id,
            winner,
            winner_bps,
            Some(evidence_hash),
        )
    }

    /// Lengthens an open bounty. Extend only: the deadline a contributor saw can never move earlier.
    pub fn extend_deadline(
        env: Env,
        maintainer: Address,
        bounty_id: u64,
        new_deadline: u64,
    ) -> Result<(), Error> {
        maintainer.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if maintainer != b.maintainer {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if new_deadline <= b.deadline {
            return Err(Error::InvalidDeadline);
        }
        let latest = env
            .ledger()
            .timestamp()
            .checked_add(cfg.max_duration_secs)
            .ok_or(Error::Overflow)?;
        if new_deadline > latest {
            return Err(Error::InvalidDeadline);
        }
        let old_deadline = b.deadline;
        b.deadline = new_deadline;
        storage::set_bounty(&env, &b);
        events::DeadlineExtended {
            id: bounty_id,
            old_deadline,
            new_deadline,
        }
        .publish(&env);
        Ok(())
    }

    /// Adds funds to an open bounty. The bounty keeps the fee snapshot it was funded with.
    pub fn top_up(env: Env, maintainer: Address, bounty_id: u64, extra: i128) -> Result<(), Error> {
        maintainer.require_auth();
        let cfg = storage::get_config(&env)?;
        if cfg.paused {
            return Err(Error::Paused);
        }
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if maintainer != b.maintainer {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if extra <= 0 {
            return Err(Error::AmountTooSmall);
        }
        // A token removed from the allowlist can no longer be topped up. Release and refund still work.
        let limits = storage::get_token(&env, &b.token).ok_or(Error::TokenNotAllowed)?;
        let uses = storage::get_top_ups(&env, bounty_id);
        if uses >= MAX_TOP_UPS {
            return Err(Error::TopUpLimitReached);
        }
        let new_amount = add(b.amount, extra)?;
        // The per-token cap applies to the total, so a top-up cannot exceed it.
        if new_amount > limits.max_amount {
            return Err(Error::AmountTooLarge);
        }

        b.amount = new_amount;
        storage::set_bounty(&env, &b);
        storage::set_top_ups(&env, bounty_id, uses + 1);
        let mut t = storage::get_totals(&env, &b.token);
        t.funded = add(t.funded, extra)?;
        t.held = add(t.held, extra)?;
        storage::set_totals(&env, &b.token, &t);

        token::Client::new(&env, &b.token).transfer(
            &maintainer,
            env.current_contract_address(),
            &extra,
        );
        events::ToppedUp {
            id: bounty_id,
            extra,
            new_amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns a timed-out dispute to `Open`. Nobody is judged: the normal rules resume, and the bounty
    /// cannot be disputed again. Callable by anyone once the timeout has passed.
    pub fn expire_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        caller.require_auth();
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if b.status != Status::Disputed {
            return Err(Error::InvalidStatus);
        }
        let record = storage::get_dispute(&env, bounty_id).ok_or(Error::InvalidStatus)?;
        let ready = record
            .opened_at
            .checked_add(storage::dispute_timeout(&env))
            .ok_or(Error::Overflow)?;
        if env.ledger().timestamp() < ready {
            return Err(Error::DisputeNotExpired);
        }
        b.status = Status::Open;
        storage::set_bounty(&env, &b);
        events::DisputeExpired { id: bounty_id }.publish(&env);
        Ok(())
    }

    /// Pays a winner credit that was deferred because the transfer failed (for example, no trustline).
    pub fn claim(env: Env, winner: Address, bounty_id: u64) -> Result<(), Error> {
        winner.require_auth();
        let mut record = storage::get_claim(&env, bounty_id).ok_or(Error::NothingToClaim)?;
        if record.claimed {
            return Err(Error::AlreadyClaimed);
        }
        if record.winner != winner {
            return Err(Error::Unauthorized);
        }
        let b = storage::get_bounty(&env, bounty_id)?;
        record.claimed = true;
        storage::set_claim(&env, bounty_id, &record);
        undefer(&env, &b.token, record.net)?;
        let mut t = storage::get_totals(&env, &b.token);
        t.released_net = add(t.released_net, record.net)?;
        storage::set_totals(&env, &b.token, &t);

        token::Client::new(&env, &b.token).transfer(
            &env.current_contract_address(),
            &winner,
            &record.net,
        );
        events::Claimed {
            id: bounty_id,
            winner,
            net: record.net,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_bounty(env: Env, id: u64) -> Result<Bounty, Error> {
        storage::get_bounty(&env, id)
    }

    pub fn active_bounty(env: Env, repo_id: u64, issue: u64) -> Option<u64> {
        storage::active_bounty(&env, repo_id, issue)
    }

    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env)
    }

    pub fn get_totals(env: Env, token: Address) -> Totals {
        storage::get_totals(&env, &token)
    }

    pub fn get_token(env: Env, token: Address) -> Option<TokenLimits> {
        storage::get_token(&env, &token)
    }

    // ---- admin ----

    pub fn set_token(
        env: Env,
        admin: Address,
        token: Address,
        min: i128,
        max: i128,
    ) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if min <= 0 {
            return Err(Error::AmountTooSmall);
        }
        if max < min {
            return Err(Error::AmountTooLarge);
        }
        storage::set_token(
            &env,
            &token,
            &TokenLimits {
                min_amount: min,
                max_amount: max,
            },
        );
        changed(&env, symbol_short!("token"));
        Ok(())
    }

    pub fn remove_token(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if storage::get_token(&env, &token).is_none() {
            return Err(Error::TokenNotAllowed);
        }
        storage::remove_token(&env, &token);
        changed(&env, symbol_short!("token"));
        Ok(())
    }

    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        if fee_bps > fees::MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
        c.fee_bps = fee_bps;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("fee"));
        Ok(())
    }

    pub fn set_relayer(env: Env, admin: Address, relayer: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.relayer = relayer;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("relayer"));
        Ok(())
    }

    pub fn set_arbiter(env: Env, admin: Address, arbiter: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.arbiter = arbiter;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("arbiter"));
        Ok(())
    }

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.treasury = treasury;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("treasury"));
        Ok(())
    }

    pub fn set_grace(env: Env, admin: Address, grace_secs: u64) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.grace_secs = grace_secs;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("grace"));
        Ok(())
    }

    pub fn set_durations(env: Env, admin: Address, min: u64, max: u64) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        if min > max {
            return Err(Error::InvalidDeadline);
        }
        c.min_duration_secs = min;
        c.max_duration_secs = max;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("duration"));
        Ok(())
    }

    /// Two-party auth: the new admin must also sign, to avoid typos.
    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        new_admin.require_auth();
        c.admin = new_admin;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("admin"));
        Ok(())
    }

    /// Blocks `fund_bounty` only; release, refund and resolve keep working.
    pub fn pause(env: Env, admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.paused = true;
        storage::set_config(&env, &c);
        events::Paused {}.publish(&env);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.paused = false;
        storage::set_config(&env, &c);
        events::Unpaused {}.publish(&env);
        Ok(())
    }

    pub fn set_dispute_timeout(env: Env, admin: Address, secs: u64) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if secs == 0 {
            return Err(Error::InvalidDeadline);
        }
        storage::set_dispute_timeout(&env, secs);
        changed(&env, symbol_short!("dispute"));
        Ok(())
    }

    pub fn get_dispute_timeout(env: Env) -> u64 {
        storage::dispute_timeout(&env)
    }

    pub fn set_relayer_limit(
        env: Env,
        admin: Address,
        token: Address,
        window_secs: u64,
        cap: i128,
    ) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if window_secs == 0 || cap <= 0 {
            return Err(Error::InvalidDeadline);
        }
        storage::set_relayer_limit(&env, &token, &RelayerLimit { window_secs, cap });
        changed(&env, symbol_short!("rlimit"));
        Ok(())
    }

    pub fn set_upgrade_delay(env: Env, admin: Address, delay_secs: u64) -> Result<(), Error> {
        let cfg = require_admin(&env, &admin)?;
        let current = storage::upgrade_delay(&env, cfg.max_duration_secs + cfg.grace_secs);
        if delay_secs < current {
            return Err(Error::UpgradeDelayCannotDecrease);
        }
        storage::set_upgrade_delay(&env, delay_secs);
        changed(&env, symbol_short!("upgrade"));
        Ok(())
    }

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) -> Result<(), Error> {
        let cfg = require_admin(&env, &admin)?;
        let eta = env
            .ledger()
            .timestamp()
            .checked_add(storage::upgrade_delay(
                &env,
                cfg.max_duration_secs + cfg.grace_secs,
            ))
            .ok_or(Error::Overflow)?;
        storage::set_pending_upgrade(
            &env,
            &PendingUpgrade {
                wasm_hash: wasm_hash.clone(),
                eta,
            },
        );
        events::UpgradeProposed { wasm_hash, eta }.publish(&env);
        Ok(())
    }

    pub fn execute_upgrade(env: Env, admin: Address) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        let pending = storage::pending_upgrade(&env).ok_or(Error::NoPendingUpgrade)?;
        if env.ledger().timestamp() < pending.eta {
            return Err(Error::UpgradeNotReady);
        }
        storage::clear_pending_upgrade(&env);
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(pending.wasm_hash.clone()));
        events::Upgraded {
            wasm_hash: pending.wasm_hash,
        }
        .publish(&env);
        Ok(())
    }

    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        let pending = storage::pending_upgrade(&env).ok_or(Error::NoPendingUpgrade)?;
        storage::clear_pending_upgrade(&env);
        events::UpgradeCancelled {
            wasm_hash: pending.wasm_hash,
        }
        .publish(&env);
        Ok(())
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
) -> Result<(), Error> {
    arbiter.require_auth();
    let cfg = storage::get_config(env)?;
    if arbiter != cfg.arbiter {
        return Err(Error::Unauthorized);
    }
    let mut b = storage::get_bounty(env, bounty_id)?;
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

    b.status = Status::Resolved;
    b.winner = if winner_bps > 0 { winner.clone() } else { None };
    storage::set_bounty(env, &b);
    storage::clear_active(env, b.repo_id, b.issue);

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
    if winner_net > 0 && !winner_deferred {
        t.released_net = add(t.released_net, winner_net)?;
    }
    t.fees = add(t.fees, fee)?;
    t.refunded = add(t.refunded, maintainer_amount)?;
    t.held = sub(t.held, b.amount)?;
    storage::set_totals(env, &b.token, &t);

    if fee > 0 {
        tok.transfer(&me, &cfg.treasury, &fee);
    }
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
    Ok(())
}
