//! Fee and split arithmetic.
//!
//! Every product is a checked multiply followed by an integer floor division by
//! 10_000. Dust is never burned: `fee`/`net` leave it with the receiver,
//! `portion` leaves it with the maintainer, and split callers assign it to the
//! first winner explicitly. Amounts are in the token's smallest unit (stroops
//! for the native asset). All numbers below are asserted by
//! `tests::worked_examples`, and the same tables appear in docs/CONTRACT.md.
//!
//! # Release — one fee on the full amount, at the `fee_bps` snapshotted at funding
//!
//! | amount | fee_bps | fee | net to winner |
//! |---:|---:|---:|---:|
//! | 1_000_000 | 0 | 0 | 1_000_000 |
//! | 1_000_000 | 300 | 30_000 | 970_000 |
//! | 1 | 300 | 0 | 1 |
//! | 33 | 300 | 0 | 33 |
//! | 34 | 300 | 1 | 33 |
//!
//! # Resolve — fee on the winner's portion only
//!
//! `portion = floor(amount * winner_bps / 10_000)`; the maintainer keeps
//! `amount - portion`. The fee is taken from the portion, not the whole bounty.
//!
//! | amount | winner_bps | fee_bps | portion | fee | winner net | maintainer |
//! |---:|---:|---:|---:|---:|---:|---:|
//! | 1_000_000 | 0 | 300 | 0 | 0 | 0 | 1_000_000 |
//! | 1_000_000 | 5_000 | 300 | 500_000 | 15_000 | 485_000 | 500_000 |
//! | 1_000_000 | 7_000 | 300 | 700_000 | 21_000 | 679_000 | 300_000 |
//! | 1_000_000 | 10_000 | 300 | 1_000_000 | 30_000 | 970_000 | 0 |
//! | 3 | 3_333 | 300 | 0 | 0 | 0 | 3 |
//!
//! # Release split — one fee on the full amount, dust to the first winner
//!
//! Each winner after the first gets `floor(net * share_bps / 10_000)`; the
//! first winner gets `net - sum(others)`, so all rounding dust goes there.
//!
//! | amount | fee_bps | shares | fee | payouts |
//! |---:|---:|---|---:|---|
//! | 1_000_000 | 300 | 5_000 / 5_000 | 30_000 | 485_000 + 485_000 |
//! | 10_001 | 300 | 3_334 / 3_333 / 3_333 | 300 | 3_235 + 3_233 + 3_233 |
//! | 1_000_000 | 0 | 2_000 / 3_000 / 5_000 | 0 | 200_000 + 300_000 + 500_000 |

use crate::{errors::Error, math::sub};

pub const BPS_DENOMINATOR: i128 = 10_000;
pub const MAX_FEE_BPS: u32 = 1_000;

fn mul_div(amount: i128, bps: u32) -> Result<i128, Error> {
    if amount < 0 {
        return Err(Error::Overflow);
    }

    amount
        .checked_mul(i128::from(bps))
        .map(|v| v / BPS_DENOMINATOR)
        .ok_or(Error::Overflow)
}

/// floor(amount * bps / 10_000)
pub fn fee(amount: i128, bps: u32) -> Result<i128, Error> {
    mul_div(amount, bps)
}

/// amount - fee; rounding dust goes to the receiver.
pub fn net(amount: i128, bps: u32) -> Result<i128, Error> {
    let fee = fee(amount, bps)?;
    sub(amount, fee)
}

/// floor(amount * winner_bps / 10_000); dust stays with the maintainer.
pub fn portion(amount: i128, winner_bps: u32) -> Result<i128, Error> {
    if winner_bps > 10_000 {
        return Err(Error::InvalidFee);
    }
    mul_div(amount, winner_bps)
}

