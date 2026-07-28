#![allow(deprecated)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String as SorobanString, Symbol,
};

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
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer);
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

// ──────────────────────────────────────────────────────────────────────────────
// distribute_batch tests
// ──────────────────────────────────────────────────────────────────────────────

use super::storage;
use super::types::DistributionState;

/// Helper: mint `amount` tokens to `distributor_id` so it can pay out.
fn fund_distributor(ctx: &TestContext<'_>, amount: i128) {
    ctx.payment_asset.mint(&ctx.distributor_id, &amount);
}

/// Build a minimal valid `BatchPaymentEntry` for the shared ctx invoice.
/// `paid_amt` is cumulative; `seller_amt` == delta from previous state.
fn make_entry(
    env: &Env,
    ctx: &TestContext<'_>,
    paid_amt: i128,
    seller_amt: i128,
    investor_amt: i128,
    fee_amt: i128,
    status: u32,
) -> BatchPaymentEntry {
    BatchPaymentEntry {
        escrow: ctx.escrow_id.clone(),
        inv_id: ctx.invoice_id.clone(),
        token: ctx.payment_token.address.clone(),
        seller: ctx.seller.clone(),
        funder: ctx.buyer.clone(),
        admin: ctx.admin.clone(),
        paid_amt,
        seller_amt,
        investor_amt,
        fee_amt,
        status,
    }
}

#[test]
fn test_batch_not_initialized_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let escrow_id = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_X");

    let entry = BatchPaymentEntry {
        escrow: escrow_id.clone(),
        inv_id: invoice_id.clone(),
        token: Address::generate(&env),
        seller: Address::generate(&env),
        funder: Address::generate(&env),
        admin: Address::generate(&env),
        paid_amt: 100,
        seller_amt: 100,
        investor_amt: 97,
        fee_amt: 3,
        status: 2u32,
    };

    let result = distributor.try_distribute_batch(&soroban_sdk::vec![&env, entry]);
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_batch_empty_entries_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let result = ctx
        .distributor
        .try_distribute_batch(&soroban_sdk::vec![&env]);
    assert_eq!(result, Err(Ok(Error::EmptyBatch)));
}

#[test]
fn test_batch_too_large_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    // Build 51 entries (MAX_BATCH_SIZE = 50).
    let mut entries: soroban_sdk::Vec<BatchPaymentEntry> = soroban_sdk::vec![&env];
    for _ in 0..51u32 {
        let escrow = Address::generate(&env);
        let inv_id = Symbol::new(&env, "INV_BIG");
        entries.push_back(BatchPaymentEntry {
            escrow,
            inv_id,
            token: ctx.payment_token.address.clone(),
            seller: ctx.seller.clone(),
            funder: ctx.buyer.clone(),
            admin: ctx.admin.clone(),
            paid_amt: 100,
            seller_amt: 100,
            investor_amt: 97,
            fee_amt: 3,
            status: 2u32,
        });
    }

    let result = ctx.distributor.try_distribute_batch(&entries);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn test_batch_invalid_escrow_status_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    fund_distributor(&ctx, 100);

    let entry = make_entry(&env, &ctx, 100, 100, 97, 3, 0u32); // status=Created
    let result = ctx
        .distributor
        .try_distribute_batch(&soroban_sdk::vec![&env, entry]);
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

#[test]
fn test_batch_nothing_to_distribute_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    fund_distributor(&ctx, 200);

    // Manually set distribution state so paid_distributed == paid_amt we are about to pass.
    storage::set_distribution(
        &env,
        &ctx.escrow_id,
        &ctx.invoice_id,
        &DistributionState {
            paid_distributed: 100,
            refund_distributed: false,
        },
    );

    // paid_amt == state.paid_distributed → delta == 0 → NothingToDistribute
    let entry = make_entry(&env, &ctx, 100, 100, 97, 3, 2u32);
    let result = ctx
        .distributor
        .try_distribute_batch(&soroban_sdk::vec![&env, entry]);
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

#[test]
fn test_batch_seller_amt_mismatch_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    fund_distributor(&ctx, 200);

    // paid_amt=100, delta=100, but seller_amt=90 → mismatch
    let entry = make_entry(&env, &ctx, 100, 90, 87, 3, 2u32);
    let result = ctx
        .distributor
        .try_distribute_batch(&soroban_sdk::vec![&env, entry]);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_batch_investor_fee_sum_mismatch_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    fund_distributor(&ctx, 200);

    // investor_amt + fee_amt = 96 + 3 = 99 ≠ seller_amt=100 → mismatch
    let entry = make_entry(&env, &ctx, 100, 100, 96, 3, 2u32);
    let result = ctx
        .distributor
        .try_distribute_batch(&soroban_sdk::vec![&env, entry]);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_batch_single_entry_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    fund_distributor(&ctx, 1_000);

    let entry = make_entry(&env, &ctx, 1_000, 1_000, 970, 30, 2u32);
    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry]);

    // Seller receives 1_000, investor receives 970, admin receives 30.
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);
}

#[test]
fn test_batch_multiple_entries_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    // Two separate escrow/invoice pairs.
    let escrow2 = Address::generate(&env);
    let inv2 = Symbol::new(&env, "INV_B");

    // Fund distributor with combined total.
    fund_distributor(&ctx, 2_000);

    let entry1 = make_entry(&env, &ctx, 1_000, 1_000, 970, 30, 2u32);
    let entry2 = BatchPaymentEntry {
        escrow: escrow2.clone(),
        inv_id: inv2.clone(),
        token: ctx.payment_token.address.clone(),
        seller: ctx.seller.clone(),
        funder: ctx.buyer.clone(),
        admin: ctx.admin.clone(),
        paid_amt: 1_000,
        seller_amt: 1_000,
        investor_amt: 970,
        fee_amt: 30,
        status: 2u32,
    };

    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry1, entry2]);

    // Combined: seller 2_000, buyer 1_940, admin 60.
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 2_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_940);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 60);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    // Both distribution states updated.
    let s1 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(s1.paid_distributed, 1_000);

    let s2 = ctx
        .distributor
        .get_distribution_state(&escrow2, &inv2);
    assert_eq!(s2.paid_distributed, 1_000);
}

#[test]
fn test_batch_incremental_two_calls_tracks_state() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    fund_distributor(&ctx, 1_000);

    // First batch: partial payment (400 of 1000).
    // fee = 400 * 5% = 20, investor = 380, seller = 400
    let entry1 = make_entry(&env, &ctx, 400, 400, 380, 20, 1u32); // Funded
    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry1]);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 380);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 20);

    // Second batch: remaining payment (1000 total - 400 already = 600 delta).
    // fee = 600 * 5% = 30, investor = 570, seller = 600
    let entry2 = make_entry(&env, &ctx, 1_000, 600, 570, 30, 2u32); // Settled
    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry2]);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
}

#[test]
fn test_batch_funded_status_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 0, true); // 0% fee
    fund_distributor(&ctx, 500);

    // Status = Funded (1) — partial payment, still active.
    let entry = make_entry(&env, &ctx, 500, 500, 500, 0, 1u32);
    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry]);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 500);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 500);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

#[test]
fn test_batch_zero_fee_no_admin_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 0, true); // 0% fee
    fund_distributor(&ctx, 1_000);

    let entry = make_entry(&env, &ctx, 1_000, 1_000, 1_000, 0, 2u32);
    ctx.distributor
        .distribute_batch(&soroban_sdk::vec![&env, entry]);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
}
