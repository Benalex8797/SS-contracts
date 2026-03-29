//! Data types for the invoice token contract (SEP-41).
//! Storage key names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: token metadata and config.
    Metadata,
    /// Instance: total supply.
    TotalSupply,
    /// Persistent: balance by holder address.
    Balance(soroban_sdk::Address),
    /// Persistent: allowance (from, spender) -> AllowanceData.
    Allowance(soroban_sdk::Address, soroban_sdk::Address),
}

/// Token metadata and admin config (instance storage).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    /// Admin address (can mint, set minter, set transfer_locked).
    pub admin: soroban_sdk::Address,
    /// Escrow contract (or minter) address allowed to mint.
    pub minter: soroban_sdk::Address,
    /// Token name (e.g. "Invoice INV-001 Token").
    pub name: soroban_sdk::String,
    /// Token symbol (e.g. "INV001").
    pub symbol: soroban_sdk::String,
    /// Number of decimals (e.g. 7).
    pub decimals: u32,
    /// Invoice identifier this token represents (Symbol for storage efficiency).
    pub invoice_id: soroban_sdk::Symbol,
    /// If true, transfers restricted until settlement (admin can still transfer).
    pub transfer_locked: bool,
    /// Emergency pause flag for sensitive token operations.
    pub paused: bool,
}

/// Allowance entry: amount and expiration ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceData {
    pub amount: i128,
    pub expiration_ledger: u32,
}
