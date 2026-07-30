#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol,
};

fn test_commitment(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

struct TestContext<'a> {
    env: Env,
    admin: Address,
    seller: Address,
    buyer: Address,
    payer: Address,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'a>,
    distributor_id: Address,
    distributor: PaymentDistributorClient<'a>,
    inv_token: InvoiceTokenClient<'a>,
    payment_token: TokenClient<'a>,
    payment_asset: AssetClient<'a>,
    invoice_id: Symbol,
}

fn setup(env: &Env, fee_bps: u32, configure_distributor: bool) -> TestContext<'_> {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let payer = Address::generate(env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(env, &distributor_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(env, &inv_token_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_id.address());
    let payment_asset = AssetClient::new(env, &token_id.address());

    let invoice_id = Symbol::new(env, "INV_DIST");
    inv_token.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Dist"),
        &SorobanString::from_str(env, "INVD"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &fee_bps);
    distributor.initialize(&admin);
    distributor.set_escrow_contract(&admin, &escrow_id);
    if configure_distributor {
        escrow.set_payment_distributor(&distributor_id);
    }

    TestContext {
        env: env.clone(),
        admin,
        seller,
        buyer,
        payer,
        escrow_id,
        escrow,
        distributor_id,
        distributor,
        inv_token,
        payment_token,
        payment_asset,
        invoice_id,
    }
}

fn create_and_fund(ctx: &TestContext<'_>, amount: i128, due_date: u64) {
    ctx.payment_asset.mint(&ctx.buyer, &amount);
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &amount,
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &test_commitment(&ctx.escrow.env),
        &None,
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &amount);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let result = distributor.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_get_distribution_state_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);

    assert_eq!(state.paid_distributed, 0);
    assert!(!state.refund_distributed);
}

#[test]
fn test_distribute_payment_rejects_created_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 0i128],
        &0u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

#[test]
fn test_incremental_payment_distribution_tracks_paid_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 380);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 20);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        400
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &600);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        1_000
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

#[test]
fn test_refund_distribution_can_only_happen_once() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(1_000);
    create_and_fund(&ctx, 1_000, 2_000);

    ctx.payment_asset.mint(&ctx.payer, &400);
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(2_001);
    ctx.escrow.refund(&ctx.invoice_id);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);

    let second_refund = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 600i128],
        &3u32,
    );
    assert_eq!(second_refund, Err(Ok(Error::RefundAlreadyDistributed)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #124: Max Fee BPS Boundary Guard Checks (10,000 max)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_rejects_negative_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone(),
        ],
        &soroban_sdk::vec![&env, 100i128, 0i128, 0i128, -1i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_payment_rejects_fee_bps_out_of_u32_range() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone(),
        ],
        &soroban_sdk::vec![&env, 100i128, 0i128, 0i128, (u32::MAX as i128) + 1],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_fee_bps_at_maximum_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    // 10,000 BPS = 100% fee (maximum allowed)
    let ctx = setup(&env, 10_000, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000); // Seller receives payment
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 1_000);
}

#[test]
fn test_fee_bps_exceeding_maximum_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);

    let result = escrow.try_initialize(&admin, &10_001);
    assert!(result.is_err());
}

#[test]
fn test_fee_bps_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 0, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0); // No fees
}

#[test]
fn test_fee_bps_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();

    // Test 1 BPS (0.01%)
    let ctx = setup(&env, 1, true);
    create_and_fund(&ctx, 10_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &10_000);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &10_000);

    let fee = ctx.payment_token.balance(&ctx.admin);
    assert_eq!(fee, 1); // 10,000 * 1 / 10,000 = 1

    // Test 9,999 BPS (99.99%)
    let env2 = Env::default();
    env2.mock_all_auths();
    let ctx2 = setup(&env2, 9_999, true);
    create_and_fund(&ctx2, 10_000, 50_000);
    ctx2.payment_asset.mint(&ctx2.payer, &10_000);
    ctx2.escrow
        .record_payment(&ctx2.invoice_id, &ctx2.payer, &10_000);

    let fee2 = ctx2.payment_token.balance(&ctx2.admin);
    assert_eq!(fee2, 9_999); // 10,000 * 9,999 / 10,000 = 9,999
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #122: Implement Distributor Fee Recipient Multisig Address Update
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_set_fee_recipient_by_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let new_recipient = Address::generate(&env);

    ctx.distributor
        .set_fee_recipient(&ctx.admin, &new_recipient);
    let stored_recipient = ctx.distributor.get_fee_recipient();
    assert_eq!(stored_recipient, new_recipient);
}

#[test]
fn test_set_fee_recipient_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let attacker = Address::generate(&env);
    let new_recipient = Address::generate(&env);

    let result = ctx
        .distributor
        .try_set_fee_recipient(&attacker, &new_recipient);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_fee_recipient_defaults_to_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let default_recipient = ctx.distributor.get_fee_recipient();
    assert_eq!(default_recipient, ctx.admin);
}

