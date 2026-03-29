#![allow(deprecated)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as AssetClient;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol, TryIntoVal};

/// Helper function to create a test commitment hash (SHA-256 format)
fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        // Just mock the mint call
        env.storage().instance().set(&to, &amount);
    }

    pub fn set_transfer_locked(_env: Env, _caller: Address, _locked: bool) {
        // Mock the set_transfer_locked call — no-op for unit tests
    }
}

#[test]
fn test_create_and_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Register the payment token
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // Register our mock invoice token
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    // Initialize escrow contract
    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV123");
    let amount = 1000;

    // Buyer gets payment tokens
    payment_token_asset.mint(&buyer, &2000);

    // Create escrow
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &amount,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Fund escrow
    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Check status
    let status = escrow_client.get_escrow_status(&invoice_id);
    assert_eq!(status, EscrowStatus::Funded);

    // Check tokens transferred to escrow
    assert_eq!(payment_token.balance(&escrow_id), 1000);
    assert_eq!(payment_token.balance(&buyer), 1000);
}

#[test]
fn test_record_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Register the payment token
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // Register our mock invoice token
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize escrow contract (300 bps = 3% fee)
    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV456");
    let amount = 1000;

    // Buyer gets payment tokens for funding
    payment_token_asset.mint(&buyer, &1000);
    // Payer gets payment tokens for settling
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);
    assert_eq!(payment_token.balance(&buyer), 0);

    // The contract holds the buyer's 1000
    assert_eq!(payment_token.balance(&escrow_id), 1000);

    // Now testing record_payment
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Status must be Settled
    let status = escrow_client.get_escrow_status(&invoice_id);
    assert_eq!(status, EscrowStatus::Settled);

    // Payer should have spent 1000
    assert_eq!(payment_token.balance(&payer), 0);

    // contract receives 1000 from payer and distributes 1000 (970 to buyer, 30 to admin).
    // AND it releases the 1000 initial funding to the seller.
    // Initial: 1000. + 1000 (payer) - 1000 (distribute) - 1000 (release) = 0.
    assert_eq!(payment_token.balance(&escrow_id), 0);

    assert_eq!(payment_token.balance(&buyer), 970);
    assert_eq!(payment_token.balance(&admin), 30);
    assert_eq!(payment_token.balance(&seller), 1000);
}

#[test]
fn test_escrow_created_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV789");
    let amount = 5000;
    let due_date = 2000000;

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

    // Assert escrow_created event was emitted
    let events = env.events().all();
    let event = events.last().unwrap();

    // Event tuple is (contract_address, topics, data)
    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "escrow_created"),).into_val(&env)
    );

    let event_data: (
        Symbol,
        Address,
        Address,
        i128,
        i128,
        u64,
        Address,
        Address,
        BytesN<32>,
    ) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, seller);
    assert_eq!(event_data.2, seller);
    assert_eq!(event_data.3, amount);
    assert_eq!(event_data.4, amount);
    assert_eq!(event_data.5, due_date);
    assert_eq!(event_data.6, payment_token_id.address());
    assert_eq!(event_data.7, inv_token_id);
    assert_eq!(event_data.8, test_commitment(&env, "test_invoice_data"));
    assert_eq!(event_data.6, payment_token_id.address());
    assert_eq!(event_data.7, inv_token_id);
}

#[test]
fn test_escrow_funded_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV999");
    let amount = 3000;

    payment_token_asset.mint(&buyer, &3000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &amount,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Find escrow_funded event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(topics, (Symbol::new(&env, "escrow_funded"),).into_val(&env));

    let event_data: (Symbol, Address, i128, i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, buyer);
    assert_eq!(event_data.2, amount);
    assert_eq!(event_data.3, amount); // funded_amt
    assert_eq!(event_data.4, amount); // purchase_price
}

#[test]
fn test_payment_settled_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV111");
    let amount = 1000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Find payment_settled event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "payment_settled"),).into_val(&env)
    );

    let event_data: (Symbol, i128, i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, amount); // total amount
    assert_eq!(event_data.2, 30); // platform_fee (3% of 1000)
    assert_eq!(event_data.3, 970); // investor_amount (1000 - 30)
}

#[test]
fn test_escrow_refunded_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV222");
    let amount = 2000;
    let due_date = 1000;

    payment_token_asset.mint(&buyer, &2000);

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

    // Set ledger timestamp past due date to allow refund
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    escrow_client.refund(&invoice_id);

    // Find escrow_refunded event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "escrow_refunded"),).into_val(&env)
    );

    let event_data: (Symbol, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, amount);
}

