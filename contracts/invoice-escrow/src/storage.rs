//! Storage helpers: instance for config, persistent for escrow data.

use soroban_sdk::{Address, Symbol};

use crate::types::{Config, EmergencyApprovals, EscrowData, MultiSigConfig, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a persistent entry's TTL to when bumped (~30 days at 5s/ledger).
const TTL_EXTEND_TO: u32 = 518_400;

/// Extend the TTL of an escrow's persistent storage entry so it survives
/// ledger pruning across the full lifetime of a (potentially long-lived,
/// e.g. multi-month) invoice, not just the archival minimum.
pub fn extend_ttl(env: &soroban_sdk::Env, inv_id: Symbol) {
    env.storage().persistent().extend_ttl(
        &StorageKey::Escrow(inv_id),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

/// Load contract config from instance storage.
pub fn get_config(env: &soroban_sdk::Env) -> Option<Config> {
    env.storage().instance().get(&StorageKey::Config)
}

/// Save contract config to instance storage.
pub fn set_config(env: &soroban_sdk::Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
}

/// Load escrow data for an invoice from persistent storage.
/// Extends the entry's TTL on every access so actively-used escrows never
/// expire mid-lifecycle regardless of how long between state transitions.
pub fn get_escrow(env: &soroban_sdk::Env, inv_id: Symbol) -> Option<EscrowData> {
    let data = env
        .storage()
        .persistent()
        .get(&StorageKey::Escrow(inv_id.clone()));
    if data.is_some() {
        extend_ttl(env, inv_id);
    }
    data
}

/// Save escrow data for an invoice to persistent storage, extending its TTL.
pub fn set_escrow(env: &soroban_sdk::Env, inv_id: Symbol, data: &EscrowData) {
    env.storage()
        .persistent()
        .set(&StorageKey::Escrow(inv_id.clone()), data);
    extend_ttl(env, inv_id);
}

/// Check if an escrow exists for the given invoice.
pub fn has_escrow(env: &soroban_sdk::Env, inv_id: Symbol) -> bool {
    env.storage().persistent().has(&StorageKey::Escrow(inv_id))
}

/// Remove escrow data and all per-funder contribution records for an invoice from
/// persistent storage (storage footprint cleanup).
pub fn remove_escrow_state(
    env: &soroban_sdk::Env,
    inv_id: Symbol,
    funders: &soroban_sdk::Vec<Address>,
) {
    for funder in funders.iter() {
        env.storage()
            .persistent()
            .remove(&StorageKey::FunderAmount(inv_id.clone(), funder));
    }
    env.storage()
        .persistent()
        .remove(&StorageKey::Escrow(inv_id));
}

/// Get the highest nonce consumed so far for a buyer's signed off-chain approvals.
pub fn get_nonce(env: &soroban_sdk::Env, buyer: &soroban_sdk::Address) -> u64 {
    env.storage()
        .persistent()
        .get(&StorageKey::Nonce(buyer.clone()))
        .unwrap_or(0)
}

/// Record the highest nonce consumed for a buyer's signed off-chain approvals.
pub fn set_nonce(env: &soroban_sdk::Env, buyer: &soroban_sdk::Address, nonce: u64) {
    env.storage()
        .persistent()
        .set(&StorageKey::Nonce(buyer.clone()), &nonce);
}

/// Get the amount funded by a specific funder for an invoice.
pub fn get_funder_amount(
    env: &soroban_sdk::Env,
    inv_id: Symbol,
    funder: &soroban_sdk::Address,
) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::FunderAmount(inv_id, funder.clone()))
        .unwrap_or(0)
}

/// Set the amount funded by a specific funder for an invoice.
pub fn set_funder_amount(
    env: &soroban_sdk::Env,
    inv_id: Symbol,
    funder: &soroban_sdk::Address,
    amount: i128,
) {
    if amount == 0 {
        env.storage()
            .persistent()
            .remove(&StorageKey::FunderAmount(inv_id, funder.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&StorageKey::FunderAmount(inv_id, funder.clone()), &amount);
    }
}

/// Whether `buyer` is whitelisted to fund (buy) escrows. Absent entry = not whitelisted.
pub fn is_whitelisted(env: &soroban_sdk::Env, buyer: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::BuyerWhitelist(buyer.clone()))
        .unwrap_or(false)
}

/// Set (or clear) a buyer's whitelist status.
pub fn set_whitelisted(env: &soroban_sdk::Env, buyer: &Address, allowed: bool) {
    if allowed {
        env.storage()
            .persistent()
            .set(&StorageKey::BuyerWhitelist(buyer.clone()), &true);
    } else {
        env.storage()
            .persistent()
            .remove(&StorageKey::BuyerWhitelist(buyer.clone()));
    }
}

// ── Funding invoice (BytesN<32>) storage for position management ───────

use soroban_sdk::BytesN;

use crate::types::FundingInvoice;

pub fn get_invoice(env: &soroban_sdk::Env, invoice_id: BytesN<32>) -> Option<FundingInvoice> {
    env.storage()
        .persistent()
        .get(&StorageKey::Invoice(invoice_id))
}

pub fn set_invoice(env: &soroban_sdk::Env, invoice_id: BytesN<32>, invoice: &FundingInvoice) {
    env.storage()
        .persistent()
        .set(&StorageKey::Invoice(invoice_id), invoice);
}

pub fn has_invoice(env: &soroban_sdk::Env, invoice_id: BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::Invoice(invoice_id))
}

