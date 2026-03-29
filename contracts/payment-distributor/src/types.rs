use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Distribution(soroban_sdk::Address, soroban_sdk::Symbol),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionState {
    pub paid_distributed: i128,
    pub refund_distributed: bool,
}
