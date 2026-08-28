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
    /// Persistent: highest nonce consumed for a signed off-chain approval, by buyer address.
    Nonce(soroban_sdk::Address),
    /// Persistent: buyer whitelist flag by buyer address.
    BuyerWhitelist(soroban_sdk::Address),
    /// Persistent: funding invoice by BytesN<32> invoice id (new position management).
    Invoice(soroban_sdk::BytesN<32>),
    /// Persistent: investor position by (invoice_id BytesN<32>, investor address).
    InvestorPosition(soroban_sdk::BytesN<32>, soroban_sdk::Address),
    /// Instance: emergency multi-sig admin configuration.
    EmergencyConfig,
    /// Persistent: approvals collected for a given invoice's emergency release.
    EmergencyApprovals(soroban_sdk::Symbol),
    /// Instance: total count of escrows created for indexing.
    EscrowCount,
    /// Persistent: invoice_id indexed by sequential creation order.
    EscrowIdByIndex(u32),
    /// Persistent: invoice metadata and parameters by BytesN<32>.
    InvoiceRecord(soroban_sdk::BytesN<32>),
}

/// Registered invoice metadata and funding parameters stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceData {
    /// Invoice identifier (32 bytes).
    pub invoice_id: soroban_sdk::BytesN<32>,
    /// Face value: total amount owed by debtor.
    pub face_value: i128,
    /// Funding target: total funding amount to raise.
    pub funding_target: i128,
    /// Yield rate in basis points (1 to 5000).
    pub yield_bps: u32,
    /// Funding deadline (ledger sequence).
    pub deadline_ledger: u32,
    /// Total amount raised so far from investors.
    pub total_raised: i128,
    /// Current lifecycle status.
    pub status: EscrowStatus,
    /// List of investor addresses.
    pub investors: soroban_sdk::Vec<soroban_sdk::Address>,
}
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
    /// When true, `fund_escrow` requires the buyer to be on the whitelist.
    /// Defaults to false (opt-in) so existing deployments/tests are unaffected
    /// until an admin explicitly enables it.
    pub whitelist_enabled: bool,
    /// Minimum investment amount (stroops) accepted by `fund_escrow`.
    /// `0` disables the floor (only `amount > 0` is required). Completing the
    /// remaining capacity below this floor is always allowed.
    pub min_investment: i128,
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
    /// Cancelled by seller while in Created state (refunds partial funders if any).
    /// Cancelled by seller while still in Created state and never funded
    /// (locked out once any investor contribution has been received).
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
    /// Every funder that has contributed to this escrow so cleanup can prune their
    /// contribution records once the escrow reaches a terminal state.
    pub funders: soroban_sdk::Vec<soroban_sdk::Address>,
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
    /// Minimum chunk size required for each partial funding operation (except the final one).
    pub funding_milestone: Option<i128>,
    /// Commitment hash: immutable on-chain anchor for off-chain invoice data (PDF hash, ERP ID, etc.).
    /// Set at creation, cannot be modified. SHA-256 hash (32 bytes).
    pub commitment: soroban_sdk::BytesN<32>,
    /// Optional early-settlement discount hook. If present, payments made before
    /// `cutoff_date` receive a reduced effective face value.
    pub early_settlement: Option<EarlySettlementConfig>,
}

/// Status for BytesN<32> funding invoices (position management).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceStatus {
    Open = 0,
    Funded = 1,
}

/// Funding invoice for secondary market position management (BytesN<32> invoices).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingInvoice {
    /// Seller who will receive funds on finalisation.
    pub seller: soroban_sdk::Address,
    /// Total funding target.
    pub funding_target: i128,
    /// Total raised so far across all investors.
    pub total_raised: i128,
    /// Ledger deadline for funding (partial_refund only before this).
    pub deadline_ledger: u32,
    /// Minimum investment floor for remaining position after partial refund.
    pub min_investment: i128,
    /// Optional per-investor cap applied on top_up.
    pub per_investor_cap: Option<i128>,
    /// Current status.
    pub status: InvoiceStatus,
    /// Payment token contract address.
    pub token: soroban_sdk::Address,
}
/// Multi-signature configuration for emergency releases.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiSigConfig {
    /// Set of admin addresses authorized to approve emergency releases.
    pub admins: soroban_sdk::Vec<soroban_sdk::Address>,
    /// Number of approvals required to trigger the emergency release (N-of-M).
    pub threshold: u32,
}

/// Tracks which admins have approved an emergency release for a given invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyApprovals {
    /// List of admin addresses that have already approved this release.
    pub approvals: soroban_sdk::Vec<soroban_sdk::Address>,
}

/// Optional early-settlement discount hook configuration.
///
/// If set on an escrow, any `record_payment` whose ledger timestamp is strictly
/// before `cutoff_date` will have the effective face value reduced by
/// `discount_bps` basis points (e.g. 200 = 2%).  The discounted effective face
/// value is floored to `1` so the escrow can always be settled.
///
/// The discount applies uniformly across all partial payments made before the
/// cutoff; payments made on or after the cutoff are settled at the original
/// `face_value`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarlySettlementConfig {
    /// Discount in basis points applied to `face_value` when payment is early.
    /// Must be in [1, 9999] — a 0-bps discount is a no-op; 10000 bps (100%) is
    /// rejected to prevent the effective face value from collapsing to 0.
    pub discount_bps: u32,
    /// Ledger timestamp deadline (exclusive): payment is "early" iff
    /// `current_timestamp < cutoff_date`.
    pub cutoff_date: u64,
}
