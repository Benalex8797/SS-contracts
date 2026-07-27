use super::*;
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol,
};

/// Helper function to create a test commitment hash (SHA-256 format)
fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

#[test]
fn test_integration_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Setup Identities
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    // 2. Register Escrow Contract
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // 3. Register Token Contract
    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token_client = InvoiceTokenClient::new(&env, &inv_token_id);

    // 4. Register Payment Token (Stellar Asset)
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_client = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // 5. Initialize Contracts
    let invoice_id = Symbol::new(&env, "INV001");
    // Token initialization with escrow as minter
    inv_token_client.initialize(
        &admin,
        &SorobanString::from_str(&env, "Invoice #1"),
        &SorobanString::from_str(&env, "INV1"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    // Escrow initialization
    escrow_client.initialize(&admin, &300); // 3% fee

    // 6. Fund Buyer Account
    let amount = 1000;
    payment_token_asset.mint(&buyer, &amount);
    payment_token_asset.mint(&payer, &amount);

    // 7. Create Escrow
    let due_date = 10000;
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    // 8. Fund Escrow (Buyer buys the invoice)
    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Verify buyer received invoice tokens and paid payment tokens
    assert_eq!(inv_token_client.balance(&buyer), amount);
    assert_eq!(payment_token_client.balance(&buyer), 0);
    assert_eq!(payment_token_client.balance(&escrow_id), amount);

    // Verify invoice token is locked while escrow is active
    assert!(inv_token_client.transfer_locked());

    // 9. Record Payment (Payer settles the invoice)
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // The investor receives the net proceeds, the admin receives the fee,
    // and the original funded principal is released to the seller.
    assert_eq!(payment_token_client.balance(&payer), 0);
    assert_eq!(payment_token_client.balance(&admin), 30);
    assert_eq!(payment_token_client.balance(&buyer), 970);
    assert_eq!(payment_token_client.balance(&seller), 1000);
    assert_eq!(payment_token_client.balance(&escrow_id), 0);

    // Status check
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Settled
    );

    // Invoice token transfers must be unlocked after settlement
    assert!(!inv_token_client.transfer_locked());

    // Buyer can now transfer their invoice tokens freely
    let recipient = Address::generate(&env);
    inv_token_client.transfer(&buyer, &recipient, &amount);
    assert_eq!(inv_token_client.balance(&buyer), 0);
    assert_eq!(inv_token_client.balance(&recipient), amount);
}

#[test]
fn test_integration_refund_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token_client = InvoiceTokenClient::new(&env, &inv_token_id);

    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_client = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    let invoice_id = Symbol::new(&env, "INVFAIL");
    inv_token_client.initialize(
        &admin,
        &SorobanString::from_str(&env, "Failed Invoice"),
        &SorobanString::from_str(&env, "INV_FAIL"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow_client.initialize(&admin, &300);

    let amount = 1000;
    payment_token_asset.mint(&buyer, &amount);

    let due_date = 10000;
    env.ledger().set_timestamp(5000); // Current time is 5000

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &amount,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Attempt refund before due date (should fail)
    let res = escrow_client.try_refund(&invoice_id);
    assert!(res.is_err());

    // Advance time beyond due date
    env.ledger().set_timestamp(10001);

    // Attempt refund (should pass)
    escrow_client.refund(&invoice_id);

    // Verify buyer got their funds back $1000
    assert_eq!(payment_token_client.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );

    // Invoice token transfers must be unlocked after refund
    assert!(!inv_token_client.transfer_locked());
}

#[test]
fn test_integration_token_locked_during_active_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token_client = InvoiceTokenClient::new(&env, &inv_token_id);

    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    let invoice_id = Symbol::new(&env, "INVLOCK");
    inv_token_client.initialize(
        &admin,
        &SorobanString::from_str(&env, "Lock Test Invoice"),
        &SorobanString::from_str(&env, "INVLCK"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow_client.initialize(&admin, &300);

    let amount = 500i128;
    payment_token_asset.mint(&buyer, &amount);

    let due_date = 20000u64;
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Token is locked even before funding (initialized locked)
    assert!(inv_token_client.transfer_locked());

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Token is still locked after funding — transfers are blocked while invoice is active
    assert!(inv_token_client.transfer_locked());
    let other = Address::generate(&env);
    let result = inv_token_client.try_transfer(&buyer, &other, &100);
    assert!(result.is_err());

    // After settlement, token unlocks
    payment_token_asset.mint(&payer, &amount);
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    assert!(!inv_token_client.transfer_locked());
    // Buyer can now freely transfer invoice tokens
    inv_token_client.transfer(&buyer, &other, &100);
    assert_eq!(inv_token_client.balance(&buyer), amount - 100);
    assert_eq!(inv_token_client.balance(&other), 100);
}