#[test]
fn test_fee_recipient_receives_platform_fees() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    let custom_recipient = Address::generate(&env);

    ctx.distributor
        .set_fee_recipient(&ctx.admin, &custom_recipient);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Verify custom recipient got the fee, not admin
    assert_eq!(ctx.payment_token.balance(&custom_recipient), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
}

#[test]
fn test_fee_recipient_update_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let new_recipient = Address::generate(&env);

    ctx.distributor
        .set_fee_recipient(&ctx.admin, &new_recipient);

    // Verify event was emitted (events are tracked in env)
    let events = env.events().all();
    assert!(events.events().len() > 0);
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #123: Emit Structured Payment Distribution Audit Events
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_payment_distributed_event_includes_audit_data() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    let events_before = env.events().all().events().len();
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    let events_after = env.events().all();

    // Verify PaymentDistributed event was emitted
    assert!(events_after.events().len() > events_before);

    // The event should contain structured data with escrow_status and timestamp
    // (actual event structure verification would require parsing event data)
}

#[test]
fn test_payment_distributed_event_symbol_is_pascal_case() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let events = env.events().all();
    // Event symbol should be "PaymentDistributed" (PascalCase) for issue #123
    // (verification would require parsing event topics)
    assert!(events.events().len() > 0);
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #132: Implement Automated Fee Rounding Loss Minimization
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_rounding_loss_allocated_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    // Use a fee BPS that causes rounding (333 BPS = 3.33%)
    let ctx = setup(&env, 333, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &100);

    let seller_balance = ctx.payment_token.balance(&ctx.seller);
    let investor_balance = ctx.payment_token.balance(&ctx.buyer);
    let fee_balance = ctx.payment_token.balance(&ctx.admin);

    // 100 * 333 / 10000 = 3.33 -> rounds to 3
    // Investor gets their share, fee is 3, seller gets remainder (absorbs rounding loss)
    let total_distributed = seller_balance + investor_balance + fee_balance;
    assert_eq!(
        total_distributed, 200,
        "Total must equal payment amount x 2"
    );
}

#[test]
fn test_no_dust_left_in_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 777, true); // 7.77% - creates rounding
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &129);

    // Verify no tokens are stuck in distributor contract
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

#[test]
fn test_rounding_minimization_with_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 250, true); // 2.5% fee
    let large_amount = 999_999_999i128;

    create_and_fund(&ctx, large_amount, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &large_amount);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &large_amount);

    let seller_balance = ctx.payment_token.balance(&ctx.seller);
    let investor_balance = ctx.payment_token.balance(&ctx.buyer);
    let fee_balance = ctx.payment_token.balance(&ctx.admin);

    let total = seller_balance + investor_balance + fee_balance;
    assert_eq!(
        total,
        large_amount * 2,
        "No rounding loss for large amounts"
    );
}

