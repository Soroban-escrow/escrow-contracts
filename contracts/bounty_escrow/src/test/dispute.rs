use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Event,
};

use super::setup::*;
use crate::{events::EvidenceRecorded, Error, Status};

#[test]
fn maintainer_and_arbiter_can_open() {
    let c = Ctx::new();
    let a = c.fund(1);
    let b = c.fund(2);
    c.client.open_dispute(&c.maintainer, &a);
    c.client.open_dispute(&c.arbiter, &b);
    assert_eq!(c.client.get_bounty(&a).status, Status::Disputed);
    assert_eq!(c.client.get_bounty(&b).status, Status::Disputed);
}

#[test]
fn random_cannot_open() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c.client.try_open_dispute(&Address::generate(&c.env), &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
}

#[test]
fn cannot_open_twice_or_after_close() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.maintainer, &id);
    assert_eq!(
        c.client
            .try_open_dispute(&c.maintainer, &id)
            .unwrap_err()
            .unwrap(),
        Error::InvalidStatus
    );
    let id2 = c.fund(2);
    c.client.release(&c.maintainer, &id2, &c.winner);
    assert_eq!(
        c.client
            .try_open_dispute(&c.maintainer, &id2)
            .unwrap_err()
            .unwrap(),
        Error::InvalidStatus
    );
}

fn disputed(c: &Ctx) -> u64 {
    let id = c.fund(1);
    c.client.open_dispute(&c.arbiter, &id);
    id
}

#[test]
fn resolve_full_to_winner() {
    let c = Ctx::new();
    let id = disputed(&c);
    let m0 = c.bal(&c.token, &c.maintainer);
    c.client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &10_000);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);
    c.client.withdraw_fees(&c.treasury, &c.token);
    assert_eq!(c.bal(&c.token, &c.treasury), 3 * UNIT);
    assert_eq!(c.bal(&c.token, &c.maintainer), m0);
    assert_eq!(c.client.get_bounty(&id).status, Status::Resolved);
    assert_eq!(c.client.active_bounty(&REPO, &1), None);
    c.assert_invariant(&c.token);
}

#[test]
fn resolve_split_70_30() {
    let c = Ctx::new();
    let id = disputed(&c);
    let m0 = c.bal(&c.token, &c.maintainer);
    c.client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &7_000);
    // portion 70, fee 3% of 70 = 2.1, winner 67.9, maintainer 30
    assert_eq!(c.bal(&c.token, &c.winner), 679 * UNIT / 10);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 21 * UNIT / 10);
    c.client.withdraw_fees(&c.treasury, &c.token);
    assert_eq!(c.bal(&c.token, &c.treasury), 21 * UNIT / 10);
    assert_eq!(c.bal(&c.token, &c.maintainer), m0 + 30 * UNIT);
    c.assert_invariant(&c.token);
}

#[test]
fn resolve_zero_returns_all_to_maintainer() {
    let c = Ctx::new();
    let id = disputed(&c);
    let m0 = c.bal(&c.token, &c.maintainer);
    c.client.resolve(&c.arbiter, &id, &None, &0);
    assert_eq!(c.bal(&c.token, &c.maintainer), m0 + 100 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_bounty(&id).winner, None);
    c.assert_invariant(&c.token);
}

#[test]
fn resolve_half_with_zero_fee_and_dust() {
    let c = Ctx::new();
    let id = c.fund_with(1, 3 * UNIT + 1, true);
    c.client.open_dispute(&c.arbiter, &id);
    c.client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &5_000);
    c.assert_invariant(&c.token);
}

#[test]
fn resolve_winner_is_maintainer_rejected() {
    let c = Ctx::new();
    let id = disputed(&c);
    let r = c
        .client
        .try_resolve(&c.arbiter, &id, &Some(c.maintainer.clone()), &5_000);
    assert_eq!(r.unwrap_err().unwrap(), Error::SelfPayout);
}

