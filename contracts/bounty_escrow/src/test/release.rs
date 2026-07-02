use soroban_sdk::{testutils::Address as _, Address};

use super::setup::*;
use crate::{Error, Status};

#[test]
fn maintainer_releases_with_correct_fee() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 3 * UNIT);
    assert_eq!(c.contract_bal(&c.token), 0);
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Released);
    assert_eq!(b.winner, Some(c.winner.clone()));
    c.assert_invariant(&c.token);
}

#[test]
fn relayer_releases_when_allowed() {
    let c = Ctx::new();
    let id = c.fund_with(1, 100 * UNIT, true);
    c.client.release(&c.relayer, &id, &c.winner);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
}

#[test]
fn relayer_rejected_when_not_allowed() {
    let c = Ctx::new();
    let id = c.fund_with(1, 100 * UNIT, false);
    let r = c.client.try_release(&c.relayer, &id, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::RelayerNotAllowed);
    c.client.release(&c.maintainer, &id, &c.winner);
}

#[test]
fn random_address_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c
        .client
        .try_release(&Address::generate(&c.env), &id, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
}

#[test]
fn self_payout_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c.client.try_release(&c.maintainer, &id, &c.maintainer);
    assert_eq!(r.unwrap_err().unwrap(), Error::SelfPayout);
}

#[test]
fn release_after_deadline_before_refund_works() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.now() + 7 * DAY + GRACE + 10 * DAY);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.client.get_bounty(&id).status, Status::Released);
}

#[test]
fn double_release_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    let r = c.client.try_release(&c.maintainer, &id, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
}

#[test]
fn release_on_disputed_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.maintainer, &id);
    let r = c.client.try_release(&c.maintainer, &id, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn fee_snapshot_survives_fee_change_and_dust_goes_to_winner() {
    let c = Ctx::new();
    let id = c.fund_with(1, 33 * UNIT / 10 + 1, true); // fee rounds down
    c.client.set_fee(&c.admin, &1_000);
    c.client.release(&c.maintainer, &id, &c.winner);
    let amount = 33 * UNIT / 10 + 1;
    let fee = amount * 300 / 10_000;
    assert_eq!(c.bal(&c.token, &c.treasury), fee);
    assert_eq!(c.bal(&c.token, &c.winner), amount - fee);
}

#[test]
fn release_requires_caller_auth() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.env.set_auths(&[]);
    assert!(c.client.try_release(&c.maintainer, &id, &c.winner).is_err());
}

#[test]
fn zero_fee_pays_treasury_nothing() {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &0);
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.bal(&c.token, &c.winner), 100 * UNIT);
}
