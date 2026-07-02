use soroban_sdk::{testutils::Address as _, Address};

use super::setup::*;
use crate::{Error, Status};

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
