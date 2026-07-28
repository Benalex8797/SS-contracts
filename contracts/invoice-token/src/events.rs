//! Event definitions for SEP-41 token (transfer, approve, mint, burn, fee, role, nonce, history).

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

/// Emit an allowance expiration extension (topics ["allowance_extended", from, spender], data new_expiration_ledger).
pub fn allowance_extended_event(
    env: &Env,
    from: &Address,
    spender: &Address,
    new_expiration_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "allow_extend"), from, spender),
        new_expiration_ledger,
    );
}

/// Emit a token-decimal configuration update.
pub fn decimals_updated_event(env: &Env, old_value: u32, new_value: u32) {
    env.events().publish(
        (Symbol::new(env, "decimals_updated"),),
        (old_value, new_value),
    );
}

/// Emit an explicit allowance revocation.
pub fn approval_revoked_event(env: &Env, from: &Address, spender: &Address) {
    env.events()
        .publish((Symbol::new(env, "approval_revoked"), from, spender), ());
}

/// Emit a nonce query event for audit/tracking purposes.
pub fn nonce_queried_event(env: &Env, account: &Address, nonce: u64) {
    env.events()
        .publish((Symbol::new(env, "nonce_queried"), account), nonce);
}

/// Emit a fee deduction event when a transfer fee is charged.
pub fn fee_deducted_event(env: &Env, from: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "fee_deducted"), from), amount);
}

/// Emit an ownership history record appended event.
pub fn history_appended_event(env: &Env, _caller: &Address, from: &Address, to: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "history_appended"), from, to), amount);
}

/// Emit a fee configuration update event.
pub fn fee_updated_event(env: &Env, old_bps: i128, new_bps: i128) {
    env.events()
        .publish((Symbol::new(env, "fee_updated"),), (old_bps, new_bps));
}

/// Emit a role admin update event.
pub fn role_admin_updated_event(env: &Env, role: &Symbol, old_admin: &Address, new_admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "role_admin_updated"),), (role, old_admin, new_admin));
}

/// Emit a role grant/revoke event.
pub fn role_granted_event(env: &Env, role: &Symbol, account: &Address, granted: bool) {
    env.events()
        .publish((Symbol::new(env, "role_granted"),), (role, account, granted));
}
