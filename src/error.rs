use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    NotAuthorized = 1,
    DuplicateVote = 2,
    TaskNotVerified = 3,
    StreamAlreadyActive = 4,
    DripsCallFailed = 5,
    AlreadyInitialized = 6,
    NotInitialized = 7,
    NoReputationScore = 8,
    ZeroWeightVote = 9,
    WeightOverflow = 10,
    InsufficientLockedBalance = 11,
    StillGuardian = 12,
    NotGuardian = 13,
    Locked = 14,
    ContractPaused = 15,
    EscrowUnavailable = 16,
    TaskCancelled = 17,
    BatchTooLarge = 18,
    TaskAlreadyArchived = 19,
    TaskNotFound = 20,
    TaskNotStale = 21,
}