#[test]
fn test_no_settlement_event_on_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV333");
    let amount = 1000;

    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Try to record payment without funding first (should fail)
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &amount);

    // Should fail with AlreadySettled error (status is Created, not Funded)
    assert!(result.is_err());

    // Verify no payment_settled event was emitted by checking all events
    let all_events = env.events().all();
    for event in all_events.iter() {
        let (_addr, topics, _data) = event;
        // Check if this is a payment_settled event
        let topic_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.clone();
        if !topic_vec.is_empty() {
            if let Ok(symbol) = topic_vec.get(0).unwrap().try_into_val(&env) {
                let sym: Symbol = symbol;
                // Assert that no payment_settled event exists
                assert_ne!(sym, Symbol::new(&env, "payment_settled"));
            }
        }
    }
}

#[test]
fn test_no_refund_event_on_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV444");
    let amount = 1000;
    let due_date = 1000;

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

    // Set ledger timestamp past due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    // Try to refund without funding first (should fail)
    let result = escrow_client.try_refund(&invoice_id);

    // Should fail with RefundNotAllowed error (status is Created, not Funded)
    assert!(result.is_err());

    // Verify no escrow_refunded event was emitted by checking all events
    let all_events = env.events().all();
    for event in all_events.iter() {
        let (_addr, topics, _data) = event;
        // Check if this is an escrow_refunded event
        let topic_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.clone();
        if !topic_vec.is_empty() {
            if let Ok(symbol) = topic_vec.get(0).unwrap().try_into_val(&env) {
                let sym: Symbol = symbol;
                // Assert that no escrow_refunded event exists
                assert_ne!(sym, Symbol::new(&env, "escrow_refunded"));
            }
        }
    }
}

// ========== Authorization Tests ==========

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // First initialization should succeed
    escrow_client.initialize(&admin, &300);

    // Second initialization should fail
    let result = escrow_client.try_initialize(&admin, &500);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_create_escrow_requires_seller_auth() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Without auth, should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );
    assert!(result.is_err());
}

#[test]
fn test_update_platform_fee_requires_admin_auth() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Without auth, should fail
    let result = escrow_client.try_update_platform_fee_bps(&500);
    assert!(result.is_err());
}

// ========== Invalid Input Tests ==========

#[test]
fn test_initialize_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Fee > 10000 bps (100%) should fail
    let result = escrow_client.try_initialize(&admin, &10001);
    assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
}

#[test]
fn test_create_escrow_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Zero amount should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &seller,
        &0,
        &0,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_create_escrow_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Negative amount should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &seller,
        &-100,
        &-100,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_create_escrow_duplicate_invoice_id() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    // First create should succeed
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Second create with same invoice_id should fail
    let result = escrow_client.try_create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &2000,
        &2000,
        &2000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );
    assert_eq!(result, Err(Ok(Error::EscrowExists)));
}

#[test]
fn test_record_payment_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    // Zero amount should fail
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_update_platform_fee_invalid_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Fee > 10000 bps should fail
    let result = escrow_client.try_update_platform_fee_bps(&10001);
    assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
}

// ========== State Transition Tests ==========

#[test]
fn test_fund_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Try to fund non-existent escrow
    let result = escrow_client.try_fund_escrow(&Symbol::new(&env, "NONEXISTENT"), &buyer, &1000);
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_fund_escrow_already_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer1, &1000);
    payment_token_asset.mint(&buyer2, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    // First funding should succeed
    escrow_client.fund_escrow(&invoice_id, &buyer1, &1000);

    // Second funding should fail
    let result = escrow_client.try_fund_escrow(&invoice_id, &buyer2, &1000);
    assert_eq!(result, Err(Ok(Error::EscrowFunded)));
}

#[test]
fn test_record_payment_not_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Try to record payment without funding first
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1000);
    assert_eq!(result, Err(Ok(Error::AlreadySettled)));
}

#[test]
fn test_record_payment_already_settled() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &2000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // Try to record payment again
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1000);
    assert_eq!(result, Err(Ok(Error::AlreadySettled)));
}

#[test]
fn test_record_payment_amount_exceeds_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &2000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);

    // Try to record payment with amount > escrow amount
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1500);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_refund_not_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Set time past due date
    env.ledger().with_mut(|li| li.timestamp = 2000);

    // Try to refund without funding first
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

