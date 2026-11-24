use std::vec::Vec;

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    Address, BytesN,
};

use super::setup::*;
use crate::{BountyEscrow, BountyEscrowClient, DataKey, Error, TokenLimits, Totals};
use soroban_sdk::Event as _;

#[test]
fn init_once() {
    let c = Ctx::new();
    let a = Address::generate(&c.env);
    let r = c.client.try_init(&crate::InitParams {
        admin: a.clone(),
        relayer: a.clone(),
        arbiter: a.clone(),
        treasury: a.clone(),
        fee_bps: 0,
        grace_secs: 0,
        min_duration_secs: DAY,
        max_duration_secs: DAY,
    });
    assert_eq!(r.unwrap_err().unwrap(), Error::AlreadyInitialized);
    let cfg = c.client.get_config();
    assert_eq!(cfg.admin, c.admin);
    assert_eq!(cfg.fee_bps, FEE_BPS);
}

#[test]
fn init_emits_initialized_event() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &id);
    let a = Address::generate(&env);
    client.init(&crate::InitParams {
        admin: a.clone(),
        relayer: a.clone(),
        arbiter: a.clone(),
        treasury: a.clone(),
        fee_bps: 300,
        grace_secs: DAY,
        min_duration_secs: DAY,
        max_duration_secs: 90 * DAY,
    });
    let expected = crate::events::Initialized {
        admin: a.clone(),
        relayer: a.clone(),
        arbiter: a.clone(),
        treasury: a.clone(),
        fee_bps: 300,
        grace_secs: DAY,
        min_duration_secs: DAY,
        max_duration_secs: 90 * DAY,
        paused: false,
    };
    let all = env.events().all();
    assert!(all.events().contains(&expected.to_xdr(&env, &id)));
}

#[test]
fn init_validates() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let id = env.register(BountyEscrow, ());
    let cl = BountyEscrowClient::new(&env, &id);
    let a = Address::generate(&env);
    assert_eq!(
        cl.try_init(&crate::InitParams {
            admin: a.clone(),
            relayer: a.clone(),
            arbiter: a.clone(),
            treasury: a.clone(),
            fee_bps: 1_001,
            grace_secs: 0,
            min_duration_secs: 1,
            max_duration_secs: 2,
        })
            .unwrap_err()
            .unwrap(),
        Error::InvalidFee
    );
    assert_eq!(
        cl.try_init(&crate::InitParams {
            admin: a.clone(),
            relayer: a.clone(),
            arbiter: a.clone(),
            treasury: a.clone(),
            fee_bps: 300,
            grace_secs: 0,
            min_duration_secs: 5,
            max_duration_secs: 2,
        })
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
fn token_limit_change_only_affects_new_bounties() {
    let c = Ctx::new();
    let id = c.fund_with(1, 400 * UNIT, true);
    c.client
        .set_token(&c.admin, &c.token, &UNIT, &(100 * UNIT), &None);
    c.set_time(c.now() + 7 * DAY + GRACE);
    c.client.refund(&c.maintainer, &id);
    let result = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 2,
        token: c.token.clone(),
        amount: 101 * UNIT,
        deadline: c.now() + DAY,
        relayer_allowed: true,
    });
    assert_eq!(result.unwrap_err().unwrap(), Error::AmountTooLarge);
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
            .try_set_token(&old_admin, &c.token, &1, &UNIT, &None)
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
    c.client.set_token(&new_admin, &c.token, &1, &UNIT, &None);
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
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 3,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
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
            .try_set_token(&c.admin, &c.token, &0, &5, &None)
            .unwrap_err()
            .unwrap(),
        Error::InvalidLimits
    );
    assert_eq!(
        c.client
            .try_set_token(&c.admin, &c.token, &5, &4, &None)
            .unwrap_err()
            .unwrap(),
        Error::InvalidLimits
    );
    assert_eq!(
        c.client
            .try_set_token(&c.admin, &c.token, &1, &100, &Some(1001))
            .unwrap_err()
            .unwrap(),
        Error::InvalidFee
    );
    let t = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();
    StellarAssetClient::new(&c.env, &t).mint(&c.maintainer, &UNIT);
    c.client.set_token(&c.admin, &t, &1, &UNIT, &Some(150));
    let stored = c.client.get_token(&t).unwrap();
    assert_eq!(stored.max_amount, UNIT);
    assert_eq!(stored.fee_bps, Some(150));
}