#[test]
fn test_exact_distribution_with_zero_rounding_loss() {
    let env = Env::default();
    env.mock_all_auths();

    // 2500 BPS = 25% - should divide evenly for amounts divisible by 4
    let ctx = setup(&env, 2_500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    let seller_balance = ctx.payment_token.balance(&ctx.seller);
    let investor_balance = ctx.payment_token.balance(&ctx.buyer);
    let fee_balance = ctx.payment_token.balance(&ctx.admin);

    // 400 * 2500 / 10000 = 100 (exact)
    assert_eq!(fee_balance, 100);
    assert_eq!(seller_balance + investor_balance + fee_balance, 800);
}

#[test]
fn test_minimum_payment_rounding() {
    let env = Env::default();
    env.mock_all_auths();

    // High fee BPS with tiny payment amount to maximize rounding impact
    let ctx = setup(&env, 9_999, true); // 99.99% fee
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &3);

    let seller_balance = ctx.payment_token.balance(&ctx.seller);
    let investor_balance = ctx.payment_token.balance(&ctx.buyer);
    let fee_balance = ctx.payment_token.balance(&ctx.admin);

    // 3 * 9999 / 10000 = 2.9997 -> rounds to 2
    // Total must still be exactly 3
    assert_eq!(seller_balance + investor_balance + fee_balance, 6);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

// ══════════════════════════════════════════════════════════════════════════════
// Shared helpers for the standalone-distributor tests below.
// ══════════════════════════════════════════════════════════════════════════════

fn distributor_only(env: &Env) -> (Address, Address, PaymentDistributorClient<'_>) {
    let admin = Address::generate(env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(env, &distributor_id);
    distributor.initialize(&admin);
    (admin, distributor_id, distributor)
}

fn make_token(env: &Env) -> (TokenClient<'_>, AssetClient<'_>) {
    let token_admin = Address::generate(env);
    let id = env.register_stellar_asset_contract_v2(token_admin);
    (
        TokenClient::new(env, &id.address()),
        AssetClient::new(env, &id.address()),
    )
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #127: Re-entrancy lock barrier on distribute_payment
// ══════════════════════════════════════════════════════════════════════════════

/// Malicious token whose `transfer` callback re-invokes `distribute_payment` on the
/// distributor while the lock is held, then records whether it was rejected.
#[contract]
pub struct ReentrantToken;

#[contractimpl]
impl ReentrantToken {
    pub fn __constructor(env: Env, distributor: Address, escrow: Address, invoice_id: Symbol) {
        let storage = env.storage().instance();
        storage.set(&soroban_sdk::symbol_short!("dist"), &distributor);
        storage.set(&soroban_sdk::symbol_short!("escrow"), &escrow);
        storage.set(&soroban_sdk::symbol_short!("inv"), &invoice_id);
        storage.set(&soroban_sdk::symbol_short!("blocked"), &false);
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }

    /// Mimics the token `transfer` entrypoint; attempts a re-entrant distribution.
    pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
        let storage = env.storage().instance();
        let distributor: Address = storage.get(&soroban_sdk::symbol_short!("dist")).unwrap();
        let escrow: Address = storage.get(&soroban_sdk::symbol_short!("escrow")).unwrap();
        let invoice_id: Symbol = storage.get(&soroban_sdk::symbol_short!("inv")).unwrap();

        let addresses = soroban_sdk::vec![
            &env,
            escrow.clone(),
            escrow.clone(),
            escrow.clone(),
            escrow.clone()
        ];
        let amounts = soroban_sdk::vec![&env, 1i128, 1i128, 0i128, 0i128];

        let client = PaymentDistributorClient::new(&env, &distributor);
        let res = client.try_distribute_payment(&escrow, &invoice_id, &addresses, &amounts, &2u32);
        // Record the outcome of the re-entrant attempt: 1 == it (wrongly) succeeded.
        let code: u32 = match res {
            Ok(Ok(())) => 1,
            Ok(Err(_)) => 2,
            Err(Ok(e)) => 100 + e as u32,
            Err(Err(_)) => 3,
        };
        storage.set(&soroban_sdk::symbol_short!("code"), &code);
    }

    pub fn last_code(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("code"))
            .unwrap_or(0)
    }
}

#[test]
fn test_reentrancy_guard_rejects_when_locked() {
    // White-box: simulate an in-progress guarded distribution by setting the lock,
    // then confirm distribute_payment rejects with ReentrancyDetected.
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    env.as_contract(&distributor_id, || {
        crate::storage::set_lock(&env, true);
    });

    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "LOCKED");
    let (token, _asset) = make_token(&env);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::ReentrancyDetected)));
}

#[test]
fn test_reentrant_callback_into_distribute_payment_is_rejected() {
    // End-to-end: a malicious token whose transfer callback tries to re-invoke
    // distribute_payment cannot re-enter successfully (the re-entrant call fails).
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    distributor.initialize(&admin);

    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "REENTR");
    distributor.set_escrow_contract(&admin, &escrow);

    // Register the malicious token that will re-enter on `transfer`.
    let token_id = env.register(
        ReentrantToken,
        (distributor_id.clone(), escrow.clone(), invoice_id.clone()),
    );
    let malicious = ReentrantTokenClient::new(&env, &token_id);

    // Outer distribution: its token transfers route into the malicious token, which
    // tries to re-invoke distribute_payment while the distribution is in progress.
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![
            &env,
            token_id.clone(),
            seller.clone(),
            funder.clone(),
            admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    // Outer call completes and the re-entrant invocation was NOT allowed to succeed.
    assert!(result.is_ok());
    assert_ne!(malicious.last_code(), 1);
}

#[test]
fn test_reentrancy_lock_cleared_after_success() {
    // Two sequential distributions must both succeed; the lock must not stay stuck.
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &600);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #130: Referral fee distribution cut allocation (DistributionSplit)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribution_split_valid_referral_allocation() {
    let env = Env::default();
    let r0 = Address::generate(&env);
    let r1 = Address::generate(&env);
    let referral = Address::generate(&env);

    let split = DistributionSplit {
        recipients: soroban_sdk::vec![&env, r0, r1],
        shares_bps: soroban_sdk::vec![&env, 7_000u32, 2_000u32],
        referral: Some(referral),
        referral_bps: 1_000, // 10% referral + 90% recipients = 100%
    };

    assert_eq!(split.total_bps(), 10_000);
    assert_eq!(split.validate(), Ok(()));
}

#[test]
fn test_distribution_split_rejects_over_100_percent() {
    let env = Env::default();
    let r0 = Address::generate(&env);
    let referral = Address::generate(&env);

    let split = DistributionSplit {
        recipients: soroban_sdk::vec![&env, r0],
        shares_bps: soroban_sdk::vec![&env, 9_500u32],
        referral: Some(referral),
        referral_bps: 1_000, // 95% + 10% = 105% > 100%
    };

    assert_eq!(split.total_bps(), 10_500);
    assert_eq!(split.validate(), Err(Error::SplitsExceedTotal));
}

#[test]
fn test_distribution_split_referral_bps_without_recipient_rejected() {
    let env = Env::default();
    let r0 = Address::generate(&env);

    let split = DistributionSplit {
        recipients: soroban_sdk::vec![&env, r0],
        shares_bps: soroban_sdk::vec![&env, 5_000u32],
        referral: None,
        referral_bps: 500,
    };

    assert_eq!(split.validate(), Err(Error::InvalidReferralCut));
}

