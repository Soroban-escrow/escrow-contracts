//! Bounty lifecycle (fund, release, refund, disputes) and read-only views.
//!
//! Bodies of the entry points in lib.rs; the contract surface stays in lib.rs.

use super::*;

pub(crate) fn fund_bounty(env: Env, params: FundBountyParams) -> Result<u64, Error> {
    let FundBountyParams {
        maintainer,
        repo_id,
        issue,
        token,
        amount,
        deadline,
        relayer_allowed,
    } = params;
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

    let id = storage::next_id(&env)?;
    storage::set_bounty(
        &env,
        &Bounty {
            id,
            maintainer: maintainer.clone(),
            repo_id,
            issue,
            token: token.clone(),
            amount,
            fee_bps: limits.fee_bps.unwrap_or(cfg.fee_bps),
            grace_secs: cfg.grace_secs,
            deadline,
            relayer_allowed,
            status: Status::Open,
            winner: None,
        },
    );
    storage::set_active(&env, repo_id, issue, id);
    let mut t = storage::get_totals(&env, &token);
    t.record_fund(amount)?;
    storage::set_totals(&env, &token, &t);

    token::Client::new(&env, &token).transfer(&maintainer, env.current_contract_address(), &amount);
    events::Funded {
        id,
        maintainer,
        repo_id,
        issue,
        token,
        amount,
        fee_bps: limits.fee_bps.unwrap_or(cfg.fee_bps),
        deadline,
        relayer_allowed,
    }
    .publish(&env);
    Ok(id)
}

pub(crate) fn release(
    env: Env,
    caller: Address,
    bounty_id: u64,
    winner: Address,
) -> Result<ReleaseSettlement, Error> {
    caller.require_auth();
    let cfg = storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    if b.status != Status::Open {
        return Err(Error::InvalidStatus);
    }
    if authorize_release(&caller, &b, &cfg)? == ReleaseRole::Relayer {
        charge_relayer(&env, &b.token, b.amount)?;
    }
    if winner == b.maintainer || winner == env.current_contract_address() {
        return Err(Error::SelfPayout);
    }
    let fee = fees::fee(b.amount, b.fee_bps)?;
    let net = fees::net(b.amount, b.fee_bps)?;

    close_bounty(&env, &mut b, Status::Released, Some(winner.clone()));

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
    t.record_payout(b.amount, net, fee, paid)?;
    storage::set_totals(&env, &b.token, &t);
    events::Released {
        id: bounty_id,
        winner,
        net,
        fee,
        caller,
        deferred: !paid,
    }
    .publish(&env);
    Ok(ReleaseSettlement {
        net,
        fee,
        deferred: !paid,
    })
}

