use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Vec as SVec};

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
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.contract_bal(&c.token), 3 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 3 * UNIT);
    let b = c.client.get_bounty(&id);
    assert_eq!(b.status, Status::Released);
    assert_eq!(b.winner, Some(c.winner.clone()));
    c.assert_invariant(&c.token);
    assert_eq!(c.client.withdraw_fees(&c.treasury, &c.token), 3 * UNIT);
    assert_eq!(c.bal(&c.token, &c.treasury), 3 * UNIT);
    assert_eq!(c.contract_bal(&c.token), 0);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 0);
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
fn release_authorization_returns_each_authorized_role() {
    let c = Ctx::new();
    let id = c.fund_with(1, 100 * UNIT, false);
    let bounty = c.client.get_bounty(&id);
    let config = c.client.get_config();

    assert_eq!(
        crate::authorize_release(&c.maintainer, &bounty, &config),
        Ok(crate::ReleaseRole::Maintainer)
    );
    assert_eq!(
        crate::authorize_release(&c.relayer, &bounty, &config),
        Err(Error::RelayerNotAllowed)
    );

    let relayer_bounty = {
        let id = c.fund_with(2, 100 * UNIT, true);
        c.client.get_bounty(&id)
    };
    assert_eq!(
        crate::authorize_release(&c.relayer, &relayer_bounty, &config),
        Ok(crate::ReleaseRole::Relayer)
    );
    assert_eq!(
        crate::authorize_release(&Address::generate(&c.env), &bounty, &config),
        Err(Error::Unauthorized)
    );
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
fn release_keeps_working_while_paused() {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.pause(&c.admin);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.client.get_bounty(&id).status, Status::Released);
    let blocked = c.client.try_fund_bounty(
        &c.maintainer,
        &REPO,
        &2,
        &c.token,
        &UNIT,
        &(c.now() + 7 * DAY),
        &true,
    );
    assert_eq!(blocked.unwrap_err().unwrap(), Error::Paused);
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
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, fee);
    assert_eq!(c.bal(&c.token, &c.winner), amount - fee);
    c.client.withdraw_fees(&c.treasury, &c.token);
    assert_eq!(c.bal(&c.token, &c.treasury), fee);
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
fn tiny_amount_rounds_fee_to_zero() {
    let c = CtxBuilder::new().fee_bps(300).limits(1, 500 * UNIT).build();
    let id = c.fund_with(99, 33, true);

    let settlement = c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(settlement.fee, 0);
    assert_eq!(settlement.net, 33);
    assert_eq!(c.bal(&c.token, &c.winner), 33);
    assert_eq!(c.bal(&c.token, &c.treasury), 0);
    c.assert_invariant(&c.token);
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

// ── Cross-maintainer isolation (#100) ────────────────────────────────────────

#[test]
fn maintainer_b_cannot_release_or_dispute_maintainer_as_bounty() {
    let c = Ctx::new();
    let maintainer_b = c.second_maintainer();
    let id_a = c.fund(1);

    let r = c.client.try_release(&maintainer_b, &id_a, &c.winner);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);
    let r = c.client.try_open_dispute(&maintainer_b, &id_a);
    assert_eq!(r.unwrap_err().unwrap(), Error::Unauthorized);

    // B's own funds and bounty are untouched by the rejected attempts.
    assert_eq!(c.bal(&c.token, &maintainer_b), 10_000 * UNIT);
    assert_eq!(c.client.get_bounty(&id_a).status, Status::Open);

    // The real maintainer can still release normally.
    c.client.release(&c.maintainer, &id_a, &c.winner);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);
}

// ── Fee extremes end to end (#107) ───────────────────────────────────────────

// ── Relayer configured as its own bounty's maintainer (#101) ────────────────

#[test]
fn relayer_release_as_its_own_bounty_maintainer_takes_maintainer_path() {
    let c = Ctx::new();
    // The bounty opts out of relayer release...
    let id = c.fund_with(1, 100 * UNIT, false);
    // ...but the configured relayer is also this bounty's maintainer.
    c.client.set_relayer(&c.admin, &c.maintainer);

    // `authorize_release` checks `caller == maintainer` first, so this takes
    // the maintainer path and succeeds even though `relayer_allowed` is
    // false and no relayer rate limit applies here.
    let settlement = c.client.release(&c.maintainer, &id, &c.winner);
    assert!(!settlement.deferred);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);

    // SelfPayout is still enforced in this configuration.
    let id2 = c.fund_with(2, 100 * UNIT, false);
    let r = c.client.try_release(&c.maintainer, &id2, &c.maintainer);
    assert_eq!(r.unwrap_err().unwrap(), Error::SelfPayout);
}

#[test]
fn release_at_max_fee_pays_ninety_percent_and_accrues_ten_percent() {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &crate::fees::MAX_FEE_BPS);
    let id = c.fund(1);

    let settlement = c.client.release(&c.maintainer, &id, &c.winner);

    assert_eq!(settlement.net, 90 * UNIT);
    assert_eq!(settlement.fee, 10 * UNIT);
    assert!(!settlement.deferred);
    assert_eq!(c.bal(&c.token, &c.winner), 90 * UNIT);
    assert_eq!(c.client.get_totals(&c.token).accrued_fees, 10 * UNIT);
    c.assert_invariant(&c.token);
}

#[test]
fn claim_without_deferred_payout_returns_nothing_to_claim() {
    let c = Ctx::new();
    let id = c.fund(1);
    let r = c.client.try_claim(&c.winner, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::NothingToClaim);
}

#[test]
fn deferred_claim_succeeds_once_and_rejects_second_claim() {
    let c = Ctx::new();
    let id = c.fund(1);
    StellarAssetClient::new(&c.env, &c.token).set_authorized(&c.winner, &false);
    let settlement = c.client.release(&c.maintainer, &id, &c.winner);
    assert!(settlement.deferred);

    StellarAssetClient::new(&c.env, &c.token).set_authorized(&c.winner, &true);
    c.client.claim(&c.winner, &id);
    assert_eq!(c.bal(&c.token, &c.winner), 97 * UNIT);

    let r = c.client.try_claim(&c.winner, &id);
    assert_eq!(r.unwrap_err().unwrap(), Error::AlreadyClaimed);
}