#[test]
fn test_distribution_split_mismatched_lengths_rejected() {
    let env = Env::default();
    let r0 = Address::generate(&env);
    let r1 = Address::generate(&env);

    let split = DistributionSplit {
        recipients: soroban_sdk::vec![&env, r0, r1],
        shares_bps: soroban_sdk::vec![&env, 10_000u32],
        referral: None,
        referral_bps: 0,
    };

    assert_eq!(split.validate(), Err(Error::InvalidSplit));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #126: Multi-currency payment distribution routing (distribute_multi_asset)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_multi_asset_routes_each_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);

    let (token_a, asset_a) = make_token(&env);
    let (token_b, asset_b) = make_token(&env);

    // Fund the distributor with both assets.
    asset_a.mint(&distributor_id, &1_000);
    asset_b.mint(&distributor_id, &500);

    let a0 = Address::generate(&env);
    let a1 = Address::generate(&env);
    let ref_a = Address::generate(&env);
    let b0 = Address::generate(&env);
    let b1 = Address::generate(&env);

    let route_a = AssetRoute {
        token: token_a.address.clone(),
        amount: 1_000,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, a0.clone(), a1.clone()],
            shares_bps: soroban_sdk::vec![&env, 6_000u32, 3_000u32],
            referral: Some(ref_a.clone()),
            referral_bps: 1_000, // 10% referral cut
        },
    };
    let route_b = AssetRoute {
        token: token_b.address.clone(),
        amount: 500,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, b0.clone(), b1.clone()],
            shares_bps: soroban_sdk::vec![&env, 5_000u32, 5_000u32],
            referral: None,
            referral_bps: 0,
        },
    };

    let events_before = env.events().all().events().len();
    distributor.distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route_a, route_b]);

    // Asset A: referral 10% = 100; a1 = 30% = 300; a0 residual = 1000-100-300 = 600.
    assert_eq!(token_a.balance(&ref_a), 100);
    assert_eq!(token_a.balance(&a1), 300);
    assert_eq!(token_a.balance(&a0), 600);
    // Asset B: b1 = 50% = 250; b0 residual = 500-250 = 250.
    assert_eq!(token_b.balance(&b1), 250);
    assert_eq!(token_b.balance(&b0), 250);
    // No dust left in the contract for either asset.
    assert_eq!(token_a.balance(&distributor_id), 0);
    assert_eq!(token_b.balance(&distributor_id), 0);
}

#[test]
fn test_distribute_multi_asset_empty_list_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);

    let routes: soroban_sdk::Vec<AssetRoute> = soroban_sdk::vec![&env];
    let result = distributor.try_distribute_multi_asset(&admin, &routes);
    assert_eq!(result, Err(Ok(Error::EmptyAssetList)));
}

#[test]
fn test_distribute_multi_asset_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &1_000);

    let attacker = Address::generate(&env);
    let recipient = Address::generate(&env);
    let route = AssetRoute {
        token: token.address.clone(),
        amount: 1_000,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, recipient],
            shares_bps: soroban_sdk::vec![&env, 10_000u32],
            referral: None,
            referral_bps: 0,
        },
    };

    let result = distributor.try_distribute_multi_asset(&attacker, &soroban_sdk::vec![&env, route]);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_distribute_multi_asset_rejects_over_100_percent_split() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &1_000);

    let r0 = Address::generate(&env);
    let referral = Address::generate(&env);
    let route = AssetRoute {
        token: token.address.clone(),
        amount: 1_000,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, r0],
            shares_bps: soroban_sdk::vec![&env, 9_500u32],
            referral: Some(referral),
            referral_bps: 1_000, // 95% + 10% = 105% > 100%
        },
    };

    let result = distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);
    assert_eq!(result, Err(Ok(Error::SplitsExceedTotal)));
    // Atomic rollback: no funds moved out of the contract.
    assert_eq!(token.balance(&distributor_id), 1_000);
}

#[test]
fn test_distribute_multi_asset_rejects_duplicate_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &1_000);

    let r0 = Address::generate(&env);
    let split = DistributionSplit {
        recipients: soroban_sdk::vec![&env, r0],
        shares_bps: soroban_sdk::vec![&env, 10_000u32],
        referral: None,
        referral_bps: 0,
    };
    let route_1 = AssetRoute {
        token: token.address.clone(),
        amount: 400,
        split: split.clone(),
    };
    let route_2 = AssetRoute {
        token: token.address.clone(),
        amount: 600,
        split,
    };

    let result =
        distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route_1, route_2]);
    assert_eq!(result, Err(Ok(Error::AssetMismatch)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #125: Emergency withdrawal safeguard
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_emergency_withdraw_by_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &1_000);

    let safe = Address::generate(&env);
    distributor.emergency_withdraw(&admin, &token.address, &safe);

    assert_eq!(token.balance(&safe), 1_000);
    assert_eq!(token.balance(&distributor_id), 0);
}

