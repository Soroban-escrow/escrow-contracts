use soroban_sdk::contracterror;

/// Stable numbering: never renumber after deploy.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    TokenNotAllowed = 5,
    AmountTooSmall = 6,
    AmountTooLarge = 7,
    InvalidDeadline = 8,
    IssueAlreadyFunded = 9,
    BountyNotFound = 10,
    InvalidStatus = 11,
    DeadlineNotReached = 12,
    SelfPayout = 13,
    InvalidSplit = 14,
    InvalidFee = 15,
    Overflow = 16,
    RelayerNotAllowed = 17,
    TopUpLimitReached = 18,
    DisputeNotExpired = 19,
    DisputeAlreadyUsed = 20,
    NothingToClaim = 21,
    AlreadyClaimed = 22,
    /// The arbiter named itself or the treasury as the resolution winner.
    ConflictOfInterest = 23,
    /// The token credited a different amount than requested (fee-on-transfer or non-standard token).
    UnexpectedTransferAmount = 24,
    /// The proposed new admin is already the active admin.
    SameAdmin = 25,
}
