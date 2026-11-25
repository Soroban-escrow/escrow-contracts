//! Admin setters, roles, pause, fee treasury and upgrades.
//!
//! Bodies of the entry points in lib.rs; the contract surface stays in lib.rs.

use super::*;

pub(crate) fn init(env: Env, params: InitParams) -> Result<(), Error> {
    let InitParams {
        admin,
        relayer,
        arbiter,
        treasury,
        fee_bps,
        grace_secs,
        min_duration_secs,
        max_duration_secs,
    } = params;
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
    let event = events::Initialized {
        admin: admin.clone(),
        relayer: relayer.clone(),
        arbiter: arbiter.clone(),
        treasury: treasury.clone(),
        fee_bps,
        grace_secs,
        min_duration_secs,
        max_duration_secs,
        paused: false,
    };
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
            guardian: None,
        },
    );
    event.publish(&env);
    Ok(())
}

pub(crate) fn set_token(
    env: Env,
    admin: Address,
    token: Address,
    min: i128,
    max: i128,
    fee_bps: Option<u32>,
) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    if let Some(f) = fee_bps {
        if f > fees::MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
    }
    if min <= 0 {
        return Err(Error::InvalidLimits);
    }
    if max < min {
        return Err(Error::InvalidLimits);
    }
    storage::set_token(
        &env,
        &token,
        &TokenLimits {
            min_amount: min,
            max_amount: max,
            fee_bps,
        },
    );
    changed(&env, symbol_short!("token"));
    Ok(())
}

pub(crate) fn remove_token(env: Env, admin: Address, token: Address) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    if storage::get_token(&env, &token).is_none() {
        return Err(Error::TokenNotAllowed);
    }
    storage::remove_token(&env, &token);
    changed(&env, symbol_short!("token"));
    Ok(())
}

pub(crate) fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
    let mut c = require_admin(&env, &admin)?;
    if fee_bps > fees::MAX_FEE_BPS {
        return Err(Error::InvalidFee);
    }
    c.fee_bps = fee_bps;
    storage::set_config(&env, &c);
    changed(&env, symbol_short!("fee"));
    Ok(())
}

#[derive(Clone, Copy)]
enum Role {
    Relayer,
    Arbiter,
    Treasury,
}

/// Single authorization, storage write and event path for role changes.
fn set_role(env: Env, admin: Address, next: Address, role: Role) -> Result<(), Error> {
    let mut config = require_admin(&env, &admin)?;
    let field = match role {
        Role::Relayer => {
            config.relayer = next;
            symbol_short!("relayer")
        }
        Role::Arbiter => {
            config.arbiter = next;
            symbol_short!("arbiter")
        }
        Role::Treasury => {
            config.treasury = next;
            symbol_short!("treasury")
        }
    };
    storage::set_config(&env, &config);
    changed(&env, field);
    Ok(())
}

pub(crate) fn set_relayer(env: Env, admin: Address, relayer: Address) -> Result<(), Error> {
    set_role(env, admin, relayer, Role::Relayer)
}

pub(crate) fn set_arbiter(env: Env, admin: Address, arbiter: Address) -> Result<(), Error> {
    set_role(env, admin, arbiter, Role::Arbiter)
}

pub(crate) fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), Error> {
    set_role(env, admin, treasury, Role::Treasury)
}

pub(crate) fn set_grace(env: Env, admin: Address, grace_secs: u64) -> Result<(), Error> {
    let mut c = require_admin(&env, &admin)?;
    c.grace_secs = grace_secs;
    storage::set_config(&env, &c);
    changed(&env, symbol_short!("grace"));
    Ok(())
}

pub(crate) fn set_durations(env: Env, admin: Address, min: u64, max: u64) -> Result<(), Error> {
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

pub(crate) fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
    let mut c = require_admin(&env, &admin)?;
    new_admin.require_auth();
    c.admin = new_admin;
    storage::set_config(&env, &c);
    changed(&env, symbol_short!("admin"));
    Ok(())
}

pub(crate) fn pause(env: Env, admin: Address) -> Result<(), Error> {
    let mut c = require_admin(&env, &admin)?;
    c.paused = true;
    storage::set_config(&env, &c);
    events::Paused {}.publish(&env);
    Ok(())
}

