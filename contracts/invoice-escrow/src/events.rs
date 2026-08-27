#![allow(deprecated)]
//! Event definitions for state changes (escrow_created, escrow_funded, payment_settled).

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::types::EscrowStatus;

/// Publish a lifecycle transition event carrying the new status and ledger
/// timestamp, in addition to the narrower per-action events below. Lets
/// off-chain indexers reconstruct full escrow lifecycle history/metadata
/// from a single event stream instead of correlating five separate events.
pub fn escrow_status_changed(env: &Env, inv_id: Symbol, status: EscrowStatus, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "escrow_status_changed"),),
        (inv_id, status as u32, timestamp),
    );
}

pub fn escrow_created(
    env: &Env,
    inv_id: Symbol,
    seller: &Address,
    debtor: &Address,
    face_value: i128,
    purchase_price: i128,
    due_dt: u64,
    token: &Address,
    inv_token: &Address,
    commitment: &soroban_sdk::BytesN<32>,
    funding_milestone: Option<i128>,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_created"),),
        (
            inv_id.clone(),
            seller,
            debtor,
            face_value,
            purchase_price,
            due_dt,
            token,
            inv_token,
            commitment,
            funding_milestone,
        ),
    );
}

/// Publish escrow_funded event with partial funding info.
pub fn escrow_funded(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
    amount: i128,
    funded_amt: i128,
    purchase_price: i128,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_funded"),),
        (inv_id, funder, amount, funded_amt, purchase_price),
    );
}

/// Publish payment_settled event (amount, platform_fee, investor_amount).
pub fn payment_settled(
    env: &Env,
    inv_id: Symbol,
    amount: i128,
    platform_fee: i128,
    investor_amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "payment_settled"),),
        (inv_id, amount, platform_fee, investor_amount),
    );
}

/// Publish refund event.
pub fn escrow_refunded(env: &Env, inv_id: Symbol, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "escrow_refunded"),), (inv_id, amount));
}

/// Publish escrow_cancelled event (invoice_id, seller).
pub fn escrow_cancelled(env: &Env, inv_id: Symbol, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "escrow_cancelled"),), (inv_id, seller));
}

/// Publish escrow_funded event for a signed off-chain approval, including the consumed nonce.
pub fn escrow_funded_signed(env: &Env, inv_id: Symbol, buyer: &Address, amount: i128, nonce: u64) {
    env.events().publish(
        (Symbol::new(env, "escrow_fund_sig"),),
        (inv_id, buyer, amount, nonce),
    );
}

/// Publish escrow_cleaned_up event once a terminal escrow's storage has been reclaimed.
pub fn escrow_cleaned_up(env: &Env, inv_id: Symbol) {
    env.events()
        .publish((Symbol::new(env, "escrow_cleaned"),), inv_id);
}

/// Publish platform fee update event with old and new basis points.
pub fn platform_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (Symbol::new(env, "platform_fee_updated"),),
        (old_fee_bps, new_fee_bps),
    );
}

/// Publish payment distributor update event with previous and new distributor addresses.
pub fn payment_distributor_updated(
    env: &Env,
    had_previous_distributor: bool,
    new_distributor: &Address,
) {
    env.events().publish(
        (
            Symbol::new(env, "distributor_updated"),
            new_distributor.clone(),
        ),
        had_previous_distributor,
    );
}

/// Publish pause state updates.
pub fn paused_updated(env: &Env, old_paused: bool, new_paused: bool) {
    env.events().publish(
        (Symbol::new(env, "paused_updated"),),
        (old_paused, new_paused),
    );
}

/// Investment topped up event.
pub fn investment_topped_up(
    env: &Env,
    investor: &Address,
    invoice_id: BytesN<32>,
    additional_amount: i128,
    new_total_position: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "investment_topped_up"),
            investor.clone(),
            invoice_id.clone(),
        ),
        (additional_amount, new_total_position),
    );
}

/// Investment partially refunded event.
pub fn investment_partially_refunded(
    env: &Env,
    investor: &Address,
    invoice_id: BytesN<32>,
    amount_refunded: i128,
    remaining_position: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "investment_partially_refunded"),
            investor.clone(),
            invoice_id.clone(),
        ),
        (amount_refunded, remaining_position),
    );
}

/// Position transferred event.
pub fn position_transferred(
    env: &Env,
    from: &Address,
    to: &Address,
    invoice_id: BytesN<32>,
    position_amount: i128,
    price: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "position_transferred"),
            from.clone(),
            to.clone(),
            invoice_id.clone(),
        ),
        (position_amount, price),
    );
}

/// Funding finalised event.
pub fn funding_finalised(env: &Env, invoice_id: BytesN<32>, total_raised: i128, seller: &Address) {
    env.events().publish(
        (Symbol::new(env, "funding_finalised"), invoice_id.clone()),
        (total_raised, seller.clone()),
    );
}

/// Publish invoice_registered event with all parameters.
pub fn invoice_registered(
    env: &Env,
    invoice_id: &soroban_sdk::BytesN<32>,
    face_value: i128,
    funding_target: i128,
    yield_bps: u32,
    deadline_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "invoice_registered"),),
        (
            invoice_id.clone(),
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
        ),
    );
}

/// Publish investment_refunded event.
pub fn investment_refunded(
    env: &Env,
    investor: &Address,
    invoice_id: &soroban_sdk::BytesN<32>,
    amount_refunded: i128,
) {
    env.events().publish(
        (Symbol::new(env, "investment_refunded"),),
        (investor.clone(), invoice_id.clone(), amount_refunded),
    );
}

/// Publish settlement_paid event per investor.
pub fn settlement_paid(
    env: &Env,
    investor: &Address,
    invoice_id: &soroban_sdk::BytesN<32>,
    payout_amount: i128,
    yield_earned: i128,
) {
    env.events().publish(
        (Symbol::new(env, "settlement_paid"),),
        (investor.clone(), invoice_id.clone(), payout_amount, yield_earned),
    );
}
