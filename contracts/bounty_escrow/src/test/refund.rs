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
    let settlement = c.client.refund(&c.maintainer, &id);
    assert_eq!(settlement.amount, 100 * UNIT);
    assert_eq!(c.bal(&c.token, &c.maintainer), before);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_bounty(&id).status, Status::Refunded);
    assert_eq!(c.client.active_bounty(&REPO, &1), None);
    c.assert_invariant(&c.token);
}

#[test]
fn refundable_at_matches_refund_boundary() {
    let c = Ctx::new();
    let id = c.fund(1);
    let expected = c.now() + 7 * DAY + GRACE;
    assert_eq!(c.client.refundable_at(&id), expected);
    c.set_time(expected);
    c.client.refund(&c.maintainer, &id);
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

#[test]
fn refund_blocked_when_dispute_opened_after_refund_window() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.client.refundable_at(&id) + 1);
    c.client.open_dispute(&c.arbiter, &id);
    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::InvalidStatus);
}

/// #100: one maintainer's authority must never reach another maintainer's
/// bounty. `Error::Unauthorized` on refund, with no effect on B's balance or
/// bounty A's status.
#[test]
fn maintainer_b_cannot_refund_maintainer_as_bounty() {
    let c = Ctx::new();
    let maintainer_b = c.second_maintainer();
    let id_a = c.fund(1);
    c.set_time(c.now() + 7 * DAY + GRACE);

    let r = c.client.try_refund(&maintainer_b, &id_a);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
    assert_eq!(c.bal(&c.token, &maintainer_b), 10_000 * UNIT);
    assert_eq!(c.client.get_bounty(&id_a).status, Status::Open);

    // The real maintainer can still refund normally.
    c.client.refund(&c.maintainer, &id_a);
    assert_eq!(c.client.get_bounty(&id_a).status, Status::Refunded);
}

/// #163: grace_secs is snapshotted at funding time.
/// Raising grace after a bounty is funded must not push that bounty's refund window.
#[test]
fn grace_snapshot_at_funding_time_isolates_existing_bounty() {
    let c = Ctx::new();
    // Fund the bounty; the bounty records grace = GRACE (3 days).
    let id = c.fund(1);
    assert_eq!(c.client.get_bounty(&id).grace_secs, GRACE);

    // Admin raises grace to 30 days after the bounty already exists.
    c.client.set_grace(&c.admin, &(30 * DAY));

    // The existing bounty's refundable_at is still deadline + original GRACE.
    let expected_refundable = c.now() + 7 * DAY + GRACE;
    assert_eq!(c.client.refundable_at(&id), expected_refundable);

    // Refund is still blocked one second before the original window.
    c.set_time(expected_refundable - 1);
    assert_eq!(
        c.client
            .try_refund(&c.maintainer, &id)
            .unwrap_err()
            .unwrap(),
        Error::DeadlineNotReached
    );

    // Refund succeeds at the original deadline + original grace.
    c.set_time(expected_refundable);
    c.client.refund(&c.maintainer, &id);
}

#[test]
fn refund_returns_overflow_when_deadline_plus_grace_overflows() {
    let c = Ctx::new();
    // Configure grace_secs so deadline + grace_secs overflows u64::MAX.
    c.client.set_grace(&c.admin, &u64::MAX);
    let id = c.fund(1);

    let r = c.client.try_refund(&c.maintainer, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::Overflow);
    assert_eq!(
        c.client.try_refundable_at(&id).unwrap_err().unwrap(),
        Error::Overflow
    );
    assert_eq!(c.client.get_bounty(&id).status, Status::Open);
    assert_eq!(c.client.active_bounty(&REPO, &1), Some(id));
    c.assert_invariant(&c.token);
}
