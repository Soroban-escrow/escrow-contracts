use soroban_sdk::{contractevent, Address, BytesN, Symbol, Vec};

use crate::types::SplitPayout;

#[contractevent(topics = ["bounty", "funded"])]
pub struct Funded {
    #[topic]
    pub id: u64,
    pub maintainer: Address,
    pub repo_id: u64,
    pub issue: u64,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub deadline: u64,
    pub relayer_allowed: bool,
}

#[contractevent(topics = ["bounty", "released"])]
pub struct Released {
    #[topic]
    pub id: u64,
    pub winner: Address,
    pub net: i128,
    pub fee: i128,
    pub caller: Address,
    /// True when the winner could not be paid now and the net is claimable later.
    pub deferred: bool,
}

#[contractevent(topics = ["bounty", "released_split"])]
pub struct ReleasedSplit {
    #[topic]
    pub id: u64,
    pub payouts: Vec<SplitPayout>,
    pub fee: i128,
    pub caller: Address,
}

#[contractevent(topics = ["bounty", "refunded"])]
pub struct Refunded {
    #[topic]
    pub id: u64,
    pub amount: i128,
}

#[contractevent(topics = ["bounty", "disputed"])]
pub struct Disputed {
    #[topic]
    pub id: u64,
    pub caller: Address,
}

#[contractevent(topics = ["bounty", "resolved"])]
pub struct Resolved {
    #[topic]
    pub id: u64,
    pub winner: Option<Address>,
    pub winner_net: i128,
    pub fee: i128,
    pub maintainer_amount: i128,
    pub winner_deferred: bool,
}

#[contractevent(topics = ["bounty", "config"])]
pub struct ConfigChanged {
    #[topic]
    pub field: Symbol,
}

#[contractevent(topics = ["bounty", "token_set"])]
pub struct TokenSet {
    #[topic]
    pub token: Address,
    pub min: i128,
    pub max: i128,
}

#[contractevent(topics = ["bounty", "token_removed"])]
pub struct TokenRemoved {
    #[topic]
    pub token: Address,
}

#[contractevent(topics = ["bounty", "paused"])]
pub struct Paused {}

#[contractevent(topics = ["bounty", "unpaused"])]
pub struct Unpaused {}

#[contractevent(topics = ["bounty", "upgraded"])]
pub struct Upgraded {
    pub wasm_hash: BytesN<32>,
}

#[contractevent(topics = ["bounty", "upgrade_proposed"])]
pub struct UpgradeProposed {
    pub wasm_hash: BytesN<32>,
    pub eta: u64,
}

#[contractevent(topics = ["bounty", "upgrade_cancelled"])]
pub struct UpgradeCancelled {
    pub wasm_hash: BytesN<32>,
}

#[contractevent(topics = ["bounty", "deadline_extended"])]
pub struct DeadlineExtended {
    #[topic]
    pub id: u64,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

#[contractevent(topics = ["bounty", "topped_up"])]
pub struct ToppedUp {
    #[topic]
    pub id: u64,
    pub extra: i128,
    pub new_amount: i128,
}

#[contractevent(topics = ["bounty", "dispute_expired"])]
pub struct DisputeExpired {
    #[topic]
    pub id: u64,
}

#[contractevent(topics = ["bounty", "claimed"])]
pub struct Claimed {
    #[topic]
    pub id: u64,
    pub winner: Address,
    pub net: i128,
}

/// The arbiter recorded the evidence behind a resolution. Emitted only by
/// `resolve_with_evidence`, right after `resolved`.
#[contractevent(topics = ["bounty", "evidence"])]
pub struct EvidenceRecorded {
    #[topic]
    pub id: u64,
    pub arbiter: Address,
    /// SHA-256 digest of the evidence bundle; see docs/CONTRACT.md.
    pub evidence_hash: BytesN<32>,
}