// ========== Refund Timing Tests ==========

#[test]
fn test_refund_before_due_date() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);

    // Set time before due date
    env.ledger().with_mut(|li| li.timestamp = due_date - 1);

    // Refund should fail
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

#[test]
fn test_refund_at_due_date() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);

    // Set time exactly at due date
    env.ledger().with_mut(|li| li.timestamp = due_date);

    // Refund should succeed
    escrow_client.refund(&invoice_id);

    // Verify buyer got refund
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}

#[test]
fn test_refund_after_due_date() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);

    // Set time after due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 5000);

    // Refund should succeed
    escrow_client.refund(&invoice_id);

    // Verify buyer got refund
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}

#[test]
fn test_refund_already_settled() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // Set time after due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    // Try to refund after settlement
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

// ========== Fee Calculation Tests ==========

#[test]
fn test_fee_calculation_zero_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize with 0% fee
    escrow_client.initialize(&admin, &0);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // With 0% fee, buyer should get full amount
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(payment_token.balance(&admin), 0);
}

#[test]
fn test_fee_calculation_max_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize with 100% fee (10000 bps)
    escrow_client.initialize(&admin, &10000);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &1000);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // With 100% fee, admin gets all, buyer gets nothing
    assert_eq!(payment_token.balance(&buyer), 0);
    assert_eq!(payment_token.balance(&admin), 1000);
}

#[test]
fn test_update_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Verify initial fee
    let config = escrow_client.get_config();
    assert_eq!(config.fee_bps, 300);

    // Update fee
    escrow_client.update_platform_fee_bps(&500);

    // The update should emit a platform_fee_updated event with old/new values.
    let events = env.events().all();
    let event = events.last().unwrap();
    let (_contract_addr, topics, data) = event;
    assert_eq!(
        topics,
        (Symbol::new(&env, "platform_fee_updated"),).into_val(&env)
    );
    let event_data: (u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (300, 500));
}

// ========== View Function Tests ==========

#[test]
fn test_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Try to get non-existent escrow
    let result = escrow_client.try_get_escrow(&Symbol::new(&env, "NONEXISTENT"));
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_get_config_not_initialized() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // Try to get config before initialization
    let result = escrow_client.try_get_config();
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_get_escrow_status_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Try to get status of non-existent escrow
    let result = escrow_client.try_get_escrow_status(&Symbol::new(&env, "NONEXISTENT"));
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_get_escrow_data() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let amount = 1000;
    let due_date = 1000000;

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &amount,
        &amount,
        &due_date,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );

    // Get escrow data and verify
    let data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(data.inv_id, invoice_id);
    assert_eq!(data.seller, seller);
    assert_eq!(data.debtor, seller);
    assert_eq!(data.face_value, amount);
    assert_eq!(data.purchase_price, amount);
    assert_eq!(data.due_dt, due_date);
    assert_eq!(data.token, payment_token);
    assert_eq!(data.inv_token, inv_token);
    assert_eq!(data.status, EscrowStatus::Created);
}

// ========== Operations Before Initialization Tests ==========

#[test]
fn test_create_escrow_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    // Try to create escrow without initialization
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "test_invoice_data"),
    );
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_update_fee_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // Try to update fee without initialization
    let result = escrow_client.try_update_platform_fee_bps(&500);
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_partial_payment_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_PARTIAL");
    let amount = 1000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // First payment: 400
    escrow_client.record_payment(&invoice_id, &payer, &400);

    // Status must still be Funded
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Funded
    );

    // Check balances after 400 payment:
    // Payer spent 400, remains 600
    assert_eq!(payment_token.balance(&payer), 600);
    // Admin got 3% of 400 = 12
    assert_eq!(payment_token.balance(&admin), 12);
    // Buyer (funder) got 400 - 12 = 388
    assert_eq!(payment_token.balance(&buyer), 388);
    // Seller got 400 released
    assert_eq!(payment_token.balance(&seller), 400);
    // Contract had 1000. + 400 (payer) - 400 (distribute) - 400 (release) = 600.
    assert_eq!(payment_token.balance(&escrow_id), 600);

    // Second payment: 600 (completes the 1000)
    escrow_client.record_payment(&invoice_id, &payer, &600);

    // Status must be Settled
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Settled
    );

    // Balances after full settlement:
    assert_eq!(payment_token.balance(&payer), 0);
    // Admin gets 3% of 600 = 18. Total = 12 + 18 = 30.
    assert_eq!(payment_token.balance(&admin), 30);
    // Buyer gets 600 - 18 = 582. Total = 388 + 582 = 970.
    assert_eq!(payment_token.balance(&buyer), 970);
    // Seller gets another 600 released. Total = 400 + 600 = 1000.
    assert_eq!(payment_token.balance(&seller), 1000);
    // Contract balance should be 0.
    assert_eq!(payment_token.balance(&escrow_id), 0);
}

