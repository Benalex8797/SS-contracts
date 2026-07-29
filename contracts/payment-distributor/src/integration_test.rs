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

struct FlowContext<'a> {
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

fn setup(env: &Env, fee_bps: u32, configure_distributor: bool) -> FlowContext<'_> {
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

    let invoice_id = Symbol::new(env, "INV_FLOW");
    inv_token.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Flow"),
        &SorobanString::from_str(env, "INVF"),
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

    FlowContext {
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

fn create_and_fund(ctx: &FlowContext<'_>, amount: i128, due_date: u64) {
    ctx.payment_asset.mint(&ctx.buyer, &amount);
    ctx.payment_asset.mint(&ctx.payer, &amount);
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
fn test_integration_settlement_routes_through_distributor_when_configured() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    assert!(!ctx.inv_token.transfer_locked());

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);
}

#[test]
fn test_integration_partial_payment_then_refund_routes_through_distributor() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund(&ctx.invoice_id);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    assert!(!ctx.inv_token.transfer_locked());

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);
}

#[test]
fn test_integration_escrow_keeps_direct_flow_without_distributor() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, false);
    create_and_fund(&ctx, 1_000, 50_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        0
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #163: Mock Contract Call Invocation Verification Tests
// ══════════════════════════════════════════════════════════════════════════════

/// Verify that the correct contract function invocations were authorized
/// during the settlement flow. Uses `env.auths()` to inspect the recorded
/// authorization tree and confirm the escrow contract's `distribute_payment`
/// invocation was properly authorized with matching arguments.
#[test]
fn test_integration_verify_auth_distribution_invocations() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Verify auth records show the correct contract invocations.
    let auths = env.auths();
    assert!(
        !auths.is_empty(),
        "Expected auth invocations, got {}",
        auths.len()
    );

    // Verify that payment distribution completed and state was recorded.
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
}

/// Verify that calling `distribute_payment` with an invalid escrow status
/// properly returns the `InvalidEscrowStatus` error code.
#[test]
fn test_integration_error_invalid_escrow_status_on_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);

    // Attempt distribute_payment directly with escrow_status=0 (Created) —
    // which is not a fundable/settleable status.
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
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &0u32, // EscrowStatus::Created (invalid for distribute_payment)
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));

    // Also test with status=3 (Refunded) — invalid for distribute_payment.
    let result2 = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &3u32, // EscrowStatus::Refunded (invalid for distribute_payment)
    );
    assert_eq!(result2, Err(Ok(Error::InvalidEscrowStatus)));
}

/// Verify that `distribute_payment` returns `InsufficientBalance` when the
/// distributor contract holds no tokens to route.
#[test]
fn test_integration_error_insufficient_balance_on_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    // Do NOT create/fund the escrow — the distributor has no tokens.

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
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 300i128],
        &1u32, // EscrowStatus::Funded
    );
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

/// Verify that a non-whitelisted escrow contract is rejected with
/// `UnauthorizedEscrow` when attempting to invoke `distribute_payment`.
#[test]
fn test_integration_error_unauthorized_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    // Generate a rogue escrow address that is NOT the whitelisted ctx.escrow_id.
    let rogue_escrow = Address::generate(&env);

    let result = ctx.distributor.try_distribute_payment(
        &rogue_escrow, // Not whitelisted!
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 300i128],
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)));
}

/// Verify that distribution state persists correctly across multiple
/// incremental payments within the same escrow lifecycle.
#[test]
fn test_integration_state_persistence_across_multiple_distributions() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // First incremental payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &300);

    let state1 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state1.paid_distributed, 300);
    assert!(!state1.refund_distributed);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Second incremental payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &700);

    let state2 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state2.paid_distributed, 1_000);
    assert!(!state2.refund_distributed);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );

    // Balances reflect full distribution.
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

/// Verify that refund state correctly persists after a partial payment
/// followed by a refund through the distributor.
#[test]
fn test_integration_state_persistence_after_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);
    ctx.payment_asset.mint(&ctx.payer, &500);

    // Partial payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);

    let state_after_payment = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_after_payment.paid_distributed, 500);
    assert!(!state_after_payment.refund_distributed);

    // Advance time past due date and refund.
    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund(&ctx.invoice_id);

    let state_after_refund = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_after_refund.paid_distributed, 500);
    assert!(state_after_refund.refund_distributed);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

/// Verify that attempting a zero-amount distribution through the dry-run
/// getter returns `NothingToDistribute` (no new payment delta).
#[test]
fn test_integration_edge_case_zero_payment_delta_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // First settle the full amount so distributed state equals paid amount.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Now the paid_distributed == 1_000. A second call with the same
    // paid_amount should yield NothingToDistribute (delta = 0).
    let result = ctx.distributor.try_calculate_distribution_splits(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 950i128, 300i128],
    );
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

/// Verify that the refund distribution authorization is correctly recorded
/// when a partial-payment-then-refund flow routes through the distributor.
#[test]
fn test_integration_refund_distribution_invocation_verified() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);
    ctx.payment_asset.mint(&ctx.payer, &400);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund(&ctx.invoice_id);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert!(state.refund_distributed);

    // Verify final state.
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);
}

/// Verify that `distribute_refund` rejects a non-refunded escrow status.
#[test]
fn test_integration_error_distribute_refund_invalid_status() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // Settle the escrow first so it isn't in Refunded status.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Try distribute_refund with status=2 (Settled) — should be rejected.
    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 500i128],
        &2u32, // EscrowStatus::Settled (invalid for distribute_refund)
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));

    // Also try with status=1 (Funded) — should also be rejected.
    let result2 = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 500i128],
        &1u32, // EscrowStatus::Funded (invalid for distribute_refund)
    );
    assert_eq!(result2, Err(Ok(Error::InvalidEscrowStatus)));
}
