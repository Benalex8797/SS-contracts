use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInit = 1,
    NotInit = 2,
    InvalidAmount = 3,
    Unauthorized = 4,
    InvalidEscrowStatus = 5,
    NothingToDistribute = 6,
    RefundAlreadyDistributed = 7,
    Overflow = 8,
    WrongDistributor = 9,
    /// Fee BPS exceeds maximum allowed value (10,000). Issue #124.
    InvalidBps = 10,
}
