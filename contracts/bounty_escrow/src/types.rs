use soroban_sdk::{contracttype, Address, BytesN};

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
pub struct WinnerShare {
    pub winner: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitPayout {
    pub winner: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayerLimit {
    pub window_secs: u64,
    pub cap: i128,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelayerUsage {
    pub window_start: u64,
    pub released: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub wasm_hash: BytesN<32>,
    pub eta: u64,
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

/// Written by `open_dispute`. Its presence means the bounty has used its one dispute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub opened_at: u64,
}

/// A winner payout that could not be transferred at release time (for example, no trustline).
/// The net stays in the contract until `claim` pays it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRecord {
    pub winner: Address,
    pub net: i128,
    pub claimed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseSettlement {
    pub net: i128,
    pub fee: i128,
    pub deferred: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundSettlement {
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveSettlement {
    pub winner_net: i128,
    pub fee: i128,
    pub maintainer_amount: i128,
    pub winner_deferred: bool,
}

#[contracttype]
pub enum DataKey {
    Config,
    NextId,
    Bounty(u64),
    ActiveByIssue(u64, u64),
    Token(Address),
    Totals(Address),
    /// Instance storage. Kept out of `Config` so existing deployments keep decoding their config.
    DisputeTimeout,
    Dispute(u64),
    TopUps(u64),
    Claim(u64),
    /// Sum of unclaimed winner credits per token. `held + claimable == balance`.
    ClaimableTotal(Address),
    RelayerLimit(Address),
    RelayerUsage(Address),
    UpgradeDelay,
    PendingUpgrade,
}
