use std::{vec, vec::Vec};

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address};

use super::setup::*;
use crate::Status;

#[derive(Debug, Clone)]
enum Op {
    Fund {
        issue: u64,
        units: i128,
        tok: bool,
        relayer: bool,
    },
    Release {
        pick: usize,
        by_relayer: bool,
    },
    Refund {
        pick: usize,
    },
    Dispute {
        pick: usize,
    },
    Resolve {
        pick: usize,
        bps: u32,
    },
    Wait {
        days: u64,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u64..6, 1i128..400, any::<bool>(), any::<bool>()).prop_map(
            |(issue, units, tok, relayer)| Op::Fund {
                issue,
                units,
                tok,
                relayer
            }
        ),
        (0usize..8, any::<bool>()).prop_map(|(pick, by_relayer)| Op::Release { pick, by_relayer }),
        (0usize..8).prop_map(|pick| Op::Refund { pick }),
        (0usize..8).prop_map(|pick| Op::Dispute { pick }),
        (0usize..8, 0u32..=10_000).prop_map(|(pick, bps)| Op::Resolve { pick, bps }),
        (0u64..40).prop_map(|days| Op::Wait { days }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn funds_invariant_holds(ops in prop::collection::vec(op(), 1..40)) {
        let c = Ctx::new();
        let mut ids: Vec<u64> = vec![];
        let winner2 = Address::generate(&c.env);
        let mut total_supply = 0;
        let tokens = [c.token.clone(), c.token2.clone()];
        let everyone = [&c.maintainer, &c.winner, &winner2, &c.treasury, &c.client.address];
        let supply = |c: &Ctx| -> i128 {
            tokens.iter().map(|t| everyone.iter().map(|a| c.bal(t, a)).sum::<i128>()).sum()
        };
        total_supply += supply(&c);
        for o in ops {
            match o {
                Op::Fund { issue, units, tok, relayer } => {
                    let t = if tok { &c.token } else { &c.token2 };
                    if let Ok(Ok(id)) = c.client.try_fund_bounty(
                        &c.maintainer, &REPO, &issue, t, &(units * UNIT),
                        &(c.now() + 7 * DAY), &relayer) {
                        ids.push(id);
                    }
                }
                Op::Release { pick, by_relayer } if !ids.is_empty() => {
                    let id = ids[pick % ids.len()];
                    let who = if by_relayer { &c.relayer } else { &c.maintainer };
                    let _ = c.client.try_release(who, &id, &winner2);
                }
                Op::Refund { pick } if !ids.is_empty() => {
                    let _ = c.client.try_refund(&c.maintainer, &ids[pick % ids.len()]);
                }
                Op::Dispute { pick } if !ids.is_empty() => {
                    let _ = c.client.try_open_dispute(&c.arbiter, &ids[pick % ids.len()]);
                }
                Op::Resolve { pick, bps } if !ids.is_empty() => {
                    let w = if bps > 0 { Some(c.winner.clone()) } else { None };
                    let _ = c.client.try_resolve(&c.arbiter, &ids[pick % ids.len()], &w, &bps);
                }
                Op::Wait { days } => c.set_time(c.now() + days * DAY),
                _ => {}
            }
            c.assert_invariant(&c.token);
            c.assert_invariant(&c.token2);
            prop_assert_eq!(supply(&c), total_supply);
            // issue lock is held exactly while a bounty is Open/Disputed
            for id in &ids {
                let b = c.client.get_bounty(id);
                let live = matches!(b.status, Status::Open | Status::Disputed);
                if live {
                    prop_assert_eq!(c.client.active_bounty(&b.repo_id, &b.issue), Some(*id));
                }
            }
        }
    }
}