#[test]
fn token_changes_emit_dedicated_events() {
    let c = Ctx::new();
    let token = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();
    c.client.set_token(&c.admin, &token, &1, &UNIT, &None);
    assert!(!c.env.events().all().events().is_empty());
    c.client.remove_token(&c.admin, &token);
    assert!(!c.env.events().all().events().is_empty());
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
            .try_set_token(&x, &c.token, &1, &2, &None)
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

#[test]
fn pause_and_unpause_are_idempotent_and_emit_events() {
    let c = Ctx::new();
    let paused_event = crate::events::Paused {}.to_xdr(&c.env, &c.client.address);
    let unpaused_event = crate::events::Unpaused {}.to_xdr(&c.env, &c.client.address);

    // Unpausing while already unpaused (initial state) succeeds and emits 1 Unpaused event.
    assert!(!c.client.get_config().paused);
    assert_eq!(c.client.try_unpause(&c.admin), Ok(Ok(())));
    let all = c.env.events().all();
    assert_eq!(all.events().len(), 1);
    assert_eq!(
        all.events().iter().filter(|e| *e == &unpaused_event).count(),
        1
    );
    assert!(!c.client.get_config().paused);

    // First pause sets paused = true and emits 1 Paused event.
    assert_eq!(c.client.try_pause(&c.admin), Ok(Ok(())));
    let all = c.env.events().all();
    assert_eq!(all.events().len(), 1);
    assert_eq!(
        all.events().iter().filter(|e| *e == &paused_event).count(),
        1
    );
    assert!(c.client.get_config().paused);

    // Second pause (while already paused) succeeds, keeps paused = true, and emits 1 Paused event.
    assert_eq!(c.client.try_pause(&c.admin), Ok(Ok(())));
    let all = c.env.events().all();
    assert_eq!(all.events().len(), 1);
    assert_eq!(
        all.events().iter().filter(|e| *e == &paused_event).count(),
        1
    );
    assert!(c.client.get_config().paused);

    // First unpause sets paused = false and emits 1 Unpaused event.
    assert_eq!(c.client.try_unpause(&c.admin), Ok(Ok(())));
    let all = c.env.events().all();
    assert_eq!(all.events().len(), 1);
    assert_eq!(
        all.events().iter().filter(|e| *e == &unpaused_event).count(),
        1
    );
    assert!(!c.client.get_config().paused);

    // Second unpause (while already unpaused) succeeds, keeps paused = false, and emits 1 Unpaused event.
    assert_eq!(c.client.try_unpause(&c.admin), Ok(Ok(())));
    let all = c.env.events().all();
    assert_eq!(all.events().len(), 1);
    assert_eq!(
        all.events().iter().filter(|e| *e == &unpaused_event).count(),
        1
    );
    assert!(!c.client.get_config().paused);
}

const WASM: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/bounty_escrow.wasm");

#[test]
fn upgrade_swaps_the_code_and_keeps_state() {
    let c = Ctx::new();
    let open_id = c.fund(1);
    let disputed_id = c.fund(2);
    c.client.open_dispute(&c.maintainer, &disputed_id);
    let hash = c.env.deployer().upload_contract_wasm(WASM);
    c.client.propose_upgrade(&c.admin, &hash);
    assert_eq!(
        c.client.try_execute_upgrade(&c.admin).unwrap_err().unwrap(),
        Error::UpgradeNotReady
    );
    c.set_time(c.now() + 90 * DAY + GRACE);
    c.client.execute_upgrade(&c.admin);
    assert_eq!(c.client.get_bounty(&open_id).amount, 100 * UNIT);
    assert_eq!(c.client.get_bounty(&open_id).status, Status::Open);
    assert_eq!(c.client.get_bounty(&disputed_id).status, Status::Disputed);
    assert_eq!(c.client.get_config().admin, c.admin);
    c.client.release(&c.maintainer, &open_id, &c.winner);
    c.client
        .resolve(&c.arbiter, &disputed_id, &Some(c.winner.clone()), &10_000);
    assert_eq!(c.client.get_bounty(&open_id).status, Status::Released);
    assert_eq!(
        c.client.get_bounty(&disputed_id).status,
        Status::Resolved
    );
    c.assert_invariant(&c.token);
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
                fee_bps: None,
            },
        );
    });
    assert_eq!(c.client.get_token(&legacy).unwrap().max_amount, UNIT);
    c.client
        .set_token(&c.admin, &legacy, &2, &(2 * UNIT), &None);
    assert_eq!(c.client.get_token(&legacy).unwrap().min_amount, 2);
}

