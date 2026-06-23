use soroban_sdk::{contracterror, contracttype, Address, BytesN, Map};

pub use crate::contracts::storage_layout::DataKey;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    NotAdmin = 1,
    NotGuardian = 2,
    TaskAlreadyResolved = 3,
    DuplicateVote = 4,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequest {
    pub id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub requested_at_ledger: u32,
    pub is_executed: bool,
    pub is_cancelled: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: u64,
    pub votes: u32,
    pub is_done: bool,
    pub resolved_at: u64,
    pub total_weight_accrued: u64,
    pub is_cancelled: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardStream {
    pub task_id: u64,
    pub contributor: Address,
    pub drips_contract: Address,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Guardian(Address),
    Task(u64),
    Voted(u64, Address), // (task_id, guardian)
    Admin,
    DripsAddress,
    VaultAddress,
    RewardStream(u64), // keyed by task_id
    TokenAddress,
    LockThreshold,
    LockedBalance(Address),
    Lock, // re-entrancy mutex
    WeightThreshold,
    Reputation(Address), // u64 reputation score for a guardian
    FailureCount,        // circuit breaker failure counter
    Paused,              // circuit breaker pause flag
    StorageVersion,      // u32 storage schema version
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    NotAuthorized = 1,
    DuplicateVote = 2,
    TaskNotVerified = 3,
    StreamAlreadyActive = 4,
    DripsCallFailed = 5,
    Locked = 6,
    AlreadyInitialized = 7,
    NotInitialized = 8,
    InsufficientLockedBalance = 9,
    StillGuardian = 10,
    NotGuardian = 11,
    NoReputationScore = 12,
    ZeroWeightVote = 13,
    WeightOverflow = 14,
    ContractPaused = 15,
    EscrowUnavailable = 16,
}