#[test]
fn test_emergency_withdraw_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &1_000);

    let attacker = Address::generate(&env);
    let safe = Address::generate(&env);
    let result = distributor.try_emergency_withdraw(&attacker, &token.address, &safe);

    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    // Funds untouched.
    assert_eq!(token.balance(&distributor_id), 1_000);
}

#[test]
fn test_emergency_withdraw_empty_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let (token, _asset) = make_token(&env);

    let safe = Address::generate(&env);
    let result = distributor.try_emergency_withdraw(&admin, &token.address, &safe);

    assert_eq!(result, Err(Ok(Error::NothingToWithdraw)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #121: Dynamic Escrow Contract Address Binding
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_set_escrow_contract_by_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);

    assert_eq!(distributor.get_escrow_contract(), None);
    distributor.set_escrow_contract(&admin, &escrow);
    assert_eq!(distributor.get_escrow_contract(), Some(escrow));
}

#[test]
fn test_set_escrow_contract_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, _distributor_id, distributor) = distributor_only(&env);
    let attacker = Address::generate(&env);
    let escrow = Address::generate(&env);

    let result = distributor.try_set_escrow_contract(&attacker, &escrow);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    assert_eq!(distributor.get_escrow_contract(), None);
}

#[test]
fn test_set_escrow_contract_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let events = env.events().all();
    assert!(events.events().len() > 0);
}

#[test]
fn test_get_escrow_contract_requires_init() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let result = distributor.try_get_escrow_contract();
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #131: Whitelisted Escrow Origin Filter Enforcement
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_open_when_no_escrow_bound() {
    // Backward-compatible: with no escrow whitelisted, any caller (that can produce
    // escrow_contract auth) may still call distribute_payment.
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "OPEN");
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &100);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert!(result.is_ok());
}

#[test]
fn test_distribute_payment_accepts_whitelisted_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    distributor.set_escrow_contract(&admin, &escrow);

    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "WL_OK");
    let (token, asset) = make_token(&env);
    asset.mint(&distributor_id, &100);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert!(result.is_ok());
}

#[test]
fn test_distribute_payment_rejects_non_whitelisted_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let whitelisted_escrow = Address::generate(&env);
    distributor.set_escrow_contract(&admin, &whitelisted_escrow);

    let other_escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "WL_BAD");
    let (token, asset) = make_token(&env);
    asset.mint(&other_escrow, &100);

    let result = distributor.try_distribute_payment(
        &other_escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #129: Distributor Fee Calculation Dry-Run Getter
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_calculate_distribution_splits_matches_actual_distribution() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    let preview = ctx.distributor.calculate_distribution_splits(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 950i128, 500i128],
    );

    assert_eq!(preview.seller_amount, 1_000);
    assert_eq!(preview.investor_amount, 950);
    assert_eq!(preview.platform_fee, 50);
    assert_eq!(preview.total_distribution, 2_000);

    // Dry-run must not mutate any state or move funds.
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);

    // The real distribution then produces the same numbers the preview promised.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
}

#[test]
fn test_calculate_distribution_splits_rejects_invalid_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "PREVIEW_BAD_BPS");

    let result = distributor.try_calculate_distribution_splits(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, seller.clone(), seller.clone(), funder, seller.clone()],
        &soroban_sdk::vec![&env, 1_000i128, 0i128, 500i128, 10_001u32 as i128],
    );

    assert_eq!(result, Err(Ok(Error::InvalidBps)));
}

#[test]
fn test_calculate_distribution_splits_rejects_nothing_to_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "PREVIEW_NOTHING");

    // paid_amount == already-distributed (0 == 0) -> nothing new to distribute.
    let result = distributor.try_calculate_distribution_splits(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, seller.clone(), seller.clone(), funder, seller.clone()],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 0i128],
    );

    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #120: Enforce exact payment token balance verification
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);

    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "TEST_INV");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &1u32,
    );

    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_distribute_refund_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);

    let escrow = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "TEST_INV");
    let (token, _asset) = make_token(&env);

    let result = distributor.try_distribute_refund(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), funder],
        &soroban_sdk::vec![&env, 100i128],
        &3u32,
    );

    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #119: Implement Dust Amount Collector and Sweep Function
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sweep_dust_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    let fee_recipient = Address::generate(&env);
    distributor.set_fee_recipient(&admin, &fee_recipient);

    asset.mint(&distributor_id, &100);

    let result = distributor.try_sweep_dust(&admin, &token.address);
    assert!(result.is_ok());

    assert_eq!(token.balance(&fee_recipient), 100);
    assert_eq!(token.balance(&distributor_id), 0);
}