#[test]
fn resolve_requires_winner_when_bps_positive_and_valid_bps() {
    let c = Ctx::new();
    let id = disputed(&c);
    assert_eq!(
        c.client
            .try_resolve(&c.arbiter, &id, &None, &1)
            .unwrap_err()
            .unwrap(),
        Error::InvalidSplit
    );
    let w = Some(c.winner.clone());
    assert_eq!(
        c.client
            .try_resolve(&c.arbiter, &id, &w, &10_001)
            .unwrap_err()
            .unwrap(),
        Error::InvalidSplit
    );
}

#[test]
fn resolve_by_non_arbiter_rejected() {
    let c = Ctx::new();
    let id = disputed(&c);
    for who in [c.maintainer.clone(), c.admin.clone(), c.relayer.clone()] {
        let r = c
            .client
            .try_resolve(&who, &id, &Some(c.winner.clone()), &5_000);
        assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
    }
}

#[test]
fn resolve_requires_disputed_status() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c
        .client
        .try_resolve(&c.arbiter, &id, &Some(c.winner.clone()), &5_000);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn resolve_works_while_paused() {
    let c = Ctx::new();
    let id = disputed(&c);
    c.client.pause(&c.admin);
    c.client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &5_000);
}

#[test]
fn resolve_twice_rejected() {
    let c = Ctx::new();
    let id = disputed(&c);
    c.client.resolve(&c.arbiter, &id, &None, &0);
    let r = c.client.try_resolve(&c.arbiter, &id, &None, &0);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn resolve_winner_is_contract_rejected() {
    let c = Ctx::new();
    let id = disputed(&c);
    let r = c
        .client
        .try_resolve(&c.arbiter, &id, &Some(c.client.address.clone()), &5_000);
    assert_eq!(r.unwrap_err().unwrap(), Error::SelfPayout);
}

#[test]
fn resolve_rejects_the_arbiter_as_winner() {
    let c = Ctx::new();
    let id = disputed(&c);
    let r = c
        .client
        .try_resolve(&c.arbiter, &id, &Some(c.arbiter.clone()), &5_000);
    assert_eq!(r.unwrap_err().unwrap(), Error::ConflictOfInterest);
    assert_eq!(c.client.get_bounty(&id).status, Status::Disputed);
}

#[test]
fn resolve_rejects_the_treasury_as_winner() {
    let c = Ctx::new();
    let id = disputed(&c);
    let r = c
        .client
        .try_resolve(&c.arbiter, &id, &Some(c.treasury.clone()), &5_000);
    assert_eq!(r.unwrap_err().unwrap(), Error::ConflictOfInterest);
    assert_eq!(c.client.get_bounty(&id).status, Status::Disputed);
}

#[test]
fn resolve_with_evidence_pays_out_and_records_the_hash() {
    let c = Ctx::new();
    let id = disputed(&c);
    let hash = BytesN::from_array(&c.env, &[7u8; 32]);
    c.client
        .resolve_with_evidence(&c.arbiter, &id, &Some(c.winner.clone()), &10_000, &hash);
    // Events of the last invocation: the evidence record comes last.
    let all = c.env.events().all();
    let events = all.events();
    assert_eq!(c.client.get_bounty(&id).status, Status::Resolved);
    let expected = EvidenceRecorded {
        id,
        arbiter: c.arbiter.clone(),
        evidence_hash: hash.clone(),
    };
    assert_eq!(
        events.last().unwrap(),
        &expected.to_xdr(&c.env, &c.client.address)
    );
}

#[test]
fn resolve_with_evidence_still_rejects_the_arbiter_as_winner() {
    let c = Ctx::new();
    let id = disputed(&c);
    let hash = BytesN::from_array(&c.env, &[7u8; 32]);
    let r = c.client.try_resolve_with_evidence(
        &c.arbiter,
        &id,
        &Some(c.arbiter.clone()),
        &5_000,
        &hash,
    );
    assert_eq!(r.unwrap_err().unwrap(), Error::ConflictOfInterest);
}

// ── Fee extremes and remove_token during a dispute (#107, #108) ──────────────

#[test]
fn resolve_at_max_fee_with_even_split_is_exact() {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &crate::fees::MAX_FEE_BPS);
    let id = disputed(&c);
    let maintainer_before = c.bal(&c.token, &c.maintainer);

    let settlement = c
        .client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &5_000);

    // Fee comes off the winner's half only: 50 UNIT portion, 10% fee.
    assert_eq!(settlement.winner_net, 45 * UNIT);
    assert_eq!(settlement.fee, 5 * UNIT);
    assert_eq!(settlement.maintainer_amount, 50 * UNIT);
    assert_eq!(c.bal(&c.token, &c.winner), 45 * UNIT);
    assert_eq!(
        c.bal(&c.token, &c.maintainer),
        maintainer_before + 50 * UNIT
    );
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 5 * UNIT);
    c.assert_invariant(&c.token);
}