pub(crate) fn release_split(
    env: Env,
    caller: Address,
    bounty_id: u64,
    winners: Vec<WinnerShare>,
) -> Result<(), Error> {
    caller.require_auth();
    let cfg = storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    if b.status != Status::Open {
        return Err(Error::InvalidStatus);
    }
    if winners.is_empty() || winners.len() > MAX_WINNERS {
        return Err(Error::InvalidWinners);
    }
    if authorize_release(&caller, &b, &cfg)? == ReleaseRole::Relayer {
        charge_relayer(&env, &b.token, b.amount)?;
    }

    let mut total_bps = 0u32;
    for i in 0..winners.len() {
        let entry = winners.get(i).ok_or(Error::InvalidSplit)?;
        if entry.winner == b.maintainer || entry.winner == env.current_contract_address() {
            return Err(Error::SelfPayout);
        }
        total_bps = total_bps
            .checked_add(entry.share_bps)
            .ok_or(Error::Overflow)?;
        for j in 0..i {
            if winners.get(j).ok_or(Error::InvalidSplit)?.winner == entry.winner {
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
        let entry = winners.get(i).ok_or(Error::InvalidSplit)?;
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
    let first = winners.get(0).ok_or(Error::InvalidSplit)?;
    payouts.set(
        0,
        SplitPayout {
            winner: first.winner,
            amount: sub(net, paid)?,
        },
    );

    close_bounty(&env, &mut b, Status::Released, None);
    let tok = token::Client::new(&env, &b.token);
    let me = env.current_contract_address();
    for payout in payouts.iter() {
        tok.transfer(&me, &payout.winner, &payout.amount);
    }
    let mut totals = storage::get_totals(&env, &b.token);
    totals.record_payout(b.amount, net, fee, true)?;
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

pub(crate) fn refund(
    env: Env,
    maintainer: Address,
    bounty_id: u64,
) -> Result<RefundSettlement, Error> {
    maintainer.require_auth();
    storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    if maintainer != b.maintainer {
        return Err(Error::Unauthorized);
    }
    if b.status != Status::Open {
        return Err(Error::InvalidStatus);
    }
    // #163: use the grace snapshotted at funding time, not the live config value.
    let opens = b
        .deadline
        .checked_add(b.grace_secs)
        .ok_or(Error::Overflow)?;
    if env.ledger().timestamp() < opens {
        return Err(Error::DeadlineNotReached);
    }
    close_bounty(&env, &mut b, Status::Refunded, None);
    let mut t = storage::get_totals(&env, &b.token);
    t.record_refund(b.amount)?;
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
    Ok(RefundSettlement { amount: b.amount })
}

pub(crate) fn refundable_at(env: Env, bounty_id: u64) -> Result<u64, Error> {
    let bounty = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    // #163: use the grace snapshotted at funding time.
    bounty
        .deadline
        .checked_add(bounty.grace_secs)
        .ok_or(Error::Overflow)
}

pub(crate) fn open_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
    caller.require_auth();
    let cfg = storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
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
    storage::set_bounty_state(&env, &b);
    events::Disputed {
        id: bounty_id,
        caller,
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn resolve(
    env: Env,
    arbiter: Address,
    bounty_id: u64,
    winner: Option<Address>,
    winner_bps: u32,
) -> Result<ResolveSettlement, Error> {
    settle(&env, arbiter, bounty_id, winner, winner_bps, None)
}

pub(crate) fn resolve_with_evidence(
    env: Env,
    arbiter: Address,
    bounty_id: u64,
    winner: Option<Address>,
    winner_bps: u32,
    evidence_hash: BytesN<32>,
) -> Result<ResolveSettlement, Error> {
    settle(
        &env,
        arbiter,
        bounty_id,
        winner,
        winner_bps,
        Some(evidence_hash),
    )
}

pub(crate) fn extend_deadline(
    env: Env,
    maintainer: Address,
    bounty_id: u64,
    new_deadline: u64,
) -> Result<(), Error> {
    maintainer.require_auth();
    let cfg = storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
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

pub(crate) fn top_up(
    env: Env,
    maintainer: Address,
    bounty_id: u64,
    extra: i128,
) -> Result<(), Error> {
    maintainer.require_auth();
    let cfg = storage::get_config(&env)?;
    if cfg.paused {
        return Err(Error::Paused);
    }
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
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
    storage::set_top_ups(&env, bounty_id, uses.checked_add(1).ok_or(Error::Overflow)?);
    let mut t = storage::get_totals(&env, &b.token);
    t.record_fund(extra)?;
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

pub(crate) fn expire_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
    caller.require_auth();
    storage::get_config(&env)?;
    let mut b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
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
    storage::set_bounty_state(&env, &b);
    events::DisputeExpired { id: bounty_id }.publish(&env);
    Ok(())
}

pub(crate) fn claim(env: Env, winner: Address, bounty_id: u64) -> Result<(), Error> {
    winner.require_auth();
    storage::get_config(&env)?;
    let mut record = storage::get_claim(&env, bounty_id).ok_or(Error::NothingToClaim)?;
    if record.claimed {
        return Err(Error::AlreadyClaimed);
    }
    if record.winner != winner {
        return Err(Error::Unauthorized);
    }
    let b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    record.claimed = true;
    storage::set_claim(&env, bounty_id, &record);
    undefer(&env, &b.token, record.net)?;
    let mut t = storage::get_totals(&env, &b.token);
    t.record_claim(record.net)?;
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

pub(crate) fn get_bounty(env: Env, id: u64) -> Result<Bounty, Error> {
    storage::get_bounty(&env, id).ok_or(Error::BountyNotFound)
}

/// Returns the active bounty for an issue without extending any storage TTL.
pub(crate) fn get_bounty_by_issue(env: Env, repo_id: u64, issue: u64) -> Option<Bounty> {
    storage::get_bounty_by_issue(&env, repo_id, issue)
}

pub(crate) fn active_bounty(env: Env, repo_id: u64, issue: u64) -> Option<u64> {
    storage::active_bounty(&env, repo_id, issue)
}

pub(crate) fn get_config(env: Env) -> Result<Config, Error> {
    storage::get_config(&env)
}

pub(crate) fn get_totals(env: Env, token: Address) -> Totals {
    storage::get_totals(&env, &token)
}

pub(crate) fn get_token(env: Env, token: Address) -> Option<TokenLimits> {
    storage::get_token(&env, &token)
}

pub(crate) fn get_dispute_timeout(env: Env) -> u64 {
    storage::dispute_timeout(&env)
}

pub(crate) fn bounty_count(env: Env) -> u64 {
    storage::bounty_count(&env)
}

pub(crate) fn get_bounties(env: Env, start: u64, limit: u32) -> Vec<Bounty> {
    storage::get_bounties(&env, start, limit)
}

pub(crate) fn quote_release(env: Env, bounty_id: u64) -> Result<(i128, i128), Error> {
    let b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    let fee = fees::fee(b.amount, b.fee_bps)?;
    let net = fees::net(b.amount, b.fee_bps)?;
    Ok((net, fee))
}

pub(crate) fn quote_resolve(
    env: Env,
    bounty_id: u64,
    winner_bps: u32,
) -> Result<(i128, i128, i128), Error> {
    let b = storage::get_bounty(&env, bounty_id).ok_or(Error::BountyNotFound)?;
    if winner_bps > 10_000 {
        return Err(Error::InvalidSplit);
    }
    let portion = fees::portion(b.amount, winner_bps)?;
    let mut fee = 0;
    let mut winner_net = 0;
    if winner_bps > 0 {
        fee = fees::fee(portion, b.fee_bps)?;
        winner_net = sub(portion, fee)?;
    }
    let maintainer_amount = sub(b.amount, portion)?;
    Ok((winner_net, fee, maintainer_amount))
}

pub(crate) fn quote_fee(_env: Env, amount: i128, fee_bps: u32) -> Result<(i128, i128), Error> {
    if amount < 0 {
        return Err(Error::AmountTooSmall);
    }
    if fee_bps > fees::MAX_FEE_BPS {
        return Err(Error::InvalidFee);
    }
    let fee = fees::fee(amount, fee_bps)?;
    let net = fees::net(amount, fee_bps)?;
    Ok((net, fee))
}

pub(crate) fn bump(env: Env, bounty_ids: Vec<u64>) -> Result<(), Error> {
    storage::get_config(&env)?;
    if bounty_ids.len() > MAX_BUMP_LIMIT {
        return Err(Error::AmountTooLarge);
    }
    for id in bounty_ids.iter() {
        if let Some(b) = storage::peek_bounty(&env, id) {
            if !b.status.is_terminal() {
                storage::bump_bounty_max(&env, id);
                let active_key = DataKey::ActiveByIssue(b.repo_id, b.issue);
                if env.storage().persistent().has(&active_key) {
                    storage::bump_persistent_max(&env, &active_key);
                }
                let totals_key = DataKey::Totals(b.token.clone());
                if env.storage().persistent().has(&totals_key) {
                    storage::bump_persistent_max(&env, &totals_key);
                }
            }
        }
    }
    storage::bump_instance(&env);
    Ok(())
}
