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

/// A single entry in a batch payment fanout.
///
/// Each entry represents one settled-payment distribution from a single escrow invoice.
/// The distributor contract must already hold the tokens for every entry before
/// `distribute_batch` is called.
///
/// Field names are kept ≤10 chars to satisfy Soroban's `contracttype` constraint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPaymentEntry {
    /// Escrow contract that authorises this distribution.
    pub escrow: soroban_sdk::Address,
    /// Invoice identifier within the escrow.
    pub inv_id: soroban_sdk::Symbol,
    /// Payment token contract.
    pub token: soroban_sdk::Address,
    /// Seller (invoice owner) — receives the face-value portion.
    pub seller: soroban_sdk::Address,
    /// Investor (funder) — receives the investor portion.
    pub funder: soroban_sdk::Address,
    /// Platform admin — receives the fee.
    pub admin: soroban_sdk::Address,
    /// Cumulative paid amount for this invoice (used to detect double-distribution).
    pub paid_amt: i128,
    /// Net amount to pay the seller for this settlement call.
    pub seller_amt: i128,
    /// Net amount to pay the investor for this settlement call.
    pub investor_amt: i128,
    /// Platform fee for this settlement call.
    pub fee_amt: i128,
    /// Escrow status after the payment (must be Funded=1 or Settled=2).
    pub status: u32,
}