#[test]
fn test_refund_after_partial_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_REF_PART");
    let amount = 1000;
    let due_date = 1000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &due_date,
        &payment_token.address,
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Partial payment: 300
    escrow_client.record_payment(&invoice_id, &payer, &300);

    // Balances now: Contract 700, Seller 300, Buyer 291, Admin 9.
    assert_eq!(payment_token.balance(&escrow_id), 700);

    // Advance time
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    // Refund
    escrow_client.refund(&invoice_id);

    // Status is Refunded
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );

    // Contract should be 0
    assert_eq!(payment_token.balance(&escrow_id), 0);

    // Buyer gets the remaining 700 back. Total = 291 + 700 = 991.
    assert_eq!(payment_token.balance(&buyer), 991);
    // Seller keeps the 300 already released
    assert_eq!(payment_token.balance(&seller), 300);
}

#[test]
fn test_record_payment_removes_initial_fund_even_on_full_payment() {
    // This is essentially test_record_payment but emphasising that stranded funds are gone
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let pt_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let payment_token = TokenClient::new(&env, &pt_id.address());
    let payment_token_asset = AssetClient::new(&env, &pt_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &0); // 0% fee to simplify math

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_FULL");
    let amount = 5000;

    payment_token_asset.mint(&buyer, &5000);
    payment_token_asset.mint(&payer, &5000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &100,
        &pt_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );
    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    assert_eq!(payment_token.balance(&escrow_id), 5000);

    escrow_client.record_payment(&invoice_id, &payer, &5000);

    assert_eq!(payment_token.balance(&escrow_id), 0);
    assert_eq!(payment_token.balance(&seller), 5000);
    assert_eq!(payment_token.balance(&buyer), 5000);
}

// ── Issue #41: cancel_escrow ─────────────────────────────────────────────────

fn setup_escrow_created(env: &Env) -> (Address, InvoiceEscrowClient<'_>, Address, Address, Symbol) {
    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    let pt_admin = Address::generate(env);
    let pt_id = env.register_stellar_asset_contract_v2(pt_admin.clone());
    let pt_asset = AssetClient::new(env, &pt_id.address());

    client.initialize(&admin, &300);

    let seller = Address::generate(env);
    let invoice_id = Symbol::new(env, "INV_CANC");

    client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000i128,
        &1000i128,
        &9_999_999u64,
        &pt_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );

    let _ = (pt_asset,);
    (escrow_id, client, seller, admin, invoice_id)
}

#[test]
fn test_cancel_escrow_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client, seller, _admin, invoice_id) = setup_escrow_created(&env);

    client.cancel_escrow(&invoice_id, &seller);

    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Cancelled
    );
}

#[test]
fn test_cancel_escrow_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client, seller, _admin, invoice_id) = setup_escrow_created(&env);

    client.cancel_escrow(&invoice_id, &seller);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic: Symbol = last.1.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic, Symbol::new(&env, "escrow_cancelled"));
}

#[test]
fn test_cancel_escrow_non_seller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client, _seller, _admin, invoice_id) = setup_escrow_created(&env);

    let impostor = Address::generate(&env);
    let res = client.try_cancel_escrow(&invoice_id, &impostor);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_cancel_escrow_already_funded_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    let pt_admin = Address::generate(&env);
    let pt_id = env.register_stellar_asset_contract_v2(pt_admin.clone());
    let pt_asset = AssetClient::new(&env, &pt_id.address());
    let pt_client = TokenClient::new(&env, &pt_id.address());

    client.initialize(&admin, &0);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_CFUND");

    pt_asset.mint(&buyer, &1000);

    client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000i128,
        &1000i128,
        &9_999_999u64,
        &pt_id.address(),
        &inv_token_id,
        &test_commitment(&env, "test_invoice_data"),
    );
    client.fund_escrow(&invoice_id, &buyer, &1000);

    // Cannot cancel once funded
    let res = client.try_cancel_escrow(&invoice_id, &seller);
    assert_eq!(res, Err(Ok(Error::EscrowFunded)));

    let _ = pt_client;
}