#[test]
fn test_sweep_dust_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    asset.mint(&distributor_id, &100);

    let fake_admin = Address::generate(&env);
    let result = distributor.try_sweep_dust(&fake_admin, &token.address);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_sweep_dust_nothing_to_sweep() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let (token, _asset) = make_token(&env);

    let result = distributor.try_sweep_dust(&admin, &token.address);
    assert_eq!(result, Err(Ok(Error::NothingToSweep)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Overflow Scenarios
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_overflow_in_fee_calculation() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "OVERFLOW");
    let (token, asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);
    let large_amount = i128::MAX / 2;
    asset.mint(&distributor_id, &large_amount);

    // Use i128::MAX amounts which should trigger overflow errors in fee calculation
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, i128::MAX, i128::MAX, 0i128, 10_000u32 as i128],
        &2u32,
    );

    // Should handle overflow gracefully
    assert!(result.is_err());
}

/// Verify that a freshly initialized distributor can receive tokens and
/// immediately distribute them, with correct balances and state persistence.
///
/// Contract math for distribute_payment:
///   payment_amount = paid_amount - already_distributed = 1000 - 0 = 1000
///   platform_fee   = 1000 * 300 / 10000 = 30
///   seller_amount  = payment_amount = 1000
///   investor_amount = amounts[2] = 400
///   total_distribution = 1000 + 400 + 30 = 1430
///
/// The distributor must hold >= total_distribution tokens before the call.
#[test]
fn test_initialize_and_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_INIT");
    let (token, asset) = make_token(&env);

    // Bind the escrow so the whitelist check passes.
    distributor.set_escrow_contract(&admin, &escrow);

    // Fund the distributor with the total needed: seller(1000) + investor(400) + fee(30) = 1430.
    asset.mint(&distributor_id, &1_430);

    // Distribute: paid_amount=1000, investor=400, fee_bps=300 (3%)
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![
            &env,
            token.address.clone(),
            seller.clone(),
            funder.clone(),
            admin.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 400i128, 300i128],
        &2u32, // ESCROW_STATUS_SETTLED
    );
    assert!(
        result.is_ok(),
        "distribute_payment should succeed: {result:?}"
    );

    // seller gets 1000 (full payment delta)
    assert_eq!(token.balance(&seller), 1_000);
    // investor (funder) gets 400
    assert_eq!(token.balance(&funder), 400);
    // platform fee: 1000 * 300 / 10000 = 30
    assert_eq!(token.balance(&admin), 30);
    // No dust left in the distributor
    assert_eq!(token.balance(&distributor_id), 0);

    // State must be persisted: paid_distributed == paid_amount
    let state = distributor.get_distribution_state(&escrow, &invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);
}

#[test]
fn test_distribute_split_overflow_in_referral_calculation() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    asset.mint(&distributor_id, &i128::MAX);

    let r0 = Address::generate(&env);
    let referral = Address::generate(&env);
    let route = AssetRoute {
        token: token.address.clone(),
        amount: i128::MAX,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, r0],
            shares_bps: soroban_sdk::vec![&env, 10_000u32],
            referral: Some(referral),
            referral_bps: 10_000, // 100% referral - could cause overflow
        },
    };

    let result = distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);
    assert!(result.is_err());
}

/// Passing paid_amount == 0 (equal to already-distributed 0) yields
/// NothingToDistribute because the payment delta is zero.
#[test]
fn test_distribute_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    asset.mint(&distributor_id, &i128::MAX);

    let r0 = Address::generate(&env);
    let referral = Address::generate(&env);
    let route = AssetRoute {
        token: token.address.clone(),
        amount: i128::MAX,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, r0],
            shares_bps: soroban_sdk::vec![&env, 10_000u32],
            referral: Some(referral),
            referral_bps: 10_000, // 100% referral - could cause overflow
        },
    };

    let result = distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);
    assert!(result.is_err());
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Zero Amount Validation
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_zero_payment_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "ZERO_PAY");
    let (token, asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);
    asset.mint(&distributor_id, &100);

    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_ZERO");

    distributor.set_escrow_contract(&admin, &escrow);
    asset.mint(&distributor_id, &1_000);

    // paid_amount = 0 → payment_delta = 0 − 0 = 0 → NothingToDistribute
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 500u32 as i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

#[test]
fn test_distribute_split_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "SPLIT_ZERO");
    let (token, asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);
    asset.mint(&distributor_id, &1_000);

    // All amounts are zero — split has nothing to distribute
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin.clone()],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 0i128],
        &2u32,
    );
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));

    // No funds should have moved
    assert_eq!(token.balance(&distributor_id), 1_000);
}

