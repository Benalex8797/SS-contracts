//! Event definitions for SEP-41 token (transfer, approve, mint, burn).

use soroban_sdk::{Address, Env, Symbol};

/// Emit transfer event (SEP-41: topics ["transfer", from, to], data amount).
pub fn transfer_event(env: &Env, from: &Address, to: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "transfer"), from, to), amount);
}

/// Emit approve event (SEP-41: topics ["approve", from, spender], data (amount, expiration_ledger)).
pub fn approve_event(
    env: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "approve"), from, spender),
        (amount, expiration_ledger),
    );
}

/// Emit mint event.
pub fn mint_event(env: &Env, to: &Address, amount: i128) {
    env.events().publish((Symbol::new(env, "mint"), to), amount);
}

/// Emit burn event (SEP-41: topics ["burn", from], data amount).
pub fn burn_event(env: &Env, from: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "burn"), from), amount);
}

/// Emit transfer_locked update event with previous and new values.
pub fn transfer_locked_updated_event(env: &Env, old_value: bool, new_value: bool) {
    env.events().publish(
        (Symbol::new(env, "transfer_locked_updated"),),
        (old_value, new_value),
    );
}

/// Emit minter update event with previous and new minter addresses.
pub fn minter_updated_event(env: &Env, old_minter: &Address, new_minter: &Address) {
    env.events().publish(
        (Symbol::new(env, "minter_updated"),),
        (old_minter, new_minter),
    );
}

/// Emit pause state updates.
pub fn paused_updated_event(env: &Env, old_value: bool, new_value: bool) {
    env.events().publish(
        (Symbol::new(env, "paused_updated"),),
        (old_value, new_value),
    );
}
