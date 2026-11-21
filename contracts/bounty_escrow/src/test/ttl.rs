use soroban_sdk::{
    testutils::{
        storage::{Instance as _, Persistent as _},
        Ledger,
    },
    vec,
};

use crate::{
    errors::Error,
    storage::DAY_LEDGERS,
    test::setup::{Ctx, DAY, GRACE, REPO, UNIT},
    types::{DataKey, Status},
};

#[test]
fn max_duration_bounty_survives_to_deadline_plus_grace() {
    let c = Ctx::new();
    let contract_id = c.client.address.clone();

    // Fund bounty with max duration (90 days)
    let max_duration = 90 * DAY;
    let deadline = c.now() + max_duration;
    let id = c.client.fund_bounty(
        &c.maintainer,
        &REPO,
        &101,
        &c.token,
        &(50 * UNIT),
        &deadline,
        &true,
    );

    // Initial TTL checks
    let bounty_key = DataKey::Bounty(id);
    let active_key = DataKey::ActiveByIssue(REPO, 101);
    let totals_key = DataKey::Totals(c.token.clone());

    let initial_bounty_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });
    let initial_active_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&active_key)
    });
    let initial_instance_ttl = c
        .env
        .as_contract(&contract_id, || c.env.storage().instance().get_ttl());

    assert!(initial_bounty_ttl >= 120 * DAY_LEDGERS);
    assert!(initial_active_ttl >= 120 * DAY_LEDGERS);
    assert!(initial_instance_ttl >= 120 * DAY_LEDGERS);

    // Advance ledger across 90 days + 3 days grace (93 days total) with NO intermediate calls
    let days_elapsed = 93u32;
    let seq_advance = days_elapsed * DAY_LEDGERS;
    let time_advance = (days_elapsed as u64) * DAY;

    c.env.ledger().with_mut(|li| {
        li.sequence_number += seq_advance;
        li.timestamp += time_advance;
    });

    // Entries must still be alive (remaining TTL should be ~120 - 93 = 27 days > 0)
    let mid_bounty_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });
    let mid_active_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&active_key)
    });
    let mid_totals_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&totals_key)
    });
    let mid_instance_ttl = c
        .env
        .as_contract(&contract_id, || c.env.storage().instance().get_ttl());

    assert!(mid_bounty_ttl > 0);
    assert!(mid_active_ttl > 0);
    assert!(mid_totals_ttl > 0);
    assert!(mid_instance_ttl > 0);

    // Config and totals survive and are readable
    let cfg = c.client.get_config();
    assert_eq!(cfg.grace_secs, GRACE);
    let totals = c.client.get_totals(&c.token);
    assert_eq!(totals.held, 50 * UNIT);

    // Active lock entry survived
    assert_eq!(c.client.active_bounty(&REPO, &101), Some(id));

    // Bounty is readable and refundable at deadline + grace
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Open);

    // Refund succeeds at deadline + grace
    let settlement = c.client.refund(&c.maintainer, &id);
    assert_eq!(settlement.amount, 50 * UNIT);

    // Active-issue lock is removed on terminal state
    assert_eq!(c.client.active_bounty(&REPO, &101), None);
    let active_exists = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().has(&active_key)
    });
    assert!(!active_exists);
}

#[test]
fn keeper_bump_is_permissionless_and_extends_ttl() {
    let c = Ctx::new();
    let contract_id = c.client.address.clone();
    let id = c.fund(201);
    let bounty_key = DataKey::Bounty(id);

    // Fast-forward 60 days
    let days_elapsed = 60u32;
    c.env.ledger().with_mut(|li| {
        li.sequence_number += days_elapsed * DAY_LEDGERS;
        li.timestamp += (days_elapsed as u64) * DAY;
    });

    let before_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });
    assert!(before_ttl <= 60 * DAY_LEDGERS + 10);

    // Anyone (unauthorized random address) can call bump
    let ids = vec![&c.env, id];
    c.client.bump(&ids);

    let after_ttl = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });
    assert!(after_ttl >= 120 * DAY_LEDGERS);

    // No state change has occurred other than TTL
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Open);
    assert_eq!(b.amount, 100 * UNIT);
}

#[test]
fn keeper_bump_rejects_oversized_list() {
    let c = Ctx::new();
    let mut ids = soroban_sdk::Vec::new(&c.env);
    for i in 0..51u64 {
        ids.push_back(i);
    }
    assert_eq!(
        c.client.try_bump(&ids).unwrap_err().unwrap(),
        Error::AmountTooLarge
    );
}

#[test]
fn terminal_bounties_do_not_bump_on_read() {
    let c = Ctx::new();
    let contract_id = c.client.address.clone();
    let id = c.fund(301);
    let bounty_key = DataKey::Bounty(id);

    // Release the bounty to terminal state
    c.client.release(&c.maintainer, &id, &c.winner);

    // Advance 40 days
    c.env.ledger().with_mut(|li| {
        li.sequence_number += 40 * DAY_LEDGERS;
        li.timestamp += 40 * DAY;
    });

    let ttl_before_read = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });

    // Read the terminal bounty via get_bounty
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Released);

    let ttl_after_read = c.env.as_contract(&contract_id, || {
        c.env.storage().persistent().get_ttl(&bounty_key)
    });

    // Reading terminal bounty must NOT bump TTL (rent stays bounded)
    assert_eq!(ttl_after_read, ttl_before_read);
}