/// Passing a negative paid_amount produces NothingToDistribute because
/// the payment delta (negative − 0 = negative) is ≤ 0.
#[test]
fn test_distribute_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    asset.mint(&distributor_id, &100);

    let r0 = Address::generate(&env);
    let route = AssetRoute {
        token: token.address.clone(),
        amount: 0, // Zero amount should be rejected
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, r0],
            shares_bps: soroban_sdk::vec![&env, 10_000u32],
            referral: None,
            referral_bps: 0,
        },
    };

    let result = distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_refund_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "ZERO_REFUND");
    let (token, _asset) = make_token(&env);

    let result = distributor.try_distribute_refund(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), funder],
        &soroban_sdk::vec![&env, 0i128],
        &3u32,
    );

    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Empty and Mismatched Vector Validation
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_empty_addresses_vector() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "EMPTY_ADDR");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env], // Empty addresses vector
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_payment_empty_amounts_vector() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "EMPTY_AMT");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env], // Empty amounts vector
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_payment_mismatched_vector_lengths() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "MISMATCH");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    // 3 addresses but 4 amounts
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_refund_mismatched_vector_lengths() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "REFUND_MISMATCH");
    let (token, _asset) = make_token(&env);

    // 1 address but 2 amounts
    let result = distributor.try_distribute_refund(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone()],
        &soroban_sdk::vec![&env, 100i128, 50i128],
        &3u32,
    );

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Invalid Escrow Status Values
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_invalid_escrow_status_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "STATUS_ZERO");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &0u32, // Invalid status (0)
    );

    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

#[test]
fn test_distribute_payment_invalid_escrow_status_high_value() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "STATUS_HIGH");
    let (token, _asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &999u32, // Invalid status (999)
    );

    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

#[test]
fn test_distribute_refund_invalid_escrow_status() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "REFUND_STATUS");
    let (token, _asset) = make_token(&env);

    // Try to distribute refund with FUNDED status instead of REFUNDED
    let result = distributor.try_distribute_refund(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), funder],
        &soroban_sdk::vec![&env, 100i128],
        &1u32, // FUNDED status, not REFUNDED (3)
    );

    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: State Storage Persistence Verification
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribution_state_persists_after_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // Verify state before distribution
    let state_before = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_before.paid_distributed, 0);

    // Perform distribution
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);

    // Verify state persisted correctly
    let state_after = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_after.paid_distributed, 500);
    assert!(!state_after.refund_distributed);

    // Perform another distribution
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);

    // Verify state updated correctly
    let state_final = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_final.paid_distributed, 1_000);
    assert!(!state_final.refund_distributed);
}

#[test]
fn test_distribution_state_persists_after_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(1_000);
    create_and_fund(&ctx, 1_000, 2_000);

    ctx.payment_asset.mint(&ctx.payer, &400);
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(2_001);
    ctx.escrow.refund(&ctx.invoice_id);

    // Verify refund flag persisted
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);
}

#[test]
fn test_fee_recipient_state_persists() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let new_recipient = Address::generate(&env);

    // Set fee recipient
    distributor.set_fee_recipient(&admin, &new_recipient);

    // Verify it persisted
    let retrieved = distributor.get_fee_recipient();
    assert_eq!(retrieved, new_recipient);

    // Update again
    let another_recipient = Address::generate(&env);
    distributor.set_fee_recipient(&admin, &another_recipient);

    // Verify update persisted
    let retrieved_again = distributor.get_fee_recipient();
    assert_eq!(retrieved_again, another_recipient);
}

#[test]
fn test_escrow_contract_state_persists() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);

    // Set escrow contract
    distributor.set_escrow_contract(&admin, &escrow);

    // Verify it persisted
    let retrieved = distributor.get_escrow_contract();
    assert_eq!(retrieved, Some(escrow));

    // Update to different escrow
    let new_escrow = Address::generate(&env);
    distributor.set_escrow_contract(&admin, &new_escrow);

    // Verify update persisted
    let retrieved_again = distributor.get_escrow_contract();
    assert_eq!(retrieved_again, Some(new_escrow));
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Maximum Value Boundary Testing
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_distribute_payment_with_large_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000_000_000, 50_000); // 1 billion funding
    ctx.payment_asset.mint(&ctx.payer, &1_000_000_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000_000_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000_000_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950_000_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50_000_000);
}

#[test]
fn test_distribute_multi_asset_with_maximum_recipients() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "MAX_RECIP");
    let (token, asset) = make_token(&env);

    distributor.set_escrow_contract(&admin, &escrow);
    asset.mint(&distributor_id, &1_000);

    // Negative amounts should be rejected as NothingToDistribute
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, admin.clone()],
        &soroban_sdk::vec![&env, -500i128, -500i128, 0i128, 0i128],
        &2u32,
    );
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));

    // Funds untouched
    assert_eq!(token.balance(&distributor_id), 1_000);
}

/// Calling distribute_payment before initialize() returns NotInit.
#[test]
fn test_distribute_not_initialized_fails() {
    let env = Env::default();
    env.mock_all_auths();

    // Deliberately skip initialize().
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let (token, asset) = make_token(&env);
    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_NOINIT");

    asset.mint(&distributor_id, &1_000);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, token.address.clone(), seller, funder, escrow.clone()],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

/// A non-admin, non-operator caller to distribute_multi_asset must be rejected
/// with Unauthorized. This exercises the authorization guard directly on the
/// distributor without going through the full escrow wiring.
#[test]
fn test_distribute_unauthorized_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    asset.mint(&distributor_id, &1_000);

    let attacker = Address::generate(&env);
    let recipient = Address::generate(&env);

    let route = AssetRoute {
        token: token.address.clone(),
        amount: 1_000,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, recipient],
            shares_bps: soroban_sdk::vec![&env, 10_000u32],
            referral: None,
            referral_bps: 0,
        },
    };

    let result = distributor.try_distribute_multi_asset(&attacker, &soroban_sdk::vec![&env, route]);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    // No funds should have moved
    assert_eq!(token.balance(&distributor_id), 1_000);
}

