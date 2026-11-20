use soroban_sdk::{Address, Env};

use crate::errors::Error;
use crate::types::{
    Bounty, BountyState, BountyTerms, ClaimRecord, Config, DataKey, DisputeRecord, TokenLimits,
    Totals,
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

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Storage getters return `Option`; entry points map absence to domain errors.
pub fn get_bounty(env: &Env, id: u64) -> Option<Bounty> {
    let terms_key = DataKey::BountyTerms(id);
    let state_key = DataKey::BountyState(id);
    let terms: BountyTerms = env.storage().persistent().get(&terms_key)?;
    let state: BountyState = env.storage().persistent().get(&state_key)?;
    bump_persistent(env, &terms_key);
    bump_persistent(env, &state_key);
    Some(Bounty {
        id: terms.id,
        maintainer: terms.maintainer,
        repo_id: terms.repo_id,
        issue: terms.issue,
        token: terms.token,
        amount: terms.amount,
        fee_bps: terms.fee_bps,
        deadline: terms.deadline,
        relayer_allowed: terms.relayer_allowed,
        status: state.status,
        winner: state.winner,
        created_at: terms.created_at,
    })
}

pub fn set_bounty(env: &Env, b: &Bounty) {
    let terms_key = DataKey::BountyTerms(b.id);
    let state_key = DataKey::BountyState(b.id);
    env.storage().persistent().set(
        &terms_key,
        &BountyTerms {
            id: b.id,
            maintainer: b.maintainer.clone(),
            repo_id: b.repo_id,
            issue: b.issue,
            token: b.token.clone(),
            amount: b.amount,
            fee_bps: b.fee_bps,
            deadline: b.deadline,
            relayer_allowed: b.relayer_allowed,
            created_at: b.created_at,
        },
    );
    env.storage().persistent().set(
        &state_key,
        &BountyState {
            status: b.status,
            winner: b.winner.clone(),
        },
    );
    bump_persistent(env, &terms_key);
    bump_persistent(env, &state_key);
}

pub fn set_bounty_state(env: &Env, b: &Bounty) {
    let key = DataKey::BountyState(b.id);
    env.storage().persistent().set(
        &key,
        &BountyState {
            status: b.status,
            winner: b.winner.clone(),
        },
    );
    bump_persistent(env, &key);
}

pub fn get_bounty_by_issue(env: &Env, repo_id: u64, issue: u64) -> Option<Bounty> {
    let id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveByIssue(repo_id, issue))?;
    // Views must not extend TTL. Read the two entries directly.
    let terms: BountyTerms = env.storage().persistent().get(&DataKey::BountyTerms(id))?;
    let state: BountyState = env.storage().persistent().get(&DataKey::BountyState(id))?;
    Some(Bounty {
        id: terms.id,
        maintainer: terms.maintainer,
        repo_id: terms.repo_id,
        issue: terms.issue,
        token: terms.token,
        amount: terms.amount,
        fee_bps: terms.fee_bps,
        deadline: terms.deadline,
        relayer_allowed: terms.relayer_allowed,
        status: state.status,
        winner: state.winner,
        created_at: terms.created_at,
    })
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
        Some(t) => {
            bump_persistent(env, &key);
            t
        }
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
