use soroban_sdk::{testutils::Address as _, Address, Vec as SVec};

use super::setup::*;
use crate::{Error, Status, WinnerShare};

fn split(c: &Ctx, shares: &[(Address, u32)]) -> SVec<WinnerShare> {
    let mut result = SVec::new(&c.env);
    for (winner, share_bps) in shares {
        result.push_back(WinnerShare {
            winner: winner.clone(),
            share_bps: *share_bps,
        });
    }
    result
}

#[test]
fn maintainer_releases_with_correct_fee() {
    let c = Ctx::new();
    let id = c.fund(1);
    let settlement = c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(settlement.net, 97 * UNIT);
    assert_eq!(settlement.fee, 3 * UNIT);
    assert!(!settlement.deferred);
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

#[test]
fn contract_address_as_winner_rejected() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c.client.try_release(&c.maintainer, &id, &c.client.address);
    assert_eq!(r.unwrap_err().unwrap(), Error::SelfPayout);
    assert_eq!(c.contract_bal(&c.token), 100 * UNIT);
}

#[test]
fn split_release_assigns_dust_to_first_winner() {
    let c = Ctx::new();
    let second = Address::generate(&c.env);
    let id = c.fund_with(1, UNIT + 1, true);
    c.client.release_split(
        &c.maintainer,
        &id,
        &split(&c, &[(c.winner.clone(), 3_333), (second.clone(), 6_667)]),
    );
    let net = UNIT + 1 - (UNIT + 1) * FEE_BPS as i128 / 10_000;
    let second_amount = net * 6_667 / 10_000;
    assert_eq!(c.bal(&c.token, &second), second_amount);
    assert_eq!(c.bal(&c.token, &c.winner), net - second_amount);
    c.assert_invariant(&c.token);
}

#[test]
fn split_release_validates_count_sum_and_duplicates() {
    let c = Ctx::new();
    let id = c.fund(1);
    assert_eq!(
        c.client
            .try_release_split(&c.maintainer, &id, &SVec::new(&c.env))
            .unwrap_err()
            .unwrap(),
        Error::InvalidWinners
    );
    let duplicate = split(&c, &[(c.winner.clone(), 5_000), (c.winner.clone(), 5_000)]);
    assert_eq!(
        c.client
            .try_release_split(&c.maintainer, &id, &duplicate)
            .unwrap_err()
            .unwrap(),
        Error::InvalidWinners
    );
    let bad_sum = split(&c, &[(c.winner.clone(), 9_999)]);
    assert_eq!(
        c.client
            .try_release_split(&c.maintainer, &id, &bad_sum)
            .unwrap_err()
            .unwrap(),
        Error::InvalidSplit
    );
}

#[test]
fn split_release_supports_five_winners() {
    let c = Ctx::new();
    let winners = [
        c.winner.clone(),
        Address::generate(&c.env),
        Address::generate(&c.env),
        Address::generate(&c.env),
        Address::generate(&c.env),
    ];
    let id = c.fund(1);
    c.client.release_split(
        &c.maintainer,
        &id,
        &split(
            &c,
            &[
                (winners[0].clone(), 2_000),
                (winners[1].clone(), 2_000),
                (winners[2].clone(), 2_000),
                (winners[3].clone(), 2_000),
                (winners[4].clone(), 2_000),
            ],
        ),
    );
    assert_eq!(
        winners
            .iter()
            .map(|winner| c.bal(&c.token, winner))
            .sum::<i128>(),
        97 * UNIT
    );
}

#[test]
fn relayer_limit_caps_and_rolls_over_but_maintainer_bypasses() {
    let c = Ctx::new();
    c.client
        .set_relayer_limit(&c.admin, &c.token, &DAY, &(100 * UNIT));
    let first = c.fund(1);
    c.client.release(&c.relayer, &first, &c.winner);
    let second = c.fund(2);
    assert_eq!(
        c.client
            .try_release(&c.relayer, &second, &c.winner)
            .unwrap_err()
            .unwrap(),
        Error::RelayerLimitExceeded
    );
    c.client.release(&c.maintainer, &second, &c.winner);
    let third = c.fund(3);
    c.set_time(c.now() + DAY);
    c.client.release(&c.relayer, &third, &c.winner);
}
