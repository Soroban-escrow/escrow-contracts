use crate::Error;

/// #98: `Error` codes are part of the on-chain ABI once deployed — clients and
/// indexers persist them by number. This pins every variant's `u32` value so an
/// accidental reorder fails CI instead of silently shipping. If a variant is
/// added or renumbered, update this test and the `docs/CONTRACT.md` error
/// table in the same change.
#[test]
fn error_codes_are_pinned() {
    assert_eq!(Error::AlreadyInitialized as u32, 1);
    assert_eq!(Error::NotInitialized as u32, 2);
    assert_eq!(Error::Unauthorized as u32, 3);
    assert_eq!(Error::Paused as u32, 4);
    assert_eq!(Error::TokenNotAllowed as u32, 5);
    assert_eq!(Error::AmountTooSmall as u32, 6);
    assert_eq!(Error::AmountTooLarge as u32, 7);
    assert_eq!(Error::InvalidDeadline as u32, 8);
    assert_eq!(Error::IssueAlreadyFunded as u32, 9);
    assert_eq!(Error::BountyNotFound as u32, 10);
    assert_eq!(Error::InvalidStatus as u32, 11);
    assert_eq!(Error::DeadlineNotReached as u32, 12);
    assert_eq!(Error::SelfPayout as u32, 13);
    assert_eq!(Error::InvalidSplit as u32, 14);
    assert_eq!(Error::InvalidFee as u32, 15);
    assert_eq!(Error::Overflow as u32, 16);
    assert_eq!(Error::RelayerNotAllowed as u32, 17);
    assert_eq!(Error::TopUpLimitReached as u32, 18);
    assert_eq!(Error::DisputeNotExpired as u32, 19);
    assert_eq!(Error::DisputeAlreadyUsed as u32, 20);
    assert_eq!(Error::NothingToClaim as u32, 21);
    assert_eq!(Error::AlreadyClaimed as u32, 22);
    assert_eq!(Error::ConflictOfInterest as u32, 23);
    assert_eq!(Error::RelayerLimitExceeded as u32, 24);
    assert_eq!(Error::InvalidWinners as u32, 25);
    assert_eq!(Error::UpgradeNotReady as u32, 26);
    assert_eq!(Error::NoPendingUpgrade as u32, 27);
    assert_eq!(Error::UpgradeDelayCannotDecrease as u32, 28);
    assert_eq!(Error::InvalidKey as u32, 29);
    assert_eq!(Error::ContractAddressRejected as u32, 30);
    assert_eq!(Error::InvalidLimits as u32, 31);
}

enum ErrorReachability {
    Tested {
        file: &'static str,
        test_fn: &'static str,
        variant: &'static str,
    },
    Unreachable {
        variant: &'static str,
        reason: &'static str,
    },
}

