use soroban_sdk::{contracttype, Address, BytesN};

use crate::{
    math::{add, sub},
    Error,
};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Open,
    Released,
    Refunded,
    Disputed,
    Resolved,
}

impl Status {
    /// Returns whether the bounty has reached a final lifecycle state.
    pub const fn is_terminal(&self) -> bool {
        match self {
            Self::Released | Self::Refunded | Self::Resolved => true,
            Self::Open | Self::Disputed => false,
        }
    }
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
    /// #163: grace_secs in force at funding time, snapshotted so a later
    /// `set_grace` call cannot extend the refund window on existing bounties.
    pub grace_secs: u64,
    pub deadline: u64,
    pub relayer_allowed: bool,
    pub status: Status,
    pub winner: Option<Address>,
    // #183: created_at removed — derivable from the Funded event emitted
    // at creation time. Dropping it shrinks the serialised Bounty by ~16 bytes
    // (u64 + alignment), reducing rent and CPU on every set_bounty rewrite.
    // Migration cost: a one-time read of the Funded event for each bounty
    // if a historical timestamp is ever needed; not stored on-chain.
}

/// Immutable terms are kept separately from settlement state so status-only
/// transitions do not rewrite the full bounty record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountyTerms {
    pub id: u64,
    pub maintainer: Address,
    pub repo_id: u64,
    pub issue: u64,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub grace_secs: u64,
    pub deadline: u64,
    pub relayer_allowed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountyState {
    pub status: Status,
    pub winner: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenLimits {
    pub min_amount: i128,
    pub max_amount: i128,
    pub fee_bps: Option<u32>,
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
    /// Issue #175: Optional fast-pause key. Can call pause but never unpause or move funds.
    pub guardian: Option<Address>,
}

/// Initialization arguments grouped into one Soroban ABI value (#114).
///
/// This changes the function input shape but not the stored `Config` layout.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitParams {
    pub admin: Address,
    pub relayer: Address,
    pub arbiter: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub grace_secs: u64,
    pub min_duration_secs: u64,
    pub max_duration_secs: u64,
}

/// Funding arguments grouped into one Soroban ABI value (#114).
///
/// This changes the function input shape but not the stored `Bounty` layout.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundBountyParams {
    pub maintainer: Address,
    pub repo_id: u64,
    pub issue: u64,
    pub token: Address,
    pub amount: i128,
    pub deadline: u64,
    pub relayer_allowed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolesConfig {
    pub admin: Address,
    pub relayer: Address,
    pub arbiter: Address,
    pub treasury: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyConfig {
    pub fee_bps: u32,
    pub grace_secs: u64,
    pub min_duration_secs: u64,
    pub max_duration_secs: u64,
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
    pub accrued_fees: i128,
}

impl Totals {
    pub(crate) fn record_fund(&mut self, amount: i128) -> Result<(), Error> {
        let funded = add(self.funded, amount)?;
        let held = add(self.held, amount)?;
        self.funded = funded;
        self.held = held;
        Ok(())
    }

    pub(crate) fn record_payout(
        &mut self,
        amount: i128,
        net: i128,
        fee: i128,
        paid: bool,
    ) -> Result<(), Error> {
        let released_net = if paid {
            add(self.released_net, net)?
        } else {
            self.released_net
        };
        let fees = add(self.fees, fee)?;
        let accrued_fees = add(self.accrued_fees, fee)?;
        let held = sub(self.held, amount)?;
        self.released_net = released_net;
        self.fees = fees;
        self.accrued_fees = accrued_fees;
        self.held = held;
        Ok(())
    }

    pub(crate) fn record_refund(&mut self, amount: i128) -> Result<(), Error> {
        let refunded = add(self.refunded, amount)?;
        let held = sub(self.held, amount)?;
        self.refunded = refunded;
        self.held = held;
        Ok(())
    }

    pub(crate) fn record_claim(&mut self, net: i128) -> Result<(), Error> {
        self.released_net = add(self.released_net, net)?;
        Ok(())
    }

    pub(crate) fn record_fee_withdrawal(&mut self) -> i128 {
        let amount = self.accrued_fees;
        self.accrued_fees = 0;
        amount
    }

    pub(crate) fn record_resolution(
        &mut self,
        amount: i128,
        winner_net: i128,
        fee: i128,
        maintainer_amount: i128,
        winner_paid: bool,
    ) -> Result<(), Error> {
        let released_net = if winner_net > 0 && winner_paid {
            add(self.released_net, winner_net)?
        } else {
            self.released_net
        };
        let fees = add(self.fees, fee)?;
        let accrued_fees = add(self.accrued_fees, fee)?;
        let refunded = add(self.refunded, maintainer_amount)?;
        let held = sub(self.held, amount)?;
        self.released_net = released_net;
        self.fees = fees;
        self.accrued_fees = accrued_fees;
        self.refunded = refunded;
        self.held = held;
        Ok(())
    }
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
    Roles,
    Policy,
    Paused,
    NextId,
    Bounty(u64),
    BountyTerms(u64),
    BountyState(u64),
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
