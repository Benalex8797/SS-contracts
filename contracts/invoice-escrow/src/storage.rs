//! Storage helpers: instance for config, persistent for escrow data, invoice records, and positions.

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::types::{
    Config, EmergencyApprovals, EscrowData, InvoiceData, MultiSigConfig, StorageKey,
};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger: 60 * 24 * 3600 / 5 = 1,036,800 ledgers).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

/// Extend the TTL of any persistent storage entry to at least 60 days in ledger units.
pub fn bump_persistent(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}


/// Load contract config from instance storage, bumping instance TTL.
pub fn get_config(env: &Env) -> Option<Config> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Config)
}

/// Save contract config to instance storage, bumping instance TTL.
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load escrow data for an invoice from persistent storage.
/// Extends the entry's TTL on every access so actively-used escrows never
/// expire mid-lifecycle regardless of how long between state transitions.
pub fn get_escrow(env: &Env, inv_id: Symbol) -> Option<EscrowData> {
    let key = StorageKey::Escrow(inv_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Save escrow data for an invoice to persistent storage, extending its TTL.
pub fn set_escrow(env: &Env, inv_id: Symbol, data: &EscrowData) {
    let key = StorageKey::Escrow(inv_id.clone());
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Check if an escrow exists for the given invoice.
pub fn has_escrow(env: &Env, inv_id: Symbol) -> bool {
    let key = StorageKey::Escrow(inv_id);
    let exists = env.storage().persistent().has(&key);
    if exists {
        bump_persistent(env, &key);
    }
    exists
}

/// Remove escrow data and all per-funder contribution records for an invoice from
/// persistent storage (storage footprint cleanup).
pub fn remove_escrow_state(
    env: &Env,
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
pub fn get_nonce(env: &Env, buyer: &Address) -> u64 {
    let key = StorageKey::Nonce(buyer.clone());
    let nonce = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    nonce
}

/// Record the highest nonce consumed for a buyer's signed off-chain approvals.
pub fn set_nonce(env: &Env, buyer: &Address, nonce: u64) {
    let key = StorageKey::Nonce(buyer.clone());
    env.storage().persistent().set(&key, &nonce);
    bump_persistent(env, &key);
}

/// Get the amount funded by a specific funder for an invoice.
pub fn get_funder_amount(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
) -> i128 {
    let key = StorageKey::FunderAmount(inv_id, funder.clone());
    let amount = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    amount
}

/// Set the amount funded by a specific funder for an invoice.
pub fn set_funder_amount(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
    amount: i128,
) {
    let key = StorageKey::FunderAmount(inv_id, funder.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump_persistent(env, &key);
    }
}

/// Whether `buyer` is whitelisted to fund (buy) escrows. Absent entry = not whitelisted.
pub fn is_whitelisted(env: &Env, buyer: &Address) -> bool {
    let key = StorageKey::BuyerWhitelist(buyer.clone());
    let whitelisted = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    whitelisted
}

/// Set (or clear) a buyer's whitelist status.
pub fn set_whitelisted(env: &Env, buyer: &Address, allowed: bool) {
    let key = StorageKey::BuyerWhitelist(buyer.clone());
    if allowed {
        env.storage().persistent().set(&key, &true);
        bump_persistent(env, &key);
    } else {
        env.storage().persistent().remove(&key);
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
pub fn get_emergency_config(env: &Env) -> Option<MultiSigConfig> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::EmergencyConfig)
}

/// Save the emergency multi-sig admin configuration.
pub fn set_emergency_config(env: &Env, config: &MultiSigConfig) {
    env.storage()
        .instance()
        .set(&StorageKey::EmergencyConfig, config);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the current emergency approvals for a given invoice.
pub fn get_emergency_approvals(env: &Env, inv_id: &Symbol) -> EmergencyApprovals {
    let key = StorageKey::EmergencyApprovals(inv_id.clone());
    let approvals = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(EmergencyApprovals {
            approvals: soroban_sdk::Vec::new(env),
        });
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    approvals
}

/// Save emergency approvals for a given invoice.
pub fn set_emergency_approvals(env: &Env, inv_id: &Symbol, approvals: &EmergencyApprovals) {
    let key = StorageKey::EmergencyApprovals(inv_id.clone());
    env.storage().persistent().set(&key, approvals);
    bump_persistent(env, &key);
}

/// Load invoice record by BytesN<32>.
pub fn get_invoice_record(env: &Env, inv_id: &BytesN<32>) -> Option<InvoiceData> {
    let key = StorageKey::InvoiceRecord(inv_id.clone());
    let data: Option<InvoiceData> = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Save invoice record by BytesN<32>, bumping its persistent TTL.
pub fn set_invoice_record(env: &Env, inv_id: &BytesN<32>, data: &InvoiceData) {
    let key = StorageKey::InvoiceRecord(inv_id.clone());
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Check if an invoice record exists for BytesN<32>.
pub fn has_invoice_record(env: &Env, inv_id: &BytesN<32>) -> bool {
    let key = StorageKey::InvoiceRecord(inv_id.clone());
    let exists = env.storage().persistent().has(&key);
    if exists {
        bump_persistent(env, &key);
    }
    exists
}

/// Get investor position for (invoice_id, investor).
pub fn get_investor_position(env: &Env, inv_id: &BytesN<32>, investor: &Address) -> i128 {
    let key = StorageKey::InvestorPosition(inv_id.clone(), investor.clone());
    let amount: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    amount
}

/// Set investor position for (invoice_id, investor).
pub fn set_investor_position(env: &Env, inv_id: &BytesN<32>, investor: &Address, amount: i128) {
    let key = StorageKey::InvestorPosition(inv_id.clone(), investor.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump_persistent(env, &key);
    }
}

/// Remove investor position for (invoice_id, investor).
pub fn remove_investor_position(env: &Env, inv_id: &BytesN<32>, investor: &Address) {
    let key = StorageKey::InvestorPosition(inv_id.clone(), investor.clone());
    env.storage().persistent().remove(&key);
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
