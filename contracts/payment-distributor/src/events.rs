use soroban_sdk::{Address, Env, Symbol, Vec};

pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

pub fn payment_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
) {
    let topics = (
        Symbol::new(env, "payment_distributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (recipients.clone(), amounts.clone()));
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