/// #99: `get_totals` and `get_token` are read-only views. An unseen token must
/// read as zero/`None` without ever writing a record for it, and the same
/// goes for a token after `remove_token` — otherwise a view call could grow
/// storage for tokens nobody configured.
#[test]
fn totals_and_token_views_are_pure_for_unseen_and_removed_tokens() {
    let c = Ctx::new();
    let unseen = Address::generate(&c.env);

    assert_eq!(c.client.get_totals(&unseen), Totals::default());
    assert_eq!(c.client.get_token(&unseen), None);
    c.env.as_contract(&c.client.address, || {
        assert!(!c
            .env
            .storage()
            .persistent()
            .has(&DataKey::Totals(unseen.clone())));
        assert!(!c
            .env
            .storage()
            .persistent()
            .has(&DataKey::Token(unseen.clone())));
        assert!(!c
            .env
            .storage()
            .instance()
            .has(&DataKey::Token(unseen.clone())));
    });

    c.client.remove_token(&c.admin, &c.token);
    assert_eq!(c.client.get_token(&c.token), None);
    c.env.as_contract(&c.client.address, || {
        assert!(!c
            .env
            .storage()
            .persistent()
            .has(&DataKey::Token(c.token.clone())));
        assert!(!c
            .env
            .storage()
            .instance()
            .has(&DataKey::Token(c.token.clone())));
    });
}

#[test]
fn withdraw_fees_auth_and_sweep() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);

    // Random address cannot withdraw fees
    let stranger = Address::generate(&c.env);
    assert_eq!(
        c.client
            .try_withdraw_fees(&stranger, &c.token)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );

    // Admin can withdraw
    let withdrawn = c.client.withdraw_fees(&c.admin, &c.token);
    assert_eq!(withdrawn, 3 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 3 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 0);

    // Further withdrawal fails because accrued_fees is 0
    assert_eq!(
        c.client
            .try_withdraw_fees(&c.treasury, &c.token)
            .unwrap_err()
            .unwrap(),
        Error::AmountTooSmall
    );

    // Fund and release another to test treasury withdrawal
    let id2 = c.fund(2);
    c.client.release(&c.maintainer, &id2, &c.winner);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);

    // Treasury can withdraw
    let withdrawn2 = c.client.withdraw_fees(&c.treasury, &c.token);
    assert_eq!(withdrawn2, 3 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 6 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 0);
}

