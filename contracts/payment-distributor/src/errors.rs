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
}
