use soroban_sdk::{Address, Env, Symbol};

use crate::types::{DistributionState, StorageKey};

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn set_fee_recipient(env: &Env, fee_recipient: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::FeeRecipient, fee_recipient);
}

pub fn get_fee_recipient(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::FeeRecipient)
}

/// Whitelisted escrow contract address accessors (Issue #121).
pub fn set_escrow_contract(env: &Env, escrow_contract: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::EscrowContract, escrow_contract);
}

pub fn get_escrow_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::EscrowContract)
}

/// Re-entrancy guard flag accessors (Issue #127).
pub fn is_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Locked)
        .unwrap_or(false)
}

pub fn set_lock(env: &Env, locked: bool) {
    env.storage().instance().set(&StorageKey::Locked, &locked);
}

pub fn get_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
) -> Option<DistributionState> {
    env.storage().persistent().get(&StorageKey::Distribution(
        escrow.clone(),
        invoice_id.clone(),
    ))
}

pub fn set_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    state: &DistributionState,
) {
    env.storage().persistent().set(
        &StorageKey::Distribution(escrow.clone(), invoice_id.clone()),
        state,
    );
}
