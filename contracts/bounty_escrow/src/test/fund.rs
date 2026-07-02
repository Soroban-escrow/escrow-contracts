use soroban_sdk::{testutils::Address as _, Address};

use super::setup::*;
use crate::{Error, Status};

#[test]
fn happy_path_stores_bounty_and_moves_tokens() {
    let c = Ctx::new();
    let before = c.bal(&c.token, &c.maintainer);
    let id = c.fund(7);
    assert_eq!(id, 1);
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Open);
    assert_eq!(b.amount, 100 * UNIT);
    assert_eq!(b.fee_bps, FEE_BPS);
    assert_eq!(b.issue, 7);
    assert_eq!(c.client.active_bounty(&REPO, &7), Some(id));
    assert_eq!(c.bal(&c.token, &c.maintainer), before - 100 * UNIT);
    assert_eq!(c.contract_bal(&c.token), 100 * UNIT);
    c.assert_invariant(&c.token);
}

#[test]
fn rejects_when_paused() {
    let c = Ctx::new();
    c.client.pause(&c.admin);
    let r = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
    assert_eq!(r.unwrap_err().unwrap(), Error::Paused);
}

#[test]
fn rejects_non_allowlisted_token() {
    let c = Ctx::new();
    let other = Address::generate(&c.env);
    let r = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &other,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
    assert_eq!(r.unwrap_err().unwrap(), Error::TokenNotAllowed);
}

#[test]
fn rejects_below_min_and_above_max() {
    let c = Ctx::new();
    let d = c.now() + 2 * DAY;
    let lo = c
        .client
        .try_fund_bounty(&c.maintainer, &REPO, &1, &c.token, &(UNIT - 1), &d, &true);
    assert_eq!(lo.unwrap_err().unwrap(), Error::AmountTooSmall);
    let hi = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &(500 * UNIT + 1),
        &d,
        &true,
    );
    assert_eq!(hi.unwrap_err().unwrap(), Error::AmountTooLarge);
    // boundaries are inclusive
    c.client
        .fund_bounty(&c.maintainer, &REPO, &1, &c.token, &UNIT, &d, &true);
    c.client
        .fund_bounty(&c.maintainer, &REPO, &2, &c.token, &(500 * UNIT), &d, &true);
}

#[test]
fn rejects_deadline_too_soon_and_too_far() {
    let c = Ctx::new();
    let soon = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &UNIT,
        &(c.now() + DAY - 1),
        &true,
    );
    assert_eq!(soon.unwrap_err().unwrap(), Error::InvalidDeadline);
    let far = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &UNIT,
        &(c.now() + 90 * DAY + 1),
        &true,
    );
    assert_eq!(far.unwrap_err().unwrap(), Error::InvalidDeadline);
    c.client.fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &UNIT,
        &(c.now() + DAY),
        &true,
    );
    c.client.fund_bounty(
        &c.maintainer,
        &REPO,
        &2,
        &c.token,
        &UNIT,
        &(c.now() + 90 * DAY),
        &true,
    );
}

#[test]
fn rejects_second_bounty_on_same_issue() {
    let c = Ctx::new();
    c.fund(5);
    let r = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &5,
        &c.token,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
    assert_eq!(r.unwrap_err().unwrap(), Error::IssueAlreadyFunded);
    // a different issue or repo is fine
    c.fund(6);
    c.client.fund_bounty(
        &c.maintainer,
        &(REPO + 1),
        &5,
        &c.token,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
}

#[test]
fn allows_new_bounty_after_previous_is_terminal() {
    let c = Ctx::new();
    let id = c.fund(5);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.client.active_bounty(&REPO, &5), None);
    let id2 = c.fund(5);
    assert_eq!(id2, id + 1);
}

#[test]
fn requires_maintainer_auth() {
    let c = Ctx::new();
    c.env.set_auths(&[]);
    let r = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &1,
        &c.token,
        &UNIT,
        &(c.now() + 2 * DAY),
        &true,
    );
    assert!(r.is_err());
    assert!(c.env.auths().is_empty());
}

#[test]
fn unknown_bounty_not_found() {
    let c = Ctx::new();
    assert_eq!(
        c.client.try_get_bounty(&99).unwrap_err().unwrap(),
        Error::BountyNotFound
    );
}
