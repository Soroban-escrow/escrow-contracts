use std::vec::Vec;

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    Address, BytesN,
};

use super::setup::*;
use crate::{BountyEscrow, BountyEscrowClient, DataKey, Error, TokenLimits};

#[test]
fn init_once() {
    let c = Ctx::new();
    let a = Address::generate(&c.env);
    let r = c.client.try_init(&a, &a, &a, &a, &0, &0, &DAY, &DAY);
    assert_eq!(r.unwrap_err().unwrap(), Error::AlreadyInitialized);
    let cfg = c.client.get_config();
    assert_eq!(cfg.admin, c.admin);
    assert_eq!(cfg.fee_bps, FEE_BPS);
}

#[test]
fn init_validates() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let id = env.register(BountyEscrow, ());
    let cl = BountyEscrowClient::new(&env, &id);
    let a = Address::generate(&env);
    assert_eq!(
        cl.try_init(&a, &a, &a, &a, &1_001, &0, &1, &2)
            .unwrap_err()
            .unwrap(),
        Error::InvalidFee
    );
    assert_eq!(
        cl.try_init(&a, &a, &a, &a, &300, &0, &5, &2)
            .unwrap_err()
            .unwrap(),
        Error::InvalidDeadline
    );
    assert_eq!(
        cl.try_get_config().unwrap_err().unwrap(),
        Error::NotInitialized
    );
}

#[test]
fn fee_cap() {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &1_000);
    assert_eq!(
        c.client.try_set_fee(&c.admin, &1_001).unwrap_err().unwrap(),
        Error::InvalidFee
    );
}

#[test]
fn fee_change_does_not_affect_existing_bounties() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.set_fee(&c.admin, &1_000);
    assert_eq!(c.client.get_bounty(&id).fee_bps, FEE_BPS);
    let id2 = c.fund(2);
    assert_eq!(c.client.get_bounty(&id2).fee_bps, 1_000);
}

#[test]
fn set_admin_needs_both_auths() {
    let c = Ctx::new();
    let new_admin = Address::generate(&c.env);
    c.client.set_admin(&c.admin, &new_admin);
    let auths = c.env.auths();
    let signers: Vec<_> = auths.iter().map(|(a, _)| a.clone()).collect();
    assert!(signers.contains(&c.admin));
    assert!(signers.contains(&new_admin));
    assert_eq!(c.client.get_config().admin, new_admin);
    // old admin no longer works
    assert_eq!(
        c.client.try_set_fee(&c.admin, &0).unwrap_err().unwrap(),
        Error::Unauthorized
    );
}

