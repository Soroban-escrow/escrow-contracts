use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address};

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
fn get_bounty_by_issue_returns_active_only() {
    let c = Ctx::new();
    assert_eq!(c.client.get_bounty_by_issue(&REPO, &7), None);
    let id = c.fund(7);
    assert_eq!(c.client.get_bounty_by_issue(&REPO, &7).unwrap().id, id);
    c.client.release(&c.maintainer, &id, &c.winner);
    assert_eq!(c.client.get_bounty_by_issue(&REPO, &7), None);
}

#[test]
fn rejects_when_paused() {
    let c = Ctx::new();
    c.client.pause(&c.admin);
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
    assert_eq!(r.unwrap_err().unwrap(), Error::Paused);
}

#[test]
fn rejects_non_allowlisted_token() {
    let c = Ctx::new();
    let other = Address::generate(&c.env);
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: other.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
    assert_eq!(r.unwrap_err().unwrap(), Error::TokenNotAllowed);
}

#[test]
fn rejects_below_min_and_above_max() {
    let c = Ctx::new();
    let d = c.now() + 2 * DAY;
    let lo = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT - 1,
        deadline: d,
        relayer_allowed: true,
    });
    assert_eq!(lo.unwrap_err().unwrap(), Error::AmountTooSmall);
    let hi = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: 500 * UNIT + 1,
        deadline: d,
        relayer_allowed: true,
    });
    assert_eq!(hi.unwrap_err().unwrap(), Error::AmountTooLarge);
    // boundaries are inclusive
    c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: d,
        relayer_allowed: true,
    });
    c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 2,
        token: c.token.clone(),
        amount: 500 * UNIT,
        deadline: d,
        relayer_allowed: true,
    });
}

#[test]
fn rejects_deadline_too_soon_and_too_far() {
    let c = Ctx::new();
    let soon = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + DAY - 1,
        relayer_allowed: true,
    });
    assert_eq!(soon.unwrap_err().unwrap(), Error::InvalidDeadline);
    let far = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 90 * DAY + 1,
        relayer_allowed: true,
    });
    assert_eq!(far.unwrap_err().unwrap(), Error::InvalidDeadline);
    c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + DAY,
        relayer_allowed: true,
    });
    c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 2,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 90 * DAY,
        relayer_allowed: true,
    });
}

