use soroban_sdk::{testutils::Address as _, Address};

use super::setup::*;
use crate::{Error, Status};

#[test]
fn rejected_before_deadline_plus_grace() {
    let c = Ctx::new();
    let id = c.fund(1);
    let open = c.now() + 7 * DAY + GRACE;
    c.set_time(open - 1);
    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::DeadlineNotReached);
}

#[test]
fn works_at_exactly_deadline_plus_grace_without_fee() {
    let c = Ctx::new();
    let before = c.bal(&c.token, &c.maintainer);
    let id = c.fund(1);
    c.set_time(c.now() + 7 * DAY + GRACE);
    c.client.refund(&c.maintainer, &id);
    assert_eq!(c.bal(&c.token, &c.maintainer), before);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_bounty(&id).status, Status::Refunded);
    assert_eq!(c.client.active_bounty(&REPO, &1), None);
    c.assert_invariant(&c.token);
}

#[test]
fn works_while_paused() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.pause(&c.admin);
    c.set_time(c.now() + 7 * DAY + GRACE);
    c.client.refund(&c.maintainer, &id);
}

#[test]
fn non_maintainer_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.now() + 30 * DAY);
    let r = c.client.try_refund(&Address::generate(&c.env), &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
}

#[test]
fn refund_after_release_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    c.set_time(c.now() + 30 * DAY);
    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn double_refund_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.now() + 30 * DAY);
    c.client.refund(&c.maintainer, &id);
    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn release_after_refund_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.now() + 30 * DAY);
    c.client.refund(&c.maintainer, &id);
    let r = c.client.try_release(&c.maintainer, &id, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

#[test]
fn refund_blocked_while_disputed() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.maintainer, &id);
    c.set_time(c.now() + 30 * DAY);
    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}