#[test]
fn old_admin_loses_every_admin_entry_point_after_handover() {
    let c = Ctx::new();
    let old_admin = c.admin.clone();
    let new_admin = Address::generate(&c.env);
    c.client.set_admin(&old_admin, &new_admin);
    let wasm_hash = c.env.deployer().upload_contract_wasm(WASM);

    assert_eq!(
        c.client.try_set_fee(&old_admin, &1).unwrap_err().unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_relayer(&old_admin, &new_admin)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_arbiter(&old_admin, &new_admin)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_treasury(&old_admin, &new_admin)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client.try_set_grace(&old_admin, &1).unwrap_err().unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_durations(&old_admin, &1, &2)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_token(&old_admin, &c.token, &1, &UNIT)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_remove_token(&old_admin, &c.token)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client.try_pause(&old_admin).unwrap_err().unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client.try_unpause(&old_admin).unwrap_err().unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_set_dispute_timeout(&old_admin, &1)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
    assert_eq!(
        c.client
            .try_upgrade(&old_admin, &wasm_hash)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );

    // The new admin is accepted by every admin family, including upgrade.
    c.client.set_fee(&new_admin, &1);
    c.client.set_relayer(&new_admin, &old_admin);
    c.client.set_arbiter(&new_admin, &old_admin);
    c.client.set_treasury(&new_admin, &old_admin);
    c.client.set_grace(&new_admin, &1);
    c.client.set_durations(&new_admin, &1, &2);
    c.client.set_token(&new_admin, &c.token, &1, &UNIT);
    c.client.remove_token(&new_admin, &c.token);
    c.client.pause(&new_admin);
    c.client.unpause(&new_admin);
    c.client.set_dispute_timeout(&new_admin, &1);
    c.client.upgrade(&new_admin, &wasm_hash);
}

#[test]
fn set_admin_fails_without_new_admin_signature() {
    let c = Ctx::new();
    let new_admin = Address::generate(&c.env);
    c.env.set_auths(&[]);
    assert!(c.client.try_set_admin(&c.admin, &new_admin).is_err());
}

#[test]
fn removing_token_blocks_funding_only() {
    let c = Ctx::new();
    let id = c.fund(1);
    let id2 = c.fund(2);
    c.client.remove_token(&c.admin, &c.token);
    let r = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &3,
        &c.token,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
    assert_eq!(r.unwrap_err().unwrap(), Error::TokenNotAllowed);
    c.client.release(&c.maintainer, &id, &c.winner);
    c.set_time(c.now() + 30 * DAY);
    c.client.refund(&c.maintainer, &id2);
    assert_eq!(
        c.client
            .try_remove_token(&c.admin, &c.token)
            .unwrap_err()
            .unwrap(),
        Error::TokenNotAllowed
    );
}

#[test]
fn set_token_validates() {
    let c = Ctx::new();
    assert_eq!(
        c.client
            .try_set_token(&c.admin, &c.token, &0, &5)
            .unwrap_err()
            .unwrap(),
        Error::AmountTooSmall
    );
    assert_eq!(
        c.client
            .try_set_token(&c.admin, &c.token, &5, &4)
            .unwrap_err()
            .unwrap(),
        Error::AmountTooLarge
    );
    let t = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();
    StellarAssetClient::new(&c.env, &t).mint(&c.maintainer, &UNIT);
    c.client.set_token(&c.admin, &t, &1, &UNIT);
    assert_eq!(c.client.get_token(&t).unwrap().max_amount, UNIT);
}

#[test]
fn token_changes_emit_dedicated_events() {
    let c = Ctx::new();
    let token = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();
    c.client.set_token(&c.admin, &token, &1, &UNIT);
    c.client.remove_token(&c.admin, &token);
    assert!(c.env.events().all().events().len() >= 2);
}

#[test]
fn setters_update_config() {
    let c = Ctx::new();
    let a = Address::generate(&c.env);
    c.client.set_relayer(&c.admin, &a);
    c.client.set_arbiter(&c.admin, &a);
    c.client.set_treasury(&c.admin, &a);
    c.client.set_grace(&c.admin, &5);
    c.client.set_durations(&c.admin, &1, &2);
    let cfg = c.client.get_config();
    assert_eq!(
        (cfg.relayer, cfg.arbiter, cfg.treasury),
        (a.clone(), a.clone(), a)
    );
    assert_eq!(
        (cfg.grace_secs, cfg.min_duration_secs, cfg.max_duration_secs),
        (5, 1, 2)
    );
    assert_eq!(
        c.client
            .try_set_durations(&c.admin, &3, &2)
            .unwrap_err()
            .unwrap(),
        Error::InvalidDeadline
    );
}

#[test]
fn all_setters_require_admin() {
    let c = Ctx::new();
    let x = Address::generate(&c.env);
    let e = Error::Unauthorized;
    assert_eq!(c.client.try_set_fee(&x, &1).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_set_relayer(&x, &x).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_set_arbiter(&x, &x).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_set_treasury(&x, &x).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_set_grace(&x, &1).unwrap_err().unwrap(), e);
    assert_eq!(
        c.client.try_set_durations(&x, &1, &2).unwrap_err().unwrap(),
        e
    );
    assert_eq!(
        c.client
            .try_set_token(&x, &c.token, &1, &2)
            .unwrap_err()
            .unwrap(),
        e
    );
    assert_eq!(
        c.client
            .try_remove_token(&x, &c.token)
            .unwrap_err()
            .unwrap(),
        e
    );
    assert_eq!(c.client.try_set_admin(&x, &x).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_pause(&x).unwrap_err().unwrap(), e);
    assert_eq!(c.client.try_unpause(&x).unwrap_err().unwrap(), e);
    assert_eq!(
        c.client
            .try_propose_upgrade(&x, &BytesN::from_array(&c.env, &[0; 32]))
            .unwrap_err()
            .unwrap(),
        e
    );
}

#[test]
fn setters_require_admin_signature() {
    let c = Ctx::new();
    c.env.set_auths(&[]);
    assert!(c.client.try_set_fee(&c.admin, &1).is_err());
    assert!(c.client.try_pause(&c.admin).is_err());
}

#[test]
fn setters_emit_events() {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &100);
    assert!(!c.env.events().all().events().is_empty());
    c.client.pause(&c.admin);
    c.client.unpause(&c.admin);
    assert!(!c.client.get_config().paused);
}