pub(crate) fn unpause(env: Env, admin: Address) -> Result<(), Error> {
    let mut c = require_admin(&env, &admin)?;
    c.paused = false;
    storage::set_config(&env, &c);
    events::Unpaused {}.publish(&env);
    Ok(())
}

pub(crate) fn set_dispute_timeout(env: Env, admin: Address, secs: u64) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    if secs == 0 {
        return Err(Error::InvalidDeadline);
    }
    storage::set_dispute_timeout(&env, secs);
    changed(&env, symbol_short!("dispute"));
    Ok(())
}

pub(crate) fn set_relayer_limit(
    env: Env,
    admin: Address,
    token: Address,
    window_secs: u64,
    cap: i128,
) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    if window_secs == 0 || cap <= 0 {
        return Err(Error::InvalidDeadline);
    }
    storage::set_relayer_limit(&env, &token, &RelayerLimit { window_secs, cap });
    changed(&env, symbol_short!("rlimit"));
    Ok(())
}

pub(crate) fn set_upgrade_delay(env: Env, admin: Address, delay_secs: u64) -> Result<(), Error> {
    let cfg = require_admin(&env, &admin)?;
    let window = cfg
        .max_duration_secs
        .checked_add(cfg.grace_secs)
        .ok_or(Error::Overflow)?;
    let current = storage::upgrade_delay(&env, window);
    if delay_secs < current {
        return Err(Error::UpgradeDelayCannotDecrease);
    }
    storage::set_upgrade_delay(&env, delay_secs);
    changed(&env, symbol_short!("upgrade"));
    Ok(())
}

pub(crate) fn propose_upgrade(
    env: Env,
    admin: Address,
    wasm_hash: BytesN<32>,
) -> Result<(), Error> {
    let cfg = require_admin(&env, &admin)?;
    let eta = env
        .ledger()
        .timestamp()
        .checked_add(storage::upgrade_delay(
            &env,
            cfg.max_duration_secs
                .checked_add(cfg.grace_secs)
                .ok_or(Error::Overflow)?,
        ))
        .ok_or(Error::Overflow)?;
    storage::set_pending_upgrade(
        &env,
        &PendingUpgrade {
            wasm_hash: wasm_hash.clone(),
            eta,
        },
    );
    events::UpgradeProposed { wasm_hash, eta }.publish(&env);
    Ok(())
}

pub(crate) fn execute_upgrade(env: Env, admin: Address) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    let pending = storage::pending_upgrade(&env).ok_or(Error::NoPendingUpgrade)?;
    if env.ledger().timestamp() < pending.eta {
        return Err(Error::UpgradeNotReady);
    }
    storage::clear_pending_upgrade(&env);
    env.deployer()
        .update_current_contract(ContractExecutable::Wasm(pending.wasm_hash.clone()));
    events::Upgraded {
        wasm_hash: pending.wasm_hash,
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn cancel_upgrade(env: Env, admin: Address) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    let pending = storage::pending_upgrade(&env).ok_or(Error::NoPendingUpgrade)?;
    storage::clear_pending_upgrade(&env);
    events::UpgradeCancelled {
        wasm_hash: pending.wasm_hash,
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn withdraw_fees(env: Env, caller: Address, token: Address) -> Result<i128, Error> {
    caller.require_auth();
    let cfg = storage::get_config(&env)?;
    if caller != cfg.treasury && caller != cfg.admin {
        return Err(Error::Unauthorized);
    }
    let mut t = storage::get_totals(&env, &token);
    let amount = t.record_fee_withdrawal();
    if amount <= 0 {
        return Err(Error::AmountTooSmall);
    }
    storage::set_totals(&env, &token, &t);

    token::Client::new(&env, &token).transfer(
        &env.current_contract_address(),
        &cfg.treasury,
        &amount,
    );
    events::FeesWithdrawn {
        token,
        treasury: cfg.treasury,
        amount,
    }
    .publish(&env);
    Ok(amount)
}

pub(crate) fn upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) -> Result<(), Error> {
    require_admin(&env, &admin)?;
    env.deployer()
        .update_current_contract(ContractExecutable::Wasm(wasm_hash.clone()));
    events::Upgraded { wasm_hash }.publish(&env);
    Ok(())
}