#[test]
fn all_state_changing_entry_points_before_init_return_not_initialized() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let id = env.register(BountyEscrow, ());
    let cl = BountyEscrowClient::new(&env, &id);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let tok = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    let winners = soroban_sdk::vec![
        &env,
        crate::WinnerShare {
            winner: b.clone(),
            share_bps: 10_000,
        }
    ];
    let bump_ids = soroban_sdk::vec![&env, 1u64];

    let cases: [(&str, Error); 31] = [
        // Admin entry points
        (
            "set_token",
            cl.try_set_token(&a, &tok, &1, &UNIT, &None)
                .unwrap_err()
                .unwrap(),
        ),
        (
            "remove_token",
            cl.try_remove_token(&a, &tok).unwrap_err().unwrap(),
        ),
        ("set_fee", cl.try_set_fee(&a, &100).unwrap_err().unwrap()),
        (
            "set_relayer",
            cl.try_set_relayer(&a, &b).unwrap_err().unwrap(),
        ),
        (
            "set_arbiter",
            cl.try_set_arbiter(&a, &b).unwrap_err().unwrap(),
        ),
        (
            "set_treasury",
            cl.try_set_treasury(&a, &b).unwrap_err().unwrap(),
        ),
        ("set_grace", cl.try_set_grace(&a, &1).unwrap_err().unwrap()),
        (
            "set_durations",
            cl.try_set_durations(&a, &1, &2).unwrap_err().unwrap(),
        ),
        ("set_admin", cl.try_set_admin(&a, &b).unwrap_err().unwrap()),
        ("pause", cl.try_pause(&a).unwrap_err().unwrap()),
        ("unpause", cl.try_unpause(&a).unwrap_err().unwrap()),
        (
            "set_dispute_timeout",
            cl.try_set_dispute_timeout(&a, &1).unwrap_err().unwrap(),
        ),
        (
            "set_relayer_limit",
            cl.try_set_relayer_limit(&a, &tok, &1, &UNIT)
                .unwrap_err()
                .unwrap(),
        ),
        (
            "set_upgrade_delay",
            cl.try_set_upgrade_delay(&a, &1).unwrap_err().unwrap(),
        ),
        (
            "propose_upgrade",
            cl.try_propose_upgrade(&a, &hash).unwrap_err().unwrap(),
        ),
        (
            "execute_upgrade",
            cl.try_execute_upgrade(&a).unwrap_err().unwrap(),
        ),
        (
            "cancel_upgrade",
            cl.try_cancel_upgrade(&a).unwrap_err().unwrap(),
        ),
        (
            "withdraw_fees",
            cl.try_withdraw_fees(&a, &tok).unwrap_err().unwrap(),
        ),
        ("upgrade", cl.try_upgrade(&a, &hash).unwrap_err().unwrap()),
        // Bounty lifecycle & keeper entry points
        (
            "fund_bounty",
            cl.try_fund_bounty(&crate::FundBountyParams {
                maintainer: a.clone(),
                repo_id: REPO,
                issue: 1,
                token: tok.clone(),
                amount: UNIT,
                deadline: DAY,
                relayer_allowed: true,
            })
            .unwrap_err()
            .unwrap(),
        ),
        (
            "release",
            cl.try_release(&a, &1, &b).unwrap_err().unwrap(),
        ),
        (
            "release_split",
            cl.try_release_split(&a, &1, &winners)
                .unwrap_err()
                .unwrap(),
        ),
        ("refund", cl.try_refund(&a, &1).unwrap_err().unwrap()),
        (
            "open_dispute",
            cl.try_open_dispute(&a, &1).unwrap_err().unwrap(),
        ),
        (
            "resolve",
            cl.try_resolve(&a, &1, &Some(b.clone()), &5_000)
                .unwrap_err()
                .unwrap(),
        ),
        (
            "resolve_with_evidence",
            cl.try_resolve_with_evidence(&a, &1, &Some(b), &5_000, &hash)
                .unwrap_err()
                .unwrap(),
        ),
        (
            "extend_deadline",
            cl.try_extend_deadline(&a, &1, &(2 * DAY))
                .unwrap_err()
                .unwrap(),
        ),
        (
            "top_up",
            cl.try_top_up(&a, &1, &UNIT).unwrap_err().unwrap(),
        ),
        (
            "expire_dispute",
            cl.try_expire_dispute(&a, &1).unwrap_err().unwrap(),
        ),
        ("claim", cl.try_claim(&a, &1).unwrap_err().unwrap()),
        ("bump", cl.try_bump(&bump_ids).unwrap_err().unwrap()),
    ];

    for (name, err) in cases {
        assert_eq!(
            err,
            Error::NotInitialized,
            "{name} before init should return NotInitialized"
        );
    }
}

/// Documents pre-init behaviour of every read-only view:
/// - `get_config`: returns `Error::NotInitialized`
/// - bounty-keyed views (`get_bounty`, `refundable_at`, `quote_release`, `quote_resolve`): return `Error::BountyNotFound`
/// - `quote_fee`: pure arithmetic, succeeds before init
/// - collection/default views (`active_bounty`, `get_totals`, `get_token`, `get_dispute_timeout`, `bounty_count`, `get_bounties`): return `None`, zero/default, or empty
#[test]
fn views_before_init_return_documented_errors_or_defaults() {
    let env = soroban_sdk::Env::default();
    let id = env.register(BountyEscrow, ());
    let cl = BountyEscrowClient::new(&env, &id);
    let tok = Address::generate(&env);

    // Views that return typed errors before init
    assert_eq!(
        cl.try_get_config().unwrap_err().unwrap(),
        Error::NotInitialized
    );
    assert_eq!(
        cl.try_get_bounty(&1).unwrap_err().unwrap(),
        Error::BountyNotFound
    );
    assert_eq!(
        cl.try_refundable_at(&1).unwrap_err().unwrap(),
        Error::BountyNotFound
    );
    assert_eq!(
        cl.try_quote_release(&1).unwrap_err().unwrap(),
        Error::BountyNotFound
    );
    assert_eq!(
        cl.try_quote_resolve(&1, &5_000).unwrap_err().unwrap(),
        Error::BountyNotFound
    );

    // Pure math view succeeds without contract state
    assert_eq!(cl.quote_fee(&1_000_000, &300), (970_000, 30_000));

    // Views that return None / defaults / empty before init
    assert_eq!(cl.active_bounty(&REPO, &1), None);
    assert_eq!(cl.get_totals(&tok), Totals::default());
    assert_eq!(cl.get_token(&tok), None);
    assert_eq!(
        cl.get_dispute_timeout(),
        crate::storage::DEFAULT_DISPUTE_TIMEOUT_SECS
    );
    assert_eq!(cl.bounty_count(), 0);
    assert!(cl.get_bounties(&1, &10).is_empty());
}
