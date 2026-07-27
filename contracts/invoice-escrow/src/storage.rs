//! Storage helpers: instance for config, persistent for escrow data.

use soroban_sdk::Symbol;

use crate::types::{Config, EscrowData, StorageKey};

/// Load contract config from instance storage.
pub fn get_config(env: &soroban_sdk::Env) -> Option<Config> {
    env.storage().instance().get(&StorageKey::Config)
}

/// Save contract config to instance storage.
pub fn set_config(env: &soroban_sdk::Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
}

/// Load escrow data for an invoice from persistent storage.
pub fn get_escrow(env: &soroban_sdk::Env, inv_id: Symbol) -> Option<EscrowData> {
    env.storage().persistent().get(&StorageKey::Escrow(inv_id))
}

/// Save escrow data for an invoice to persistent storage.
pub fn set_escrow(env: &soroban_sdk::Env, inv_id: Symbol, data: &EscrowData) {
    env.storage()
        .persistent()
        .set(&StorageKey::Escrow(inv_id), data);
}

/// Check if an escrow exists for the given invoice.
pub fn has_escrow(env: &soroban_sdk::Env, inv_id: Symbol) -> bool {
    env.storage().persistent().has(&StorageKey::Escrow(inv_id))
}

/// Remove escrow data for an invoice from persistent storage (storage footprint cleanup).
pub fn remove_escrow(env: &soroban_sdk::Env, inv_id: Symbol) {
    env.storage().persistent().remove(&StorageKey::Escrow(inv_id));
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
