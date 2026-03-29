//! Data types for the invoice escrow contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config.
    Config,
    /// Persistent: escrow data by invoice id.
    Escrow(soroban_sdk::Symbol),
    /// Persistent: funder amounts by (invoice_id, funder_address).
    FunderAmount(soroban_sdk::Symbol, soroban_sdk::Address),
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address (fee updates, platform recipient).
    pub admin: soroban_sdk::Address,
    /// Platform fee in basis points (e.g. 300 = 3%).
    pub fee_bps: u32,
    /// Optional payment distributor contract used for settlement/refund fan-out.
    pub payment_distributor: Option<soroban_sdk::Address>,
    /// Emergency pause flag for lifecycle-changing operations.
    pub paused: bool,
}

/// Lifecycle status of an escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    /// Created, awaiting funding.
    Created = 0,
    /// Funded by investor.
    Funded = 1,
    /// Payment recorded and distributed.
    Settled = 2,
    /// Refunded to investor after due date.
    Refunded = 3,
    /// Cancelled by seller while still in Created state (never funded).
    Cancelled = 4,
}

/// Per-invoice escrow data stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    /// Invoice identifier (Symbol, ≤10 chars when used as key).
    pub inv_id: soroban_sdk::Symbol,
    /// Seller (invoice owner).
    pub seller: soroban_sdk::Address,
    /// Debtor (authorized payer of the invoice).
    pub debtor: soroban_sdk::Address,
    /// Face value: what the debtor owes (amount to be paid at settlement).
    pub face_value: i128,
    /// Purchase price: total amount to be funded by all investors (discount applied here).
    pub purchase_price: i128,
    /// Total amount funded so far by all investors.
    pub funded_amt: i128,
    /// Primary funder address (MVP: single funder for now).
    pub funder: Option<soroban_sdk::Address>,
    /// Due date (ledger timestamp).
    pub due_dt: u64,
    /// Payment token contract address.
    pub token: soroban_sdk::Address,
    /// Invoice token contract address (ownership/claim).
    pub inv_token: soroban_sdk::Address,
    /// Amount already paid by payer.
    pub paid_amt: i128,
    /// Current status.
    pub status: EscrowStatus,
    /// Commitment hash: immutable on-chain anchor for off-chain invoice data (PDF hash, ERP ID, etc.).
    /// Set at creation, cannot be modified. SHA-256 hash (32 bytes).
    pub commitment: soroban_sdk::BytesN<32>,
}
