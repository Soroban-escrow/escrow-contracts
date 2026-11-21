use soroban_sdk::{Address, Env, Vec};

use crate::errors::Error;
use crate::types::{
    Bounty, ClaimRecord, Config, DataKey, DisputeRecord, Status, TokenLimits, Totals,
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

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

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
    env.storage().instance().get(&DataKey::Token(token.clone()))
}

pub fn set_token(env: &Env, token: &Address, l: &TokenLimits) {
    env.storage()
        .instance()
        .set(&DataKey::Token(token.clone()), l);
}

pub fn remove_token(env: &Env, token: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey::Token(token.clone()));
}

pub fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_bounty(env: &Env, id: u64) -> Result<Bounty, Error> {
    let key = DataKey::Bounty(id);
    let b: Bounty = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::BountyNotFound)?;
    if b.status == Status::Open || b.status == Status::Disputed {
        bump_persistent(env, &key);
    }
    Ok(b)
}

pub const MAX_BOUNTIES_LIMIT: u32 = 50;

pub fn get_bounties(env: &Env, start: u64, limit: u32) -> Vec<Bounty> {
    let capped_limit = limit.min(MAX_BOUNTIES_LIMIT);
    let mut list = Vec::new(env);
    if capped_limit == 0 {
        return list;
    }
    let max_id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    let mut id = start;
    while id < max_id && list.len() < capped_limit {
        let key = DataKey::Bounty(id);
        if let Some(b) = env.storage().persistent().get::<DataKey, Bounty>(&key) {
            if b.status == Status::Open || b.status == Status::Disputed {
                bump_persistent(env, &key);
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

pub fn set_bounty(env: &Env, b: &Bounty) {
    let key = DataKey::Bounty(b.id);
    env.storage().persistent().set(&key, b);
    bump_persistent(env, &key);
}

pub fn active_bounty(env: &Env, repo_id: u64, issue: u64) -> Option<u64> {
    let key = DataKey::ActiveByIssue(repo_id, issue);
    let v: Option<u64> = env.storage().persistent().get(&key);
    if v.is_some() {
        bump_persistent(env, &key);
    }
    v
}

pub fn set_active(env: &Env, repo_id: u64, issue: u64, id: u64) {
    let key = DataKey::ActiveByIssue(repo_id, issue);
    env.storage().persistent().set(&key, &id);
    bump_persistent(env, &key);
}

pub fn clear_active(env: &Env, repo_id: u64, issue: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveByIssue(repo_id, issue));
}

pub fn get_totals(env: &Env, token: &Address) -> Totals {
    let key = DataKey::Totals(token.clone());
    match env.storage().persistent().get(&key) {
        Some(t) => t,
        None => Totals::default(),
    }
}

pub fn set_totals(env: &Env, token: &Address, t: &Totals) {
    let key = DataKey::Totals(token.clone());
    env.storage().persistent().set(&key, t);
    bump_persistent(env, &key);
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
