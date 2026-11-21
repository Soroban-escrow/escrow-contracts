use crate::{
    errors::Error,
    fees,
    test::setup::{Ctx, UNIT},
};

#[test]
fn quote_release_matches_real_release_payout() {
    let c = Ctx::new();

    // Test across several amounts including dust edge cases
    let amounts = [1, 2, 7, 33, 34, 100, 999, 10_000, 100 * UNIT];

    for (i, &amount) in amounts.iter().enumerate() {
        let issue = 1000 + (i as u64);
        let id = c.fund_with(issue, amount, false);

        let (quoted_net, quoted_fee) = c.client.quote_release(&id);
        let settlement = c.client.release(&c.maintainer, &id, &c.winner);

        assert_eq!(quoted_net, settlement.net);
        assert_eq!(quoted_fee, settlement.fee);
        assert_eq!(quoted_net + quoted_fee, amount);
    }
}

#[test]
fn quote_resolve_matches_real_resolve_payout() {
    let c = Ctx::new();

    // Amounts, splits (winner_bps), dust cases
    let cases = [
        (100 * UNIT, 5_000u32), // 50%
        (100 * UNIT, 10_000u32), // 100%
        (100 * UNIT, 0u32),      // 0%
        (33, 3_333u32),          // Dust case 1
        (34, 3_333u32),          // Dust case 2
        (7, 5_000u32),           // Dust case 3
        (10_000, 2_500u32),      // 25%
        (10_000, 7_500u32),      // 75%
    ];

    for (i, &(amount, winner_bps)) in cases.iter().enumerate() {
        let issue = 2000 + (i as u64);
        let id = c.fund_with(issue, amount, false);
        c.client.open_dispute(&c.maintainer, &id);

        let (q_winner_net, q_fee, q_maintainer) = c.client.quote_resolve(&id, &winner_bps);

        let winner_opt = if winner_bps > 0 {
            Some(c.winner.clone())
        } else {
            None
        };
        let settlement = c
            .client
            .resolve(&c.arbiter, &id, &winner_opt, &winner_bps);

        assert_eq!(q_winner_net, settlement.winner_net);
        assert_eq!(q_fee, settlement.fee);
        assert_eq!(q_maintainer, settlement.maintainer_amount);
        assert_eq!(q_winner_net + q_fee + q_maintainer, amount);
    }
}

#[test]
fn quote_fee_pre_funding_preview() {
    let c = Ctx::new();

    for &(amount, fee_bps) in &[(100 * UNIT, 300u32), (1_000, 100u32), (33, 300u32), (34, 300u32), (0, 300u32)] {
        let (net, fee) = c.client.quote_fee(&amount, &fee_bps);
        let expected_fee = fees::fee(amount, fee_bps).unwrap();
        let expected_net = fees::net(amount, fee_bps).unwrap();
        assert_eq!(net, expected_net);
        assert_eq!(fee, expected_fee);
        assert_eq!(net + fee, amount);
    }

    // Invalid fee exceeds MAX_FEE_BPS
    assert_eq!(
        c.client
            .try_quote_fee(&(100 * UNIT), &1_001)
            .unwrap_err()
            .unwrap(),
        Error::InvalidFee
    );

    // Negative amount rejected
    assert_eq!(
        c.client.try_quote_fee(&-1, &300).unwrap_err().unwrap(),
        Error::AmountTooSmall
    );
}

#[test]
fn quote_error_handling() {
    let c = Ctx::new();

    // Non-existent bounty returns BountyNotFound
    assert_eq!(
        c.client.try_quote_release(&999_999).unwrap_err().unwrap(),
        Error::BountyNotFound
    );
    assert_eq!(
        c.client
            .try_quote_resolve(&999_999, &5_000)
            .unwrap_err()
            .unwrap(),
        Error::BountyNotFound
    );

    // Invalid split > 10_000 returns InvalidSplit
    let id = c.fund(3001);
    assert_eq!(
        c.client
            .try_quote_resolve(&id, &10_001)
            .unwrap_err()
            .unwrap(),
        Error::InvalidSplit
    );
}
