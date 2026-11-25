use soroban_sdk::{Address, Env, IntoVal, TryFromVal, Val, Vec};

use crate::errors::Error;
use crate::types::{
    Bounty, BountyState, BountyTerms, ClaimRecord, Config, DataKey, DisputeRecord, PendingUpgrade,
    RelayerLimit, RelayerUsage, TokenLimits, Totals,
};

// Ledgers close about every 5 s: 17_280 per day. Testnet max entry TTL is
// 3_110_400 ledgers (180 d). Worst case a bounty must live is
// max_duration (90 d) + grace (3 d) = 93 d; we extend to 120 d whenever the
// remaining life drops under 30 d.
pub const DAY_LEDGERS: u32 = 17_280;
pub const TTL_THRESHOLD: u32 = 30 * DAY_LEDGERS;
pub const TTL_EXTEND_TO: u32 = 120 * DAY_LEDGERS;
const _: () = assert!(TTL_EXTEND_TO - TTL_THRESHOLD >= 90 * DAY_LEDGERS);
const _: () = assert!(TTL_EXTEND_TO <= 3_110_400);
pub(crate) const MAX_BOUNTIES_LIMIT: u32 = 256;

/// Up to `limit` bounties starting at id `start`, capped at MAX_BOUNTIES_LIMIT.
/// Open and disputed bounties get their TTL extended as they are read.
pub fn get_bounties(env: &Env, start: u64, limit: u32) -> Vec<Bounty> {
    let capped_limit = limit.min(MAX_BOUNTIES_LIMIT);
    let mut list = Vec::new(env);
    if capped_limit == 0 {
        return list;
    }
    let max_id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    let mut id = start;
    while id < max_id && list.len() < capped_limit {
        if let Some(b) = peek_bounty(env, id) {
            if !b.status.is_terminal() {
                bump_bounty(env, id);
            }
            list.push_back(b);
        }
        id = match id.checked_add(1) {
            Some(next) => next,
            None => break,
        };
    }
    list
}

/// Number of bounties ever created (ids are 1..=count).
pub fn bounty_count(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextId)
        .unwrap_or(1u64)
        .saturating_sub(1)
}

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

/// Loads the global contract configuration and extends instance TTL, or returns
/// `Error::NotInitialized` if `init` has not yet been called.
pub fn get_config(env: &Env) -> Result<Config, Error> {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(Error::NotInitialized)
}

pub fn set_config(env: &Env, c: &Config) {
    env.storage().instance().set(&DataKey::Config, c);
    bump_instance(env);
}

pub fn next_id(env: &Env) -> Result<u64, Error> {
    let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    let next = id.checked_add(1).ok_or(Error::Overflow)?;
    env.storage().instance().set(&DataKey::NextId, &next);
    Ok(id)
}

pub fn get_token(env: &Env, token: &Address) -> Option<TokenLimits> {
    let key = DataKey::Token(token.clone());
    if let Some(value) = env.storage().persistent().get(&key) {
        bump_persistent(env, &key);
        return Some(value);
    }
    env.storage().instance().get(&key)
}

pub fn set_token(env: &Env, token: &Address, l: &TokenLimits) {
    let key = DataKey::Token(token.clone());
    env.storage().persistent().set(&key, l);
    bump_persistent(env, &key);
    env.storage().instance().remove(&key);
}

pub fn remove_token(env: &Env, token: &Address) {
    let key = DataKey::Token(token.clone());
    env.storage().persistent().remove(&key);
    env.storage().instance().remove(&key);
}

pub fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Extends a persistent entry to the maximum TTL regardless of its current remaining life.
pub fn bump_persistent_max(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_EXTEND_TO, TTL_EXTEND_TO);
}

/// Typed persistent lookup with optional TTL refresh. A missing key is never
/// materialized; callers retain their existing default/error behavior.
fn read_persistent<T: TryFromVal<Env, Val>>(
    env: &Env,
    key: &DataKey,
    refresh_ttl: bool,
) -> Option<T> {
    let value = env.storage().persistent().get(key);
    if refresh_ttl && value.is_some() {
        bump_persistent(env, key);
    }
    value
}

