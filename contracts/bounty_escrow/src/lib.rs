#![no_std]

mod errors;
mod events;
mod fees;
mod storage;
mod types;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, token, Address, BytesN, ContractExecutable, Env, Symbol,
};

pub use errors::Error;
pub use types::{Bounty, Config, DataKey, Status, TokenLimits, Totals};

#[contract]
pub struct BountyEscrow;

fn require_admin(env: &Env, admin: &Address) -> Result<Config, Error> {
    let c = storage::get_config(env)?;
    if *admin != c.admin {
        return Err(Error::Unauthorized);
    }
    admin.require_auth();
    Ok(c)
}

fn changed(env: &Env, field: Symbol) {
    events::ConfigChanged { field }.publish(env);
}

fn add(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

fn sub(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

#[contractimpl]
impl BountyEscrow {
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        env: Env,
        admin: Address,
        relayer: Address,
        arbiter: Address,
        treasury: Address,
        fee_bps: u32,
        grace_secs: u64,
        min_duration_secs: u64,
        max_duration_secs: u64,
    ) -> Result<(), Error> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(Error::AlreadyInitialized);
        }
        if fee_bps > fees::MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
        if min_duration_secs > max_duration_secs {
            return Err(Error::InvalidDeadline);
        }
        storage::set_config(
            &env,
            &Config {
                admin,
                relayer,
                arbiter,
                treasury,
                fee_bps,
                grace_secs,
                min_duration_secs,
                max_duration_secs,
                paused: false,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fund_bounty(
        env: Env,
        maintainer: Address,
        repo_id: u64,
        issue: u64,
        token: Address,
        amount: i128,
        deadline: u64,
        relayer_allowed: bool,
    ) -> Result<u64, Error> {
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

        let id = storage::next_id(&env);
        storage::set_bounty(
            &env,
            &Bounty {
                id,
                maintainer: maintainer.clone(),
                repo_id,
                issue,
                token: token.clone(),
                amount,
                fee_bps: cfg.fee_bps,
                deadline,
                relayer_allowed,
                status: Status::Open,
                winner: None,
                created_at: now,
            },
        );
        storage::set_active(&env, repo_id, issue, id);
        let mut t = storage::get_totals(&env, &token);
        t.funded = add(t.funded, amount)?;
        t.held = add(t.held, amount)?;
        storage::set_totals(&env, &token, &t);

        token::Client::new(&env, &token).transfer(
            &maintainer,
            env.current_contract_address(),
            &amount,
        );
        events::Funded {
            id,
            maintainer,
            repo_id,
            issue,
            token,
            amount,
            fee_bps: cfg.fee_bps,
            deadline,
            relayer_allowed,
        }
        .publish(&env);
        Ok(id)
    }

    pub fn release(
        env: Env,
        caller: Address,
        bounty_id: u64,
        winner: Address,
    ) -> Result<(), Error> {
        caller.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if caller != b.maintainer {
            if caller != cfg.relayer {
                return Err(Error::Unauthorized);
            }
            if !b.relayer_allowed {
                return Err(Error::RelayerNotAllowed);
            }
        }
        if winner == b.maintainer || winner == env.current_contract_address() {
            return Err(Error::SelfPayout);
        }
        let fee = fees::fee(b.amount, b.fee_bps)?;
        let net = fees::net(b.amount, b.fee_bps)?;

        b.status = Status::Released;
        b.winner = Some(winner.clone());
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);
        let mut t = storage::get_totals(&env, &b.token);
        t.released_net = add(t.released_net, net)?;
        t.fees = add(t.fees, fee)?;
        t.held = sub(t.held, b.amount)?;
        storage::set_totals(&env, &b.token, &t);

        let tok = token::Client::new(&env, &b.token);
        let me = env.current_contract_address();
        tok.transfer(&me, &winner, &net);
        if fee > 0 {
            tok.transfer(&me, &cfg.treasury, &fee);
        }
        events::Released {
            id: bounty_id,
            winner,
            net,
            fee,
            caller,
        }
        .publish(&env);
        Ok(())
    }

    pub fn refund(env: Env, maintainer: Address, bounty_id: u64) -> Result<(), Error> {
        maintainer.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if maintainer != b.maintainer {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        let opens = b
            .deadline
            .checked_add(cfg.grace_secs)
            .ok_or(Error::Overflow)?;
        if env.ledger().timestamp() < opens {
            return Err(Error::DeadlineNotReached);
        }
        b.status = Status::Refunded;
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);
        let mut t = storage::get_totals(&env, &b.token);
        t.refunded = add(t.refunded, b.amount)?;
        t.held = sub(t.held, b.amount)?;
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
        Ok(())
    }

    pub fn open_dispute(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        caller.require_auth();
        let cfg = storage::get_config(&env)?;
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if caller != b.maintainer && caller != cfg.arbiter {
            return Err(Error::Unauthorized);
        }
        if b.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        b.status = Status::Disputed;
        storage::set_bounty(&env, &b);
        events::Disputed {
            id: bounty_id,
            caller,
        }
        .publish(&env);
        Ok(())
    }

    pub fn resolve(
        env: Env,
        arbiter: Address,
        bounty_id: u64,
        winner: Option<Address>,
        winner_bps: u32,
    ) -> Result<(), Error> {
        arbiter.require_auth();
        let cfg = storage::get_config(&env)?;
        if arbiter != cfg.arbiter {
            return Err(Error::Unauthorized);
        }
        let mut b = storage::get_bounty(&env, bounty_id)?;
        if b.status != Status::Disputed {
            return Err(Error::InvalidStatus);
        }
        if winner_bps > 10_000 {
            return Err(Error::InvalidSplit);
        }
        let portion = fees::portion(b.amount, winner_bps)?;
        let mut fee = 0;
        let mut winner_net = 0;
        if winner_bps > 0 {
            match &winner {
                None => return Err(Error::InvalidSplit),
                Some(w) if *w == b.maintainer || *w == env.current_contract_address() => {
                    return Err(Error::SelfPayout)
                }
                Some(_) => {}
            }
            fee = fees::fee(portion, b.fee_bps)?;
            winner_net = sub(portion, fee)?;
        }
        let maintainer_amount = sub(b.amount, portion)?;

        b.status = Status::Resolved;
        b.winner = if winner_bps > 0 { winner.clone() } else { None };
        storage::set_bounty(&env, &b);
        storage::clear_active(&env, b.repo_id, b.issue);
        let mut t = storage::get_totals(&env, &b.token);
        t.released_net = add(t.released_net, winner_net)?;
        t.fees = add(t.fees, fee)?;
        t.refunded = add(t.refunded, maintainer_amount)?;
        t.held = sub(t.held, b.amount)?;
        storage::set_totals(&env, &b.token, &t);

        let tok = token::Client::new(&env, &b.token);
        let me = env.current_contract_address();
        if let (Some(w), true) = (&winner, winner_net > 0) {
            tok.transfer(&me, w, &winner_net);
        }
        if fee > 0 {
            tok.transfer(&me, &cfg.treasury, &fee);
        }
        if maintainer_amount > 0 {
            tok.transfer(&me, &b.maintainer, &maintainer_amount);
        }
        events::Resolved {
            id: bounty_id,
            winner: b.winner,
            winner_net,
            fee,
            maintainer_amount,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_bounty(env: Env, id: u64) -> Result<Bounty, Error> {
        storage::get_bounty(&env, id)
    }

    pub fn active_bounty(env: Env, repo_id: u64, issue: u64) -> Option<u64> {
        storage::active_bounty(&env, repo_id, issue)
    }

    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env)
    }

    pub fn get_totals(env: Env, token: Address) -> Totals {
        storage::get_totals(&env, &token)
    }

    pub fn get_token(env: Env, token: Address) -> Option<TokenLimits> {
        storage::get_token(&env, &token)
    }

    // ---- admin ----

    pub fn set_token(
        env: Env,
        admin: Address,
        token: Address,
        min: i128,
        max: i128,
    ) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if min <= 0 {
            return Err(Error::AmountTooSmall);
        }
        if max < min {
            return Err(Error::AmountTooLarge);
        }
        storage::set_token(
            &env,
            &token,
            &TokenLimits {
                min_amount: min,
                max_amount: max,
            },
        );
        changed(&env, symbol_short!("token"));
        Ok(())
    }

    pub fn remove_token(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        if storage::get_token(&env, &token).is_none() {
            return Err(Error::TokenNotAllowed);
        }
        storage::remove_token(&env, &token);
        changed(&env, symbol_short!("token"));
        Ok(())
    }

    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        if fee_bps > fees::MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
        c.fee_bps = fee_bps;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("fee"));
        Ok(())
    }

    pub fn set_relayer(env: Env, admin: Address, relayer: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.relayer = relayer;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("relayer"));
        Ok(())
    }

    pub fn set_arbiter(env: Env, admin: Address, arbiter: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.arbiter = arbiter;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("arbiter"));
        Ok(())
    }

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.treasury = treasury;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("treasury"));
        Ok(())
    }

    pub fn set_grace(env: Env, admin: Address, grace_secs: u64) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.grace_secs = grace_secs;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("grace"));
        Ok(())
    }

    pub fn set_durations(env: Env, admin: Address, min: u64, max: u64) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        if min > max {
            return Err(Error::InvalidDeadline);
        }
        c.min_duration_secs = min;
        c.max_duration_secs = max;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("duration"));
        Ok(())
    }

    /// Two-party auth: the new admin must also sign, to avoid typos.
    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        new_admin.require_auth();
        c.admin = new_admin;
        storage::set_config(&env, &c);
        changed(&env, symbol_short!("admin"));
        Ok(())
    }

    /// Blocks `fund_bounty` only; release, refund and resolve keep working.
    pub fn pause(env: Env, admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.paused = true;
        storage::set_config(&env, &c);
        events::Paused {}.publish(&env);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), Error> {
        let mut c = require_admin(&env, &admin)?;
        c.paused = false;
        storage::set_config(&env, &c);
        events::Unpaused {}.publish(&env);
        Ok(())
    }

    pub fn upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(wasm_hash.clone()));
        events::Upgraded { wasm_hash }.publish(&env);
        Ok(())
    }
}