/// floor(net * share_bps / 10_000); split callers assign remaining dust explicitly.
pub fn share(net: i128, share_bps: u32) -> Result<i128, Error> {
    mul_div(net, share_bps)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    reason = "tests may panic on unexpected results and use plain arithmetic to compute expected values"
)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn zero_and_one() {
        assert_eq!(fee(0, 300), Ok(0));
        assert_eq!(fee(1, 300), Ok(0));
        assert_eq!(net(1, 300), Ok(1));
    }

    #[test]
    fn bps_boundaries() {
        assert_eq!(fee(1_000_000, 0), Ok(0));
        assert_eq!(fee(1_000_000, 1), Ok(100));
        assert_eq!(fee(1_000_000, 1_000), Ok(100_000));
        assert_eq!(portion(1_000_000, 10_000), Ok(1_000_000));
        assert_eq!(portion(1_000_000, 5_000), Ok(500_000));
    }

    #[test]
    fn rounds_down() {
        assert_eq!(fee(33, 300), Ok(0));
        assert_eq!(fee(34, 300), Ok(1));
        assert_eq!(portion(3, 3_333), Ok(0));
    }

    #[test]
    fn large_values() {
        let m = i128::MAX / 10_000;
        assert_eq!(fee(m, 10_000), Ok(m));
        assert_eq!(fee(i128::MAX, 10_000), Err(Error::Overflow));
        assert_eq!(portion(i128::MAX, 2), Err(Error::Overflow));
    }

    #[test]
    fn net_plus_fee_is_amount_regressions() {
        for a in [0i128, 1, 7, 99, 12_345_678, 5_000_000_000] {
            for b in [0u32, 1, 300, 1_000] {
                assert_eq!(net(a, b).unwrap() + fee(a, b).unwrap(), a);
            }
        }
    }

    proptest! {
        #[test]
        fn net_plus_fee_is_amount(
            amount in 0i128..=(i128::MAX / BPS_DENOMINATOR),
            bps in 0u32..=MAX_FEE_BPS,
        ) {
            let f = fee(amount, bps).unwrap();
            let n = net(amount, bps).unwrap();
            prop_assert!(f >= 0);
            prop_assert!(n >= 0);
            prop_assert_eq!(n + f, amount);
        }

        #[test]
        fn fee_is_monotonic_in_amount(
            a1 in 0i128..=(i128::MAX / BPS_DENOMINATOR),
            a2 in 0i128..=(i128::MAX / BPS_DENOMINATOR),
            bps in 0u32..=MAX_FEE_BPS,
        ) {
            let (lo, hi) = if a1 <= a2 { (a1, a2) } else { (a2, a1) };
            prop_assert!(fee(lo, bps).unwrap() <= fee(hi, bps).unwrap());
        }

        #[test]
        fn fee_is_monotonic_in_bps(
            amount in 0i128..=(i128::MAX / BPS_DENOMINATOR),
            b1 in 0u32..=MAX_FEE_BPS,
            b2 in 0u32..=MAX_FEE_BPS,
        ) {
            let (lo, hi) = if b1 <= b2 { (b1, b2) } else { (b2, b1) };
            prop_assert!(fee(amount, lo).unwrap() <= fee(amount, hi).unwrap());
        }
    }

    #[test]
    fn rejects_negative_amount() {
        assert_eq!(mul_div(-1, 300), Err(Error::Overflow));
        assert_eq!(fee(-1, 300), Err(Error::Overflow));
        assert_eq!(portion(-1, 300), Err(Error::Overflow));
        assert_eq!(net(-1, 300), Err(Error::Overflow));
    }

    #[test]
    fn rejects_out_of_range_portion_bps() {
        assert_eq!(portion(1_000_000, 10_001), Err(Error::InvalidFee));
    }

    #[test]
    fn worked_examples() {
        // Release: one fee on the full amount. Tiny amounts round down; the
        // dust stays with the winner. fee = 0 is the zero-fee case.
        assert_eq!(fee(1_000_000, 0), Ok(0));
        assert_eq!(net(1_000_000, 0), Ok(1_000_000));
        assert_eq!(fee(1_000_000, 300), Ok(30_000));
        assert_eq!(net(1_000_000, 300), Ok(970_000));
        assert_eq!(fee(1, 300), Ok(0));
        assert_eq!(net(1, 300), Ok(1));
        assert_eq!(fee(33, 300), Ok(0));
        assert_eq!(net(33, 300), Ok(33));
        assert_eq!(fee(34, 300), Ok(1));
        assert_eq!(net(34, 300), Ok(33));

        // Resolve: portion -> fee -> winner net, maintainer keeps the rest.
        let resolve = |amount: i128, winner_bps: u32, bps: u32| {
            let p = portion(amount, winner_bps).unwrap();
            let f = fee(p, bps).unwrap();
            let winner_net = net(p, bps).unwrap();
            (p, f, winner_net, amount - p)
        };
        assert_eq!(resolve(1_000_000, 0, 300), (0, 0, 0, 1_000_000));
        assert_eq!(
            resolve(1_000_000, 5_000, 300),
            (500_000, 15_000, 485_000, 500_000)
        );
        assert_eq!(
            resolve(1_000_000, 7_000, 300),
            (700_000, 21_000, 679_000, 300_000)
        );
        assert_eq!(
            resolve(1_000_000, 10_000, 300),
            (1_000_000, 30_000, 970_000, 0)
        );
        assert_eq!(resolve(3, 3_333, 300), (0, 0, 0, 3));

        // Release split: one fee on the full amount, first winner takes dust.
        let n = net(1_000_000, 300).unwrap();
        let second = share(n, 5_000).unwrap();
        assert_eq!(fee(1_000_000, 300), Ok(30_000));
        assert_eq!(second, 485_000);
        assert_eq!(n - second, 485_000);

        let n = net(10_001, 300).unwrap();
        assert_eq!(n, 9_701);
        let s2 = share(n, 3_333).unwrap();
        let s3 = share(n, 3_333).unwrap();
        assert_eq!((s2, s3), (3_233, 3_233));
        assert_eq!(n - s2 - s3, 3_235);

        let n = net(1_000_000, 0).unwrap();
        assert_eq!(share(n, 2_000).unwrap(), 200_000);
        assert_eq!(share(n, 3_000).unwrap(), 300_000);
        assert_eq!(share(n, 5_000).unwrap(), 500_000);
    }
}