pub fn get_investor_position(
    env: &soroban_sdk::Env,
    invoice_id: BytesN<32>,
    investor: &Address,
) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::InvestorPosition(invoice_id, investor.clone()))
        .unwrap_or(0)
}

pub fn set_investor_position(
    env: &soroban_sdk::Env,
    invoice_id: BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    let key = StorageKey::InvestorPosition(invoice_id, investor.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
    }
/// Load the emergency multi-sig admin configuration.
pub fn get_emergency_config(env: &soroban_sdk::Env) -> Option<MultiSigConfig> {
    env.storage().instance().get(&StorageKey::EmergencyConfig)
}

/// Save the emergency multi-sig admin configuration.
pub fn set_emergency_config(env: &soroban_sdk::Env, config: &MultiSigConfig) {
    env.storage()
        .instance()
        .set(&StorageKey::EmergencyConfig, config);
}

/// Load the current emergency approvals for a given invoice.
pub fn get_emergency_approvals(env: &soroban_sdk::Env, inv_id: &Symbol) -> EmergencyApprovals {
    env.storage()
        .persistent()
        .get(&StorageKey::EmergencyApprovals(inv_id.clone()))
        .unwrap_or(EmergencyApprovals {
            approvals: soroban_sdk::Vec::new(env),
        })
}

/// Save emergency approvals for a given invoice.
pub fn set_emergency_approvals(env: &soroban_sdk::Env, inv_id: &Symbol, approvals: &EmergencyApprovals) {
    env.storage()
        .persistent()
        .set(&StorageKey::EmergencyApprovals(inv_id.clone()), approvals);
}

/// Get the total count of escrows created (for pagination).
pub fn get_escrow_count(env: &soroban_sdk::Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::EscrowCount)
        .unwrap_or(0)
}

/// Increment the escrow count and return the new value.
pub fn increment_escrow_count(env: &soroban_sdk::Env) -> u32 {
    let count = get_escrow_count(env);
    let new_count = count + 1;
    env.storage()
        .instance()
        .set(&StorageKey::EscrowCount, &new_count);
    new_count
}

/// Get the invoice_id at a specific index.
pub fn get_escrow_id_by_index(env: &soroban_sdk::Env, index: u32) -> Option<Symbol> {
    env.storage()
        .persistent()
        .get(&StorageKey::EscrowIdByIndex(index))
}

/// Set the invoice_id at a specific index.
pub fn set_escrow_id_by_index(env: &soroban_sdk::Env, index: u32, invoice_id: &Symbol) {
    env.storage()
        .persistent()
        .set(&StorageKey::EscrowIdByIndex(index), invoice_id);
}