#[test]
fn test_fund_cancelled_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_id, client, seller, _admin, invoice_id) = setup_escrow_created(&env);
    client.cancel_escrow(&invoice_id, &seller);

    let buyer = Address::generate(&env);
    let res = client.try_fund_escrow(&invoice_id, &buyer, &1000);
    assert_eq!(res, Err(Ok(Error::EscrowCancelled)));
}

// ========== Distributor / Pause Tests (godsmiracle-contract) ==========

#[test]
fn test_set_payment_distributor_updates_config() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let distributor = Address::generate(&env);

    client.initialize(&admin, &300);
    client.set_payment_distributor(&distributor);

    let config = client.get_config();
    assert_eq!(config.payment_distributor, Some(distributor.clone()));
}

#[test]
fn test_set_paused_requires_admin_auth() {
    let env = Env::default();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    // Note: initialize itself needs no auth check here; set_paused does.
    client.initialize(&admin, &300);

    // Without mocked auth the call must fail
    let result = client.try_set_paused(&true);
    assert!(result.is_err());
}

#[test]
fn test_pause_blocks_lifecycle_operations_and_unpause_restores() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INVPAUSE");
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    let pt_admin = Address::generate(&env);
    let pt_id = env.register_stellar_asset_contract_v2(pt_admin);
    let pt_asset = AssetClient::new(&env, &pt_id.address());
    let pt_client = TokenClient::new(&env, &pt_id.address());

    client.initialize(&admin, &300);

    // Pause and verify create_escrow is blocked
    client.set_paused(&true);
    assert!(client.paused());

    let create_while_paused = client.try_create_escrow(
        &invoice_id,
        &seller,
        &seller, // debtor == seller for this test
        &1000i128,
        &1000i128,
        &9_999_999u64,
        &pt_id.address(),
        &inv_token_id,
        &test_commitment(&env, "pause_test_invoice"),
    );
    assert_eq!(create_while_paused, Err(Ok(Error::Paused)));

    // Unpause and create successfully
    client.set_paused(&false);
    client.create_escrow(
        &invoice_id,
        &seller,
        &payer, // use payer as debtor so record_payment works
        &1000i128,
        &1000i128,
        &9_999_999u64,
        &pt_id.address(),
        &inv_token_id,
        &test_commitment(&env, "pause_test_invoice"),
    );

    // Pause and verify fund_escrow is blocked
    pt_asset.mint(&buyer, &1000);
    client.set_paused(&true);
    let fund_while_paused = client.try_fund_escrow(&invoice_id, &buyer, &1000i128);
    assert_eq!(fund_while_paused, Err(Ok(Error::Paused)));

    // Unpause and fund successfully
    client.set_paused(&false);
    client.fund_escrow(&invoice_id, &buyer, &1000i128);

    // Pause and verify record_payment is blocked
    pt_asset.mint(&payer, &1000);
    client.set_paused(&true);
    let record_while_paused = client.try_record_payment(&invoice_id, &payer, &1000i128);
    assert_eq!(record_while_paused, Err(Ok(Error::Paused)));

    // Unpause and settle successfully
    client.set_paused(&false);
    client.record_payment(&invoice_id, &payer, &1000i128);

    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Settled);
    // Seller receives the released purchase_price collateral (1000, 0% fee not set here
    // but 300 bps fee means seller still gets 1000 from the collateral release path).
    assert_eq!(pt_client.balance(&seller), 1000);
}

// ========== Commitment Hash Tests (main) ==========

#[test]
fn test_create_escrow_with_commitment() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_CMT");

    escrow_client.initialize(&admin, &300);

    let commitment = test_commitment(&env, "invoice_pdf_hash_12345");

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &commitment,
    );

    // Verify escrow was created with commitment
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.commitment, commitment);
}

#[test]
fn test_commitment_immutable_after_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_IMM");

    escrow_client.initialize(&admin, &300);

    let original_commitment = test_commitment(&env, "original_invoice_data");

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &original_commitment,
    );

    // Verify commitment is stored
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.commitment, original_commitment);

    // Commitment should remain unchanged throughout the lifecycle.
    // (There's no update_commitment function, so this verifies immutability by design.)
}