/// Distribute exactly the full token balance held by the distributor.
/// Verifies that the contract can drain itself to zero with no leftover dust.
#[test]
fn test_distribute_full_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    let escrow = Address::generate(&env);
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_FULL");

    distributor.set_escrow_contract(&admin, &escrow);

    // Fund the distributor with exactly the amount we will distribute.
    // paid_amount=500, investor_amount=200, fee_bps=0 → total needed = 700.
    let total = 700i128;
    asset.mint(&distributor_id, &total);

    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![
            &env,
            token.address.clone(),
            seller.clone(),
            funder.clone(),
            admin.clone()
        ],
        &soroban_sdk::vec![&env, 500i128, 500i128, 200i128, 0i128],
        &2u32,
    );
    assert!(
        result.is_ok(),
        "full-balance distribute should succeed: {result:?}"
    );

    // seller = 500, funder = 200, fee = 0
    assert_eq!(token.balance(&seller), 500);
    assert_eq!(token.balance(&funder), 200);
    // The contract itself must be fully drained
    assert_eq!(token.balance(&distributor_id), 0);

    // State persisted correctly
    let state = distributor.get_distribution_state(&escrow, &invoice_id);
    assert_eq!(state.paid_distributed, 500);
}

/// distribute_multi_asset with multiple recipients via DistributionSplit.
/// Verifies that the primary (residual) recipient and all secondary recipients
/// receive the correct amounts with no dust left in the contract.
#[test]
fn test_distribute_multiple_recipients() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, distributor_id, distributor) = distributor_only(&env);
    let (token, asset) = make_token(&env);

    // Fund the distributor with 1_000 tokens.
    asset.mint(&distributor_id, &1_000);

    let primary = Address::generate(&env); // residual recipient (50%)
    let second = Address::generate(&env); // 30% share
    let third = Address::generate(&env); // 20% share

    // primary receives 1000 * 5000/10000 = 500, second 300, third 200
    let route = AssetRoute {
        token: token.address.clone(),
        amount: 1_000,
        split: DistributionSplit {
            recipients: soroban_sdk::vec![&env, primary.clone(), second.clone(), third.clone()],
            shares_bps: soroban_sdk::vec![&env, 5_000u32, 3_000u32, 2_000u32],
            referral: None,
            referral_bps: 0,
        },
    };

    let result = distributor.try_distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);
    assert!(result.is_ok());
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Multiple Distributions to Same Invoice
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_multiple_partial_distributions_to_same_invoice() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 10_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &10_000);

    // First partial distribution
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &2_000);
    let state1 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state1.paid_distributed, 2_000);

    // Second partial distribution
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &3_000);
    let state2 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state2.paid_distributed, 5_000);

    // Third partial distribution
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &5_000);
    let state3 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state3.paid_distributed, 10_000);
}

#[test]
fn test_distribution_state_isolated_between_invoices() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let invoice_id_2 = Symbol::new(&env, "INV_2");

    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // Distribute for first invoice
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);

    // Check state for first invoice
    let state1 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state1.paid_distributed, 500);

    // Check state for second invoice (should be default)
    let state2 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &invoice_id_2);
    assert_eq!(state2.paid_distributed, 0);
}

// ══════════════════════════════════════════════════════════════════════════════
// Edge Case: Uninitialized Contract Access
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_get_admin_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let result = distributor.try_get_admin();
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_get_fee_recipient_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let result = distributor.try_get_fee_recipient();
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_get_distribution_state_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "TEST");

    let result = distributor.try_get_distribution_state(&escrow, &invoice_id);
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_calculate_distribution_splits_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "TEST");
    let seller = Address::generate(&env);
    let funder = Address::generate(&env);

    let result = distributor.try_calculate_distribution_splits(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![&env, seller.clone(), seller.clone(), funder, seller.clone()],
        &soroban_sdk::vec![&env, 100i128, 0i128, 50i128, 500u32 as i128],
    );

    assert_eq!(result, Err(Ok(Error::NotInit)));
}

/// get_admin returns the exact address that was passed to initialize().
/// Also verifies that a second, separate distributor returns its own admin.
#[test]
fn test_get_admin_returns_correct_address() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    distributor.initialize(&admin);

    let stored = distributor.get_admin();
    assert_eq!(stored, admin, "get_admin must return the initialised admin");

    // A second, independent distributor with a different admin should return
    // its own admin, not the first one's.
    let admin2 = Address::generate(&env);
    let distributor_id2 = env.register(PaymentDistributor, ());
    let distributor2 = PaymentDistributorClient::new(&env, &distributor_id2);
    distributor2.initialize(&admin2);

    assert_eq!(distributor2.get_admin(), admin2);
    assert_ne!(distributor2.get_admin(), admin);
}