/// Typed persistent write followed by the existing TTL policy.
fn write_persistent<T: IntoVal<Env, Val>>(env: &Env, key: &DataKey, value: &T) {
    env.storage().persistent().set(key, value);
    bump_persistent(env, key);
}

/// Reads a bounty from its terms and state entries. Storage getters return
/// `Option`; entry points map absence to domain errors.
fn read_bounty(env: &Env, id: u64, refresh_ttl: bool) -> Option<Bounty> {
    let terms: BountyTerms = read_persistent(env, &DataKey::BountyTerms(id), refresh_ttl)?;
    let state: BountyState = read_persistent(env, &DataKey::BountyState(id), refresh_ttl)?;
    Some(Bounty {
        id: terms.id,
        maintainer: terms.maintainer,
        repo_id: terms.repo_id,
        issue: terms.issue,
        token: terms.token,
        amount: terms.amount,
        fee_bps: terms.fee_bps,
        grace_secs: terms.grace_secs,
        deadline: terms.deadline,
        relayer_allowed: terms.relayer_allowed,
        status: state.status,
        winner: state.winner,
    })
}

pub fn get_bounty(env: &Env, id: u64) -> Option<Bounty> {
    read_bounty(env, id, true)
}

/// Reads a bounty without extending any TTL.
pub fn peek_bounty(env: &Env, id: u64) -> Option<Bounty> {
    read_bounty(env, id, false)
}

/// Extends both entries of a bounty using the normal TTL policy.
pub fn bump_bounty(env: &Env, id: u64) {
    bump_persistent(env, &DataKey::BountyTerms(id));
    bump_persistent(env, &DataKey::BountyState(id));
}

/// Extends both entries of a bounty to the maximum TTL.
pub fn bump_bounty_max(env: &Env, id: u64) {
    bump_persistent_max(env, &DataKey::BountyTerms(id));
    bump_persistent_max(env, &DataKey::BountyState(id));
}

/// Writes both terms and state. Use when terms change (funding, deadline, amount).
pub fn set_bounty(env: &Env, b: &Bounty) {
    write_persistent(
        env,
        &DataKey::BountyTerms(b.id),
        &BountyTerms {
            id: b.id,
            maintainer: b.maintainer.clone(),
            repo_id: b.repo_id,
            issue: b.issue,
            token: b.token.clone(),
            amount: b.amount,
            fee_bps: b.fee_bps,
            grace_secs: b.grace_secs,
            deadline: b.deadline,
            relayer_allowed: b.relayer_allowed,
        },
    );
    set_bounty_state(env, b);
}

/// Writes only status and winner, for transitions that leave the terms unchanged.
pub fn set_bounty_state(env: &Env, b: &Bounty) {
    write_persistent(
        env,
        &DataKey::BountyState(b.id),
        &BountyState {
            status: b.status,
            winner: b.winner.clone(),
        },
    );
}

pub fn get_bounty_by_issue(env: &Env, repo_id: u64, issue: u64) -> Option<Bounty> {
    // Views must not extend TTL.
    let id: u64 = read_persistent(env, &DataKey::ActiveByIssue(repo_id, issue), false)?;
    peek_bounty(env, id)
}

pub fn active_bounty(env: &Env, repo_id: u64, issue: u64) -> Option<u64> {
    let key = DataKey::ActiveByIssue(repo_id, issue);
    read_persistent(env, &key, true)
}

pub fn set_active(env: &Env, repo_id: u64, issue: u64, id: u64) {
    let key = DataKey::ActiveByIssue(repo_id, issue);
    write_persistent(env, &key, &id);
}

pub fn clear_active(env: &Env, repo_id: u64, issue: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveByIssue(repo_id, issue));
}

pub fn get_totals(env: &Env, token: &Address) -> Totals {
    let key = DataKey::Totals(token.clone());
    read_persistent(env, &key, false).unwrap_or_default()
}

pub fn set_totals(env: &Env, token: &Address, t: &Totals) {
    let key = DataKey::Totals(token.clone());
    write_persistent(env, &key, t);
}

/// Used when no timeout has been set. 14 days: long enough for a maintainer to reach the arbiter, short enough
/// that a lost arbiter key does not freeze funds for long.
pub const DEFAULT_DISPUTE_TIMEOUT_SECS: u64 = 14 * 86_400;