#[test]
fn test_commitment_included_in_created_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_EVT");

    escrow_client.initialize(&admin, &300);

    let commitment = test_commitment(&env, "event_test_invoice");

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &commitment,
    );

    // Assert escrow_created event was emitted with commitment
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "escrow_created"),).into_val(&env)
    );

    // Event data includes commitment as the 9th field
    let event_data: (
        Symbol,
        Address,
        Address,
        i128,
        i128,
        u64,
        Address,
        Address,
        BytesN<32>,
    ) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, seller);
    assert_eq!(event_data.8, commitment); // Commitment is the 9th field
}

#[test]
fn test_different_commitments_for_different_invoices() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Create first invoice with commitment A
    let invoice_id_1 = Symbol::new(&env, "INV_A");
    let commitment_a = test_commitment(&env, "invoice_a_data");
    escrow_client.create_escrow(
        &invoice_id_1,
        &seller,
        &seller,
        &1000,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
        &commitment_a,
    );

    // Create second invoice with commitment B
    let invoice_id_2 = Symbol::new(&env, "INV_B");
    let commitment_b = test_commitment(&env, "invoice_b_data");
    escrow_client.create_escrow(
        &invoice_id_2,
        &seller,
        &seller,
        &2000,
        &2000,
        &2000000,
        &payment_token,
        &inv_token,
        &commitment_b,
    );

    // Verify each invoice has its own commitment
    let escrow_a = escrow_client.get_escrow(&invoice_id_1);
    let escrow_b = escrow_client.get_escrow(&invoice_id_2);

    assert_eq!(escrow_a.commitment, commitment_a);
    assert_eq!(escrow_b.commitment, commitment_b);
    assert_ne!(escrow_a.commitment, escrow_b.commitment);
}

#[test]
fn test_commitment_persists_through_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV_LIF");
    let amount = 1000;

    payment_token_asset.mint(&buyer, &amount);
    payment_token_asset.mint(&payer, &amount);

    let commitment = test_commitment(&env, "lifecycle_test_invoice");

    // Create escrow with commitment
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &payer,
        &amount,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
        &commitment,
    );

    // Verify commitment after creation
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.commitment, commitment);

    // Fund escrow
    escrow_client.fund_escrow(&invoice_id, &buyer, &amount);

    // Verify commitment persists after funding
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.commitment, commitment);

    // Record payment
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Verify commitment persists after settlement
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.commitment, commitment);
    assert_eq!(escrow_data.status, EscrowStatus::Settled);
}

// ========== Due Date Validation Tests ==========

#[test]
fn test_create_escrow_due_date_in_past_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Set ledger timestamp to a known time
    env.ledger().with_mut(|li| li.timestamp = 1000000);
    let current_timestamp = env.ledger().timestamp();

    // Try to create escrow with due_date in the past
    let past_due_date = current_timestamp - 1000;
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV_PAST"),
        &seller,
        &seller,
        &1000,
        &950,
        &past_due_date,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "past_due_test"),
    );
    assert_eq!(result, Err(Ok(Error::InvalidDueDate)));
}

#[test]
fn test_create_escrow_due_date_equal_to_current_timestamp_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Set ledger timestamp to a known time
    env.ledger().with_mut(|li| li.timestamp = 1000000);
    let current_timestamp = env.ledger().timestamp();

    // Try to create escrow with due_date equal to current timestamp
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV_EQUAL"),
        &seller,
        &seller,
        &1000,
        &950,
        &current_timestamp,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "equal_timestamp_test"),
    );
    assert_eq!(result, Err(Ok(Error::InvalidDueDate)));
}

#[test]
fn test_create_escrow_due_date_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Try to create escrow with due_date = 0
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV_ZERO"),
        &seller,
        &seller,
        &1000,
        &950,
        &0,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "zero_due_date_test"),
    );
    assert_eq!(result, Err(Ok(Error::InvalidDueDate)));
}

#[test]
fn test_create_escrow_due_date_in_future_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Set ledger timestamp to a known time
    env.ledger().with_mut(|li| li.timestamp = 1000000);
    let current_timestamp = env.ledger().timestamp();

    // Create escrow with due_date in the future - should succeed
    let future_due_date = current_timestamp + 1000000;
    let invoice_id = Symbol::new(&env, "INV_FUTURE");
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &seller,
        &1000,
        &950,
        &future_due_date,
        &payment_token,
        &inv_token,
        &test_commitment(&env, "future_due_test"),
    );

    // Verify escrow was created successfully
    let escrow_data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(escrow_data.due_dt, future_due_date);
    assert_eq!(escrow_data.status, EscrowStatus::Created);
}
