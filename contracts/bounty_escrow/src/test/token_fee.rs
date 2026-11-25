use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address};

use crate::{
    errors::Error,
    fees,
    test::setup::{Ctx, FEE_BPS, REPO, UNIT},
};

#[test]
fn token_fee_override_and_fallback() {
    let c = Ctx::new();

    // Setup token3 with explicit fee override (50 bps)
    let token3 = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();
    StellarAssetClient::new(&c.env, &token3).mint(&c.maintainer, &(10_000 * UNIT));
    c.client
        .set_token(&c.admin, &token3, &UNIT, &(500 * UNIT), &Some(50));

    // Token3 limits should reflect fee_bps = Some(50)
    let t3_limits = c.client.get_token(&token3).unwrap();
    assert_eq!(t3_limits.fee_bps, Some(50));

    // Fund bounty with token3 -> snapshots the override (50 bps)
    let id1 = c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 501,
        token: token3.clone(),
        amount: 100 * UNIT,
        deadline: c.now() + 7 * 86_400,
        relayer_allowed: true,
    });
    let b1 = c.client.get_bounty(&id1);
    assert_eq!(b1.fee_bps, 50);

    // Fund bounty with default token (token has fee_bps = None) -> snapshots config fee (FEE_BPS = 300)
    let id2 = c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 502,
        token: c.token.clone(),
        amount: 100 * UNIT,
        deadline: c.now() + 7 * 86_400,
        relayer_allowed: true,
    });
    let b2 = c.client.get_bounty(&id2);
    assert_eq!(b2.fee_bps, FEE_BPS);

    // Release both bounties to verify fee deducted matches the respective snapshots
    let release1 = c.client.release(&c.maintainer, &id1, &c.winner);
    let expected_fee1 = fees::fee(100 * UNIT, 50).unwrap();
    assert_eq!(release1.fee, expected_fee1);

    let release2 = c.client.release(&c.maintainer, &id2, &c.winner);
    let expected_fee2 = fees::fee(100 * UNIT, FEE_BPS).unwrap();
    assert_eq!(release2.fee, expected_fee2);
}

#[test]
fn token_fee_override_capped_by_max_fee() {
    let c = Ctx::new();
    let token = c
        .env
        .register_stellar_asset_contract_v2(Address::generate(&c.env))
        .address();

    // Setting fee > MAX_FEE_BPS (1000) fails with InvalidFee
    assert_eq!(
        c.client
            .try_set_token(&c.admin, &token, &UNIT, &(500 * UNIT), &Some(1001))
            .unwrap_err()
            .unwrap(),
        Error::InvalidFee
    );

    // Setting fee == MAX_FEE_BPS succeeds
    c.client
        .set_token(&c.admin, &token, &UNIT, &(500 * UNIT), &Some(1000));
    assert_eq!(c.client.get_token(&token).unwrap().fee_bps, Some(1000));
}