pub fn dispute_timeout(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::DisputeTimeout)
        .unwrap_or(DEFAULT_DISPUTE_TIMEOUT_SECS)
}

pub fn set_dispute_timeout(env: &Env, secs: u64) {
    env.storage()
        .instance()
        .set(&DataKey::DisputeTimeout, &secs);
    bump_instance(env);
}

pub fn get_dispute(env: &Env, id: u64) -> Option<DisputeRecord> {
    let key = DataKey::Dispute(id);
    let v = env.storage().persistent().get(&key);
    if v.is_some() {
        bump_persistent(env, &key);
    }
    v
}

pub fn set_dispute(env: &Env, id: u64, r: &DisputeRecord) {
    let key = DataKey::Dispute(id);
    env.storage().persistent().set(&key, r);
    bump_persistent(env, &key);
}

pub fn get_top_ups(env: &Env, id: u64) -> u32 {
    let key = DataKey::TopUps(id);
    match env.storage().persistent().get(&key) {
        Some(n) => {
            bump_persistent(env, &key);
            n
        }
        None => 0,
    }
}

pub fn set_top_ups(env: &Env, id: u64, n: u32) {
    let key = DataKey::TopUps(id);
    env.storage().persistent().set(&key, &n);
    bump_persistent(env, &key);
}

pub fn get_claim(env: &Env, id: u64) -> Option<ClaimRecord> {
    let key = DataKey::Claim(id);
    let v = env.storage().persistent().get(&key);
    if v.is_some() {
        bump_persistent(env, &key);
    }
    v
}

pub fn set_claim(env: &Env, id: u64, r: &ClaimRecord) {
    let key = DataKey::Claim(id);
    env.storage().persistent().set(&key, r);
    bump_persistent(env, &key);
}

pub fn get_claimable_total(env: &Env, token: &Address) -> i128 {
    let key = DataKey::ClaimableTotal(token.clone());
    match env.storage().persistent().get(&key) {
        Some(v) => {
            bump_persistent(env, &key);
            v
        }
        None => 0,
    }
}

pub fn set_claimable_total(env: &Env, token: &Address, v: i128) {
    let key = DataKey::ClaimableTotal(token.clone());
    env.storage().persistent().set(&key, &v);
    bump_persistent(env, &key);
}

pub fn get_relayer_limit(env: &Env, token: &Address) -> Option<RelayerLimit> {
    let key = DataKey::RelayerLimit(token.clone());
    let value = env.storage().persistent().get(&key);
    if value.is_some() {
        bump_persistent(env, &key);
    }
    value
}

pub fn set_relayer_limit(env: &Env, token: &Address, limit: &RelayerLimit) {
    let key = DataKey::RelayerLimit(token.clone());
    env.storage().persistent().set(&key, limit);
    bump_persistent(env, &key);
}

pub fn get_relayer_usage(env: &Env, token: &Address) -> RelayerUsage {
    let key = DataKey::RelayerUsage(token.clone());
    let value = env.storage().persistent().get(&key);
    if value.is_some() {
        bump_persistent(env, &key);
    }
    value.unwrap_or_default()
}

pub fn set_relayer_usage(env: &Env, token: &Address, usage: &RelayerUsage) {
    let key = DataKey::RelayerUsage(token.clone());
    env.storage().persistent().set(&key, usage);
    bump_persistent(env, &key);
}

pub fn upgrade_delay(env: &Env, fallback: u64) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::UpgradeDelay)
        .unwrap_or(fallback)
}

pub fn set_upgrade_delay(env: &Env, delay: u64) {
    env.storage().instance().set(&DataKey::UpgradeDelay, &delay);
    bump_instance(env);
}

pub fn pending_upgrade(env: &Env) -> Option<PendingUpgrade> {
    env.storage().instance().get(&DataKey::PendingUpgrade)
}

pub fn set_pending_upgrade(env: &Env, pending: &PendingUpgrade) {
    env.storage()
        .instance()
        .set(&DataKey::PendingUpgrade, pending);
    bump_instance(env);
}

pub fn clear_pending_upgrade(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingUpgrade);
}