/// Exhaustive mapping from every `Error` variant to the test that produces it
/// (or a documented reason why it is currently unreachable). Adding a new
/// variant to `Error` without updating this match fails compilation in CI.
const fn reachability(err: Error) -> ErrorReachability {
    match err {
        Error::AlreadyInitialized => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "init_once",
            variant: "AlreadyInitialized",
        },
        Error::NotInitialized => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "init_validates",
            variant: "NotInitialized",
        },
        Error::Unauthorized => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "set_admin_needs_both_auths",
            variant: "Unauthorized",
        },
        Error::Paused => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_when_paused",
            variant: "Paused",
        },
        Error::TokenNotAllowed => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_non_allowlisted_token",
            variant: "TokenNotAllowed",
        },
        Error::AmountTooSmall => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_below_min_and_above_max",
            variant: "AmountTooSmall",
        },
        Error::AmountTooLarge => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_below_min_and_above_max",
            variant: "AmountTooLarge",
        },
        Error::InvalidDeadline => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_deadline_too_soon_and_too_far",
            variant: "InvalidDeadline",
        },
        Error::IssueAlreadyFunded => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "rejects_second_bounty_on_same_issue",
            variant: "IssueAlreadyFunded",
        },
        Error::BountyNotFound => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "unknown_bounty_not_found",
            variant: "BountyNotFound",
        },
        Error::InvalidStatus => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "cannot_open_twice_or_after_close",
            variant: "InvalidStatus",
        },
        Error::DeadlineNotReached => ErrorReachability::Tested {
            file: "refund.rs",
            test_fn: "rejected_before_deadline_plus_grace",
            variant: "DeadlineNotReached",
        },
        Error::SelfPayout => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "resolve_winner_is_maintainer_rejected",
            variant: "SelfPayout",
        },
        Error::InvalidSplit => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "resolve_requires_winner_when_bps_positive_and_valid_bps",
            variant: "InvalidSplit",
        },
        Error::InvalidFee => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "init_validates",
            variant: "InvalidFee",
        },
        Error::Overflow => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "totals_overflow_on_second_fund_moves_no_tokens",
            variant: "Overflow",
        },
        Error::RelayerNotAllowed => ErrorReachability::Tested {
            file: "release.rs",
            test_fn: "release_authorization_returns_each_authorized_role",
            variant: "RelayerNotAllowed",
        },
        Error::TopUpLimitReached => ErrorReachability::Tested {
            file: "fund.rs",
            test_fn: "top_up_limit_reached_on_eleventh_top_up",
            variant: "TopUpLimitReached",
        },
        Error::DisputeNotExpired => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "expire_dispute_before_timeout_and_reopen_rejected",
            variant: "DisputeNotExpired",
        },
        Error::DisputeAlreadyUsed => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "expire_dispute_before_timeout_and_reopen_rejected",
            variant: "DisputeAlreadyUsed",
        },
        Error::NothingToClaim => ErrorReachability::Tested {
            file: "release.rs",
            test_fn: "claim_without_deferred_payout_returns_nothing_to_claim",
            variant: "NothingToClaim",
        },
        Error::AlreadyClaimed => ErrorReachability::Tested {
            file: "release.rs",
            test_fn: "deferred_claim_succeeds_once_and_rejects_second_claim",
            variant: "AlreadyClaimed",
        },
        Error::ConflictOfInterest => ErrorReachability::Tested {
            file: "dispute.rs",
            test_fn: "resolve_rejects_the_arbiter_as_winner",
            variant: "ConflictOfInterest",
        },
        Error::RelayerLimitExceeded => ErrorReachability::Tested {
            file: "release.rs",
            test_fn: "relayer_limit_caps_and_rolls_over_but_maintainer_bypasses",
            variant: "RelayerLimitExceeded",
        },
        Error::InvalidWinners => ErrorReachability::Tested {
            file: "release.rs",
            test_fn: "split_release_validates_count_sum_and_duplicates",
            variant: "InvalidWinners",
        },
        Error::UpgradeNotReady => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "upgrade_swaps_the_code_and_keeps_state",
            variant: "UpgradeNotReady",
        },
        Error::NoPendingUpgrade => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "upgrade_can_be_replaced_cancelled_and_delay_only_increased",
            variant: "NoPendingUpgrade",
        },
        Error::UpgradeDelayCannotDecrease => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "upgrade_can_be_replaced_cancelled_and_delay_only_increased",
            variant: "UpgradeDelayCannotDecrease",
        },
        Error::InvalidKey => ErrorReachability::Unreachable {
            variant: "InvalidKey",
            reason: "repo_id == 0 / issue == 0 validation is not currently enforced; retained for ABI stability",
        },
        Error::ContractAddressRejected => ErrorReachability::Unreachable {
            variant: "ContractAddressRejected",
            reason: "contract address rejection check is not currently emitted; retained for ABI stability",
        },
        Error::InvalidLimits => ErrorReachability::Tested {
            file: "admin.rs",
            test_fn: "set_token_validates",
            variant: "InvalidLimits",
        },
    }
}

const ALL_ERRORS: &[Error] = &[
    Error::AlreadyInitialized,
    Error::NotInitialized,
    Error::Unauthorized,
    Error::Paused,
    Error::TokenNotAllowed,
    Error::AmountTooSmall,
    Error::AmountTooLarge,
    Error::InvalidDeadline,
    Error::IssueAlreadyFunded,
    Error::BountyNotFound,
    Error::InvalidStatus,
    Error::DeadlineNotReached,
    Error::SelfPayout,
    Error::InvalidSplit,
    Error::InvalidFee,
    Error::Overflow,
    Error::RelayerNotAllowed,
    Error::TopUpLimitReached,
    Error::DisputeNotExpired,
    Error::DisputeAlreadyUsed,
    Error::NothingToClaim,
    Error::AlreadyClaimed,
    Error::ConflictOfInterest,
    Error::RelayerLimitExceeded,
    Error::InvalidWinners,
    Error::UpgradeNotReady,
    Error::NoPendingUpgrade,
    Error::UpgradeDelayCannotDecrease,
    Error::InvalidKey,
    Error::ContractAddressRejected,
    Error::InvalidLimits,
];

#[test]
fn every_error_variant_is_mapped_to_a_test() {
    use std::{format, fs, path::Path};

    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/test");
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let prod_files = [
        "lib.rs",
        "admin.rs",
        "bounty.rs",
        "fees.rs",
        "math.rs",
        "storage.rs",
        "types.rs",
        "events.rs",
    ];

    for (idx, &err) in ALL_ERRORS.iter().enumerate() {
        assert_eq!(err as u32, (idx + 1) as u32);
        match reachability(err) {
            ErrorReachability::Tested {
                file,
                test_fn,
                variant,
            } => {
                let content = fs::read_to_string(test_dir.join(file)).unwrap();
                assert!(
                    content.contains(&format!("fn {test_fn}(")),
                    "missing test function `{test_fn}` in {file}"
                );
                assert!(
                    content.contains(&format!("Error::{variant}")),
                    "missing `Error::{variant}` assertion in {file}"
                );
            }
            ErrorReachability::Unreachable { variant, reason } => {
                assert!(!reason.is_empty());
                let needle = format!("Error::{variant}");
                for pf in prod_files {
                    let content = fs::read_to_string(src_dir.join(pf)).unwrap();
                    let emitted = content
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("//"))
                        .any(|line| line.contains(&needle));
                    assert!(
                        !emitted,
                        "variant `{variant}` is emitted in {pf}; add a test and update reachability()"
                    );
                }
            }
        }
    }
}
