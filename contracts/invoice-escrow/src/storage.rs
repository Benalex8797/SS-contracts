//! Storage helpers: instance for config, persistent for escrow data.

use soroban_sdk::{Address, Symbol};

use crate::types::{Config, EscrowData, StorageKey};

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