// ── remove_token during a dispute (#108) ─────────────────────────────────────

#[test]
fn remove_token_during_dispute_does_not_block_resolve() {
    let c = Ctx::new();
    let id = disputed(&c);

    c.client.remove_token(&c.admin, &c.token);
    assert!(c.client.get_token(&c.token).is_none());

    let settlement = c
        .client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &10_000);

    assert_eq!(settlement.winner_net, 97 * UNIT);
    assert_eq!(settlement.fee, 3 * UNIT);
    assert_eq!(settlement.maintainer_amount, 0);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);
    c.assert_invariant(&c.token);
}

#[test]
fn remove_token_during_dispute_refund_path_returns_funds() {
    let c = Ctx::new();
    let maintainer_start = c.bal(&c.token, &c.maintainer);
    let id = disputed(&c);

    c.client.remove_token(&c.admin, &c.token);

    // The dispute expires back to Open, then the bounty is refunded after deadline + grace.
    c.set_time(c.now() + c.client.get_dispute_timeout());
    c.client.expire_dispute(&c.maintainer, &id);
    c.set_time(c.client.get_bounty(&id).deadline + GRACE);

    let settlement = c.client.refund(&c.maintainer, &id);

    assert_eq!(settlement.amount, 100 * UNIT);
    assert_eq!(c.bal(&c.token, &c.maintainer), maintainer_start);
    assert_eq!(c.contract_bal(&c.token), 0);
    c.assert_invariant(&c.token);
}

#[test]
fn expire_dispute_before_timeout_and_reopen_rejected() {
    let c = Ctx::new();
    let id = disputed(&c);
    let timeout = c.client.get_dispute_timeout();
    c.set_time(c.now() + timeout - 1);
    let r = c.client.try_expire_dispute(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::DisputeNotExpired);

    c.set_time(c.now() + 1);
    c.client.expire_dispute(&c.maintainer, &id);
    assert_eq!(c.client.get_bounty(&id).status, Status::Open);

    let r2 = c.client.try_open_dispute(&c.maintainer, &id);
    assert_eq!(r2.unwrap_err().unwrap(), Error::DisputeAlreadyUsed);
}

#[test]
fn dispute_opened_after_refund_window_blocks_refund_and_resolves() {
    let c = Ctx::new();
    let id = c.fund(1);

    // Advance past deadline + grace so the refund window is already open.
    c.set_time(c.client.refundable_at(&id) + 1);
    c.client.open_dispute(&c.arbiter, &id);
    assert_eq!(c.client.get_bounty(&id).status, Status::Disputed);

    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);

    let settlement = c
        .client
        .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &10_000);
    assert_eq!(settlement.winner_net, 97 * UNIT);
    assert_eq!(settlement.fee, 3 * UNIT);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);
    assert_eq!(c.client.get_bounty(&id).status, Status::Resolved);
    assert_eq!(c.client.active_bounty(&REPO, &1), None);
    c.assert_invariant(&c.token);
}