#[test]
fn rejects_second_bounty_on_same_issue() {
    let c = Ctx::new();
    c.fund(5);
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 5,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
    assert_eq!(r.unwrap_err().unwrap(), Error::IssueAlreadyFunded);
    // a different issue or repo is fine
    c.fund(6);
    c.client.fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO + 1,
        issue: 5,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
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
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 1,
        token: c.token.clone(),
        amount: UNIT,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
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

#[test]
fn bounty_count_tracks_created_bounties() {
    let c = Ctx::new();
    assert_eq!(c.client.bounty_count(), 0);
    c.fund(1);
    assert_eq!(c.client.bounty_count(), 1);
    c.fund(2);
    assert_eq!(c.client.bounty_count(), 2);
    c.fund(3);
    assert_eq!(c.client.bounty_count(), 3);
}

#[test]
fn totals_overflow_on_second_fund_moves_no_tokens() {
    let c = Ctx::new();
    c.client
        .set_token(&c.admin, &c.token, &UNIT, &i128::MAX, &None);
    // Ctx::new already minted 10_000 * UNIT to maintainer; top up to i128::MAX.
    let existing = c.bal(&c.token, &c.maintainer);
    StellarAssetClient::new(&c.env, &c.token).mint(&c.maintainer, &(i128::MAX - existing));

    let first = i128::MAX - 10 * UNIT;
    c.fund_with(1, first, true);

    let maintainer_before = c.bal(&c.token, &c.maintainer);
    let contract_before = c.contract_bal(&c.token);
    let totals_before = c.client.get_totals(&c.token);

    let second = 11 * UNIT;
    let r = c.client.try_fund_bounty(&crate::FundBountyParams {
        maintainer: c.maintainer.clone(),
        repo_id: REPO,
        issue: 2,
        token: c.token.clone(),
        amount: second,
        deadline: c.now() + 2 * DAY,
        relayer_allowed: true,
    });
    assert_eq!(r.unwrap_err().unwrap(), Error::Overflow);
    assert_eq!(c.bal(&c.token, &c.maintainer), maintainer_before);
    assert_eq!(c.contract_bal(&c.token), contract_before);
    assert_eq!(c.client.get_totals(&c.token), totals_before);
    assert_eq!(c.client.active_bounty(&REPO, &2), None);
    c.assert_invariant(&c.token);
}

#[test]
fn top_up_limit_reached_on_eleventh_top_up() {
    let c = Ctx::new();
    let id = c.fund(1);
    for _ in 0..10 {
        c.client.top_up(&c.maintainer, &id, &UNIT);
    }
    let r = c.client.try_top_up(&c.maintainer, &id, &UNIT);
    assert_eq!(r.unwrap_err().unwrap(), Error::TopUpLimitReached);
}

// ── Deadline window property test (#106) ─────────────────────────────────────

mod deadline_window {
    use proptest::prelude::*;
    use std::vec;
    use std::vec::Vec;

    use super::super::setup::*;
    use crate::Error;

    /// The specification, written independently of the contract code. An
    /// overflow of either bound is reported before the deadline itself is checked.
    fn expected(now: u64, min: u64, max: u64, deadline: u64) -> Result<(), Error> {
        match (now.checked_add(min), now.checked_add(max)) {
            (Some(earliest), Some(latest)) => {
                if deadline < earliest || deadline > latest {
                    Err(Error::InvalidDeadline)
                } else {
                    Ok(())
                }
            }
            _ => Err(Error::Overflow),
        }
    }

    /// Deadlines at and around the window edges, including values that wrap.
    fn interesting_deadlines(now: u64, min: u64, max: u64) -> Vec<u64> {
        let mut out = vec![0, 1, u64::MAX - 1, u64::MAX];
        if let Some(v) = now.checked_add(min) {
            out.push(v);
            if let Some(w) = v.checked_sub(1) {
                out.push(w);
            }
        }
        if let Some(v) = now.checked_add(max) {
            out.push(v);
            if let Some(w) = v.checked_add(1) {
                out.push(w);
            }
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn fund_bounty_accepts_exactly_the_specified_window(
            now in any::<u64>(),
            min in any::<u64>(),
            max in any::<u64>(),
            pick in any::<prop::sample::Index>(),
            random_deadline in any::<u64>(),
        ) {
            prop_assume!(min <= max);
            let deadline = {
                let candidates = interesting_deadlines(now, min, max);
                if pick.index(2) == 0 {
                    candidates[pick.index(candidates.len())]
                } else {
                    random_deadline
                }
            };

            let c = Ctx::new();
            c.set_time(now);
            c.client.set_durations(&c.admin, &min, &max);

            let got = c.client.try_fund_bounty(&crate::FundBountyParams {
                maintainer: c.maintainer.clone(),
                repo_id: REPO,
                issue: 1,
                token: c.token.clone(),
                amount: UNIT,
                deadline,
                relayer_allowed: false,
            });

            match expected(now, min, max, deadline) {
                Ok(()) => prop_assert!(matches!(got, Ok(Ok(_)))),
                Err(err) => prop_assert_eq!(got.unwrap_err().unwrap(), err),
            }
        }
    }

    #[test]
    fn overflow_near_u64_max_is_reported_not_wrapped() {
        let c = Ctx::new();
        c.set_time(u64::MAX - DAY);
        c.client.set_durations(&c.admin, &DAY, &(2 * DAY));

        let got = c.client.try_fund_bounty(&crate::FundBountyParams {
            maintainer: c.maintainer.clone(),
            repo_id: REPO,
            issue: 1,
            token: c.token.clone(),
            amount: UNIT,
            deadline: u64::MAX,
            relayer_allowed: false,
        });
        assert_eq!(got.unwrap_err().unwrap(), Error::Overflow);
    }
}
