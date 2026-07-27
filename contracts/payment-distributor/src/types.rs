use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Distribution(soroban_sdk::Address, soroban_sdk::Symbol),
    /// Fee recipient address for platform fees (Issue #122).
    FeeRecipient,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionState {
    pub paid_distributed: i128,
    pub refund_distributed: bool,
}

/// Maximum allowed fee in basis points (100% = 10,000 BPS). Issue #124.
pub const MAX_FEE_BPS: u32 = 10_000;
