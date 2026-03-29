#![allow(deprecated)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env, String as SorobanString, Symbol,
};

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
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &fee_bps);
    distributor.initialize(&admin);
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
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer);
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