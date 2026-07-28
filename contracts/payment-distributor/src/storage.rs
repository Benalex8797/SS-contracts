use soroban_sdk::{Address, Env, Symbol};

use crate::types::{DistributionState, StorageKey};

/// Ledgers below which a `Distribution` persistent entry's TTL is extended
/// (~7 days at 5s/ledger). Issue #128.
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a `Distribution` persistent entry's TTL to when bumped
/// (~30 days at 5s/ledger). Issue #128.
const TTL_EXTEND_TO: u32 = 518_400;

/// Extend the TTL of a distribution's persistent storage entry so it survives
/// ledger pruning across the full lifetime of a long-lived invoice fee
/// schedule. Issue #128.
pub fn extend_ttl(env: &Env, escrow: &Address, invoice_id: &Symbol) {
    env.storage().persistent().extend_ttl(
        &StorageKey::Distribution(escrow.clone(), invoice_id.clone()),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

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
    let data = env.storage().persistent().get(&StorageKey::Distribution(
        escrow.clone(),
        invoice_id.clone(),
    ));
    if data.is_some() {
        extend_ttl(env, escrow, invoice_id);
    }
    data
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
    extend_ttl(env, escrow, invoice_id);
}

/// Set the whitelisted escrow contract address authorized to invoke
/// distribution entrypoints. Admin-only. Issue #121.
pub fn set_escrow_contract(env: &Env, escrow_contract: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::EscrowContract, escrow_contract);
}

/// Get the whitelisted escrow contract address, if configured. Issue #121.
pub fn get_escrow_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::EscrowContract)
}