const WASM: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/bounty_escrow.wasm");

#[test]
fn upgrade_swaps_the_code_and_keeps_state() {
    let c = Ctx::new();
    let id = c.fund(1);
    let hash = c.env.deployer().upload_contract_wasm(WASM);
    c.client.propose_upgrade(&c.admin, &hash);
    assert_eq!(
        c.client.try_execute_upgrade(&c.admin).unwrap_err().unwrap(),
        Error::UpgradeNotReady
    );
    c.set_time(c.now() + 90 * DAY + GRACE);
    c.client.execute_upgrade(&c.admin);
    // state survives and the (same) code still answers
    assert_eq!(c.client.get_bounty(&id).amount, 100 * UNIT);
    assert_eq!(c.client.get_config().admin, c.admin);
    c.client.release(&c.maintainer, &id, &c.winner);
}

#[test]
fn upgrade_requires_admin_signature() {
    let c = Ctx::new();
    let hash = c.env.deployer().upload_contract_wasm(WASM);
    c.env.set_auths(&[]);
    assert!(c.client.try_propose_upgrade(&c.admin, &hash).is_err());
}

#[test]
fn upgrade_can_be_replaced_cancelled_and_delay_only_increased() {
    let c = Ctx::new();
    let first = BytesN::from_array(&c.env, &[1; 32]);
    let second = BytesN::from_array(&c.env, &[2; 32]);
    c.client.propose_upgrade(&c.admin, &first);
    c.client.propose_upgrade(&c.admin, &second);
    c.client.cancel_upgrade(&c.admin);
    assert_eq!(
        c.client.try_execute_upgrade(&c.admin).unwrap_err().unwrap(),
        Error::NoPendingUpgrade
    );
    c.client.set_upgrade_delay(&c.admin, &(100 * DAY + GRACE));
    assert_eq!(
        c.client
            .try_set_upgrade_delay(&c.admin, &(90 * DAY))
            .unwrap_err()
            .unwrap(),
        Error::UpgradeDelayCannotDecrease
    );
}

#[test]
fn legacy_instance_token_is_read_until_rewritten_to_persistent_storage() {
    let c = Ctx::new();
    let legacy = Address::generate(&c.env);
    c.env.as_contract(&c.client.address, || {
        c.env.storage().instance().set(
            &DataKey::Token(legacy.clone()),
            &TokenLimits {
                min_amount: 1,
                max_amount: UNIT,
            },
        );
    });
    assert_eq!(c.client.get_token(&legacy).unwrap().max_amount, UNIT);
    c.client.set_token(&c.admin, &legacy, &2, &(2 * UNIT));
    assert_eq!(c.client.get_token(&legacy).unwrap().min_amount, 2);
}
