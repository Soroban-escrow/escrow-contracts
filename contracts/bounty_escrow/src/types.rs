use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Open,
    Released,
    Refunded,
    Disputed,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bounty {
    pub id: u64,
    pub maintainer: Address,
    pub repo_id: u64,
    pub issue: u64,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub deadline: u64,
    pub relayer_allowed: bool,
    pub status: Status,
    pub winner: Option<Address>,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenLimits {
    pub min_amount: i128,
    pub max_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    pub relayer: Address,
    pub arbiter: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub grace_secs: u64,
    pub min_duration_secs: u64,
    pub max_duration_secs: u64,
    pub paused: bool,
}

/// Per-token accounting: funded == released_net + fees + refunded + held.
#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Totals {
    pub funded: i128,
    pub released_net: i128,
    pub fees: i128,
    pub refunded: i128,
    pub held: i128,
}

#[contracttype]
pub enum DataKey {
    Config,
    NextId,
    Bounty(u64),
    ActiveByIssue(u64, u64),
    Token(Address),
    Totals(Address),
}
