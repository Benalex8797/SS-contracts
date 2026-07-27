use soroban_sdk::{Address, Env, Symbol, Vec};

pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Issue #122: Fee recipient updated event
pub fn fee_recipient_updated(env: &Env, old_recipient: Option<Address>, new_recipient: &Address) {
    let topics = (Symbol::new(env, "fee_recipient_updated"),);
    env.events()
        .publish(topics, (old_recipient, new_recipient.clone()));
}

/// Issue #123: Enhanced structured payment distribution audit event.
/// Emits comprehensive distribution details for compliance and audit trails.
///
/// Event structure:
/// - Topics: (PaymentDistributed, escrow_address, invoice_id)
/// - Data: (
///     recipients: Vec<Address>,  // [seller, funder, fee_recipient]
///     amounts: Vec<i128>,        // [seller_amount, investor_amount, platform_fee, total_paid]
///     escrow_status: u32,        // Current escrow status
///     timestamp: u64             // Ledger timestamp
///   )
pub fn payment_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    escrow_status: u32,
) {
    let topics = (
        Symbol::new(env, "PaymentDistributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    // Issue #123: Add escrow_status and timestamp for comprehensive audit trail
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            recipients.clone(),
            amounts.clone(),
            escrow_status,
            timestamp,
        ),
    );
}

pub fn refund_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    funder: &Address,
    amount: i128,
) {
    let topics = (
        Symbol::new(env, "refund_distributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    env.events().publish(topics, (funder, amount));
}
