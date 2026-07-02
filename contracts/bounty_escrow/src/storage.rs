use soroban_sdk::{Address, Env};

use crate::errors::Error;
use crate::types::{Bounty, Config, DataKey, TokenLimits, Totals};

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

pub fn next_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    env.storage().instance().set(&DataKey::NextId, &(id + 1));
    id
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

pub fn get_bounty(env: &Env, id: u64) -> Result<Bounty, Error> {
    let key = DataKey::Bounty(id);
    let b = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::BountyNotFound)?;
    bump_persistent(env, &key);
    Ok(b)
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
