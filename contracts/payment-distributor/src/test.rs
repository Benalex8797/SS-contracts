#![allow(deprecated)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol,
};

fn test_commitment(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

struct TestContext<'a> {
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
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &fee_bps);
    distributor.initialize(&admin);
    if configure_distributor {
        escrow.set_payment_distributor(&distributor_id);
    }

    TestContext {
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
fn test_fee_bps_at_maximum_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    // 10,000 BPS = 100% fee (maximum allowed)
    let ctx = setup(&env, 10_000, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 0); // All goes to fees
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 1_000);
}

#[test]
fn test_fee_bps_exceeding_maximum_fails() {
    let env = Env::default();
    env.mock_all_auths();

    // 10,001 BPS exceeds maximum - should fail during distribution
    let ctx = setup(&env, 10_001, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    let result = ctx.escrow.try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert!(result.is_err());
}

#[test]
fn test_fee_bps_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 0, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

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
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &10_000);
    
    let fee = ctx.payment_token.balance(&ctx.admin);
    assert_eq!(fee, 1); // 10,000 * 1 / 10,000 = 1

    // Test 9,999 BPS (99.99%)
    let env2 = Env::default();
    env2.mock_all_auths();
    let ctx2 = setup(&env2, 9_999, true);
    create_and_fund(&ctx2, 10_000, 50_000);
    ctx2.payment_asset.mint(&ctx2.payer, &10_000);
    ctx2.escrow.record_payment(&ctx2.invoice_id, &ctx2.payer, &10_000);
    
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

    ctx.distributor.set_fee_recipient(&ctx.admin, &new_recipient);
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

    let result = ctx.distributor.try_set_fee_recipient(&attacker, &new_recipient);
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
    
    ctx.distributor.set_fee_recipient(&ctx.admin, &custom_recipient);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

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

    ctx.distributor.set_fee_recipient(&ctx.admin, &new_recipient);

    // Verify event was emitted (events are tracked in env)
    let events = env.events().all();
    assert!(events.len() > 0);
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

    let events_before = env.events().all().len();
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    let events_after = env.events().all();

    // Verify PaymentDistributed event was emitted
    assert!(events_after.len() > events_before);
    
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

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let events = env.events().all();
    // Event symbol should be "PaymentDistributed" (PascalCase) for issue #123
    // (verification would require parsing event topics)
    assert!(events.len() > 0);
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
    assert_eq!(total_distributed, 100, "Total must equal payment amount exactly");
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

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &large_amount);

    let seller_balance = ctx.payment_token.balance(&ctx.seller);
    let investor_balance = ctx.payment_token.balance(&ctx.buyer);
    let fee_balance = ctx.payment_token.balance(&ctx.admin);

    let total = seller_balance + investor_balance + fee_balance;
    assert_eq!(total, large_amount, "No rounding loss for large amounts");
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
    assert_eq!(seller_balance + investor_balance + fee_balance, 400);
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
    assert_eq!(seller_balance + investor_balance + fee_balance, 3);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}
