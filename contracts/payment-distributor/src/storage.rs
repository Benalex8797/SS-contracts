use soroban_sdk::{Address, Env, Symbol};

use crate::types::{DistributionState, StorageKey};

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
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
