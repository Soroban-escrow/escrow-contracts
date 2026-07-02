use soroban_sdk::{contractevent, Address, BytesN, Symbol};

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
}

#[contractevent(topics = ["bounty", "config"])]
pub struct ConfigChanged {
    #[topic]
    pub field: Symbol,
}

#[contractevent(topics = ["bounty", "paused"])]
pub struct Paused {}

#[contractevent(topics = ["bounty", "unpaused"])]
pub struct Unpaused {}

#[contractevent(topics = ["bounty", "upgraded"])]
pub struct Upgraded {
    pub wasm_hash: BytesN<32>,
}
