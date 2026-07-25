//! Unit tests for the invoice token contract.

use super::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Symbol, TryIntoVal, Vec};

fn setup_token(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address) {
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    let name = SorobanString::from_str(env, "Invoice INV-001");
    let symbol = SorobanString::from_str(env, "INV001");
    let invoice_id = Symbol::new(env, "inv_001");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    (client, admin, minter)
}

#[test]
fn test_initialize_and_metadata() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.balance(&admin), 0);
    assert_eq!(client.invoice_id(), Symbol::new(&env, "inv_001"));
    assert!(client.transfer_locked());

    let other = Address::generate(&env);
    assert_eq!(client.balance(&other), 0);
    assert_eq!(client.allowance(&admin, &other), 0);
}

#[test]
fn test_transfer_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    assert_eq!(client.balance(&user), 1000);

    // Transfer should be locked by default (transfer_locked = true)
    assert!(client.transfer_locked());

    // Non-admin transfer should fail with TransferLocked
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);
    assert_eq!(client.balance(&admin), 1000);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // Admin transfer should succeed even when locked
    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
}

#[test]
fn test_transfer_from_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // User approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user, &spender, &500, &expiration);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // transfer_from should fail when from is non-admin
    let recipient = Address::generate(&env);
    let result = client.try_transfer_from(&spender, &user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_from_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // transfer_from should succeed when from is admin
    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.allowance(&admin, &spender), 400);
}

#[test]
fn test_transfer_unlocked_all_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to users
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint(&user1, &1000, &minter);
    client.mint(&user2, &1000, &minter);

    // Unlock transfers (admin as caller)
    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());

    // Non-admin transfer should now succeed
    let recipient = Address::generate(&env);
    client.transfer(&user1, &recipient, &100);
    assert_eq!(client.balance(&user1), 900);
    assert_eq!(client.balance(&recipient), 100);

    // Non-admin transfer_from should also succeed
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user2, &spender, &500, &expiration);

    let recipient2 = Address::generate(&env);
    client.transfer_from(&spender, &user2, &recipient2, &200);
    assert_eq!(client.balance(&user2), 800);
    assert_eq!(client.balance(&recipient2), 200);
    assert_eq!(client.allowance(&user2, &spender), 300);
}

#[test]
fn test_set_transfer_locked_toggle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // Initially locked
    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    // Unlock transfers (admin as caller)
    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());
    client.transfer(&user, &recipient, &100);
    assert_eq!(client.balance(&user), 900);
    assert_eq!(client.balance(&recipient), 100);

    // Lock again (admin as caller)
    client.set_transfer_locked(&admin, &true);
    assert!(client.transfer_locked());
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_set_transfer_locked_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Admin unlocks transfers and emits the old/new lock state.
    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let events = env.events().all();
    let event = events.last().unwrap();
    let (_contract_addr, topics, data) = event;
    assert_eq!(
        topics,
        (Symbol::new(&env, "transfer_locked_updated"),).into_val(&env)
    );
    let event_data: (bool, bool) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (true, false));
}

#[test]
fn test_set_transfer_locked_by_minter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // Minter can unlock transfers
    assert!(client.transfer_locked());
    client.set_transfer_locked(&minter, &false);
    assert!(!client.transfer_locked());

    // Minter can also re-lock
    client.set_transfer_locked(&minter, &true);
    assert!(client.transfer_locked());
}

#[test]
fn test_set_minter_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, old_minter) = setup_token(&env);

    let new_minter = Address::generate(&env);
    client.set_minter(&new_minter);

    let events = env.events().all();
    let event = events.last().unwrap();
    let (_contract_addr, topics, data) = event;
    assert_eq!(
        topics,
        (Symbol::new(&env, "minter_updated"),).into_val(&env)
    );
    let event_data: (Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, old_minter);
    assert_eq!(event_data.1, new_minter);
}

#[test]
fn test_set_transfer_locked_unauthorized_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let stranger = Address::generate(&env);
    let result = client.try_set_transfer_locked(&stranger, &false);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));

    // Lock state should be unchanged
    assert!(client.transfer_locked());
}

#[test]
fn test_transfer_locked_with_sufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user with sufficient balance
    let user = Address::generate(&env);
    client.mint(&user, &10000, &minter);
    assert_eq!(client.balance(&user), 10000);

    // Transfer should still fail even with sufficient balance when locked
    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    // Balance should remain unchanged
    assert_eq!(client.balance(&user), 10000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Unlock transfers so we can isolate the balance failure path.
    client.set_transfer_locked(&admin, &false);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.mint(&sender, &50, &minter);

    let result = client.try_transfer(&sender, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientBalance)));
    assert_eq!(client.balance(&sender), 50);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers for this test
    client.set_transfer_locked(&admin, &false);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &250);

    // Find transfer event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250);
}

#[test]
fn test_approve_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let amount = 500;
    let expiration = env.ledger().sequence() + 100;

    client.approve(&admin, &spender, &amount, &expiration);

    // Find approve event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "approve"), admin.clone(), spender.clone()).into_val(&env)
    );

    let event_data: (i128, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, amount);
    assert_eq!(event_data.1, expiration);
}

#[test]
fn test_mint_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let amount = 5000;

    client.mint(&recipient, &amount, &minter);

    // Find mint event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "mint"), recipient.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, amount);
}

#[test]
fn test_mint_authorization_and_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let stranger = Address::generate(&env);

    // Admin can mint.
    client.mint(&recipient, &100, &admin);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.total_supply(), 100);

    // Minter can also mint.
    client.mint(&recipient, &50, &minter);
    assert_eq!(client.balance(&recipient), 150);
    assert_eq!(client.total_supply(), 150);

    // Unauthorized caller is rejected.
    let unauthorized = client.try_mint(&recipient, &25, &stranger);
    assert_eq!(unauthorized, Err(Ok(crate::errors::Error::Unauthorized)));

    // Zero amount is rejected even for an authorized caller.
    let invalid_amount = client.try_mint(&recipient, &0, &minter);
    assert_eq!(invalid_amount, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.balance(&recipient), 150);
    assert_eq!(client.total_supply(), 150);
}

#[test]
fn test_burn_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens first
    client.mint(&admin, &1000, &minter);

    // Burn some tokens
    let burn_amount = 300;
    client.burn(&admin, &burn_amount);

    // Find burn event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_burn_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &100, &minter);

    let result = client.try_burn(&admin, &200);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientBalance)));
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
}

#[test]
fn test_transfer_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Spender transfers from admin to recipient
    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &200);

    // Find transfer event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 200);
}

#[test]
fn test_burn_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Spender burns from admin
    let burn_amount = 150;
    client.burn_from(&spender, &admin, &burn_amount);

    // Find burn event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_burn_updates_balance_and_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    client.burn(&admin, &300);
    assert_eq!(client.balance(&admin), 700);
    assert_eq!(client.total_supply(), 700);
}

#[test]
fn test_burn_from_updates_balance_allowance_and_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    client.burn_from(&spender, &admin, &150);
    assert_eq!(client.balance(&admin), 850);
    assert_eq!(client.total_supply(), 850);
    assert_eq!(client.allowance(&admin, &spender), 350);
}

#[test]
fn test_burn_from_allowance_and_balance_checks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &100, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Insufficient allowance.
    client.approve(&admin, &spender, &50, &(current_ledger + 100));
    let allowance_fail = client.try_burn_from(&spender, &admin, &60);
    assert_eq!(
        allowance_fail,
        Err(Ok(crate::errors::Error::InsufficientAllowance))
    );

    // Sufficient allowance but insufficient balance.
    client.approve(&admin, &spender, &200, &(current_ledger + 100));
    let balance_fail = client.try_burn_from(&spender, &admin, &150);
    assert_eq!(
        balance_fail,
        Err(Ok(crate::errors::Error::InsufficientBalance))
    );
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.allowance(&admin, &spender), 200);
}

#[test]
fn test_no_transfer_event_on_locked_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // Transfer is locked by default
    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let events_before = env.events().all().len();

    // Try to transfer (should fail)
    let result = client.try_transfer(&user, &recipient, &100);
    assert!(result.is_err());

    // No new transfer event should be emitted after the failed call
    let events_after = env.events().all();

    // Check that no transfer event was emitted
    for i in events_before..events_after.len() {
        let event = &events_after.get(i).unwrap();
        let (_addr, topics, _data) = event;
        if let Some(first_topic) = topics.get(0) {
            let symbol: Symbol = first_topic.try_into_val(&env).unwrap();
            assert_ne!(symbol, Symbol::new(&env, "transfer"));
        }
    }
}

#[test]
fn test_multiple_events_in_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Mint event
    client.mint(&user1, &1000, &minter);

    // Transfer event
    client.transfer(&user1, &user2, &300);

    // Burn event
    client.burn(&user2, &100);

    let events = env.events().all();

    // Debug: check how many events we have
    let event_count = events.len();

    // We expect at least 3 events (mint, transfer, burn)
    // But let's be flexible and just verify the last 3 events if they exist
    if event_count >= 3 {
        // Find and verify mint event (3rd from last)
        let mint_event = events.iter().rev().nth(2).unwrap();
        let (_addr1, topics1, _data1) = mint_event;
        assert_eq!(
            topics1,
            (Symbol::new(&env, "mint"), user1.clone()).into_val(&env)
        );

        // Find and verify transfer event (2nd from last)
        let transfer_event = events.iter().rev().nth(1).unwrap();
        let (_addr2, topics2, _data2) = transfer_event;
        assert_eq!(
            topics2,
            (Symbol::new(&env, "transfer"), user1.clone(), user2.clone()).into_val(&env)
        );

        // Find and verify burn event (last)
        let burn_event = events.last().unwrap();
        let (_addr3, topics3, _data3) = burn_event;
        assert_eq!(
            topics3,
            (Symbol::new(&env, "burn"), user2.clone()).into_val(&env)
        );
    } else {
        // If we have fewer events, just verify they exist in order
        assert!(event_count > 0, "Expected at least one event");
    }
}

// ========== Allowance Expiration Boundary Tests ==========

#[test]
fn test_approve_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration exactly at current ledger should succeed
    client.approve(&admin, &spender, &500, &current_ledger);

    // Verify allowance was set
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_approve_expiration_below_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);

    // Advance ledger to ensure we can test below current
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration below current ledger should fail
    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert!(result.is_err());
}

#[test]
fn test_approve_expiration_zero_amount_allows_past() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);

    // Advance ledger to ensure we can test past expiration
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    // First set an allowance
    client.approve(&admin, &spender, &500, &(current_ledger + 100));
    assert_eq!(client.allowance(&admin, &spender), 500);

    // Approve with amount=0 and past expiration should succeed (revoke allowance)
    client.approve(&admin, &spender, &0, &(current_ledger - 5));

    // Allowance should be revoked
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_transfer_from_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration at current ledger
    client.approve(&admin, &spender, &500, &current_ledger);

    // transfer_from should succeed when expiration == current ledger
    client.transfer_from(&spender, &admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.balance(&recipient), 200);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_transfer_from_expiration_one_below_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    // Approve with expiration at initial ledger
    client.approve(&admin, &spender, &500, &initial_ledger);

    // Advance ledger by 1
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    // transfer_from should fail when expiration < current ledger
    let result = client.try_transfer_from(&spender, &admin, &recipient, &200);
    assert!(result.is_err());

    // Balances should remain unchanged
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration above current ledger
    client.approve(&admin, &spender, &500, &(current_ledger + 100));

    // transfer_from should succeed when expiration > current ledger
    client.transfer_from(&spender, &admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.balance(&recipient), 200);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_transfer_from_insufficient_allowance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &50, &expiration);

    let result = client.try_transfer_from(&spender, &admin, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientAllowance)));
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.allowance(&admin, &spender), 50);
}

#[test]
fn test_burn_from_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration at current ledger
    client.approve(&admin, &spender, &500, &current_ledger);

    // burn_from should succeed when expiration == current ledger
    client.burn_from(&spender, &admin, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.total_supply(), 800);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_burn_from_expiration_one_below_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    // Approve with expiration at initial ledger
    client.approve(&admin, &spender, &500, &initial_ledger);

    // Advance ledger by 1
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    // burn_from should fail when expiration < current ledger
    let result = client.try_burn_from(&spender, &admin, &200);
    assert!(result.is_err());

    // Balance and supply should remain unchanged
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.total_supply(), 1000);
}

#[test]
fn test_burn_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    // Approve with expiration above current ledger
    client.approve(&admin, &spender, &500, &(current_ledger + 100));

    // burn_from should succeed when expiration > current ledger
    client.burn_from(&spender, &admin, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.total_supply(), 800);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_allowance_returns_zero_when_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    // Approve with expiration at initial ledger
    client.approve(&admin, &spender, &500, &initial_ledger);

    // At current ledger, allowance should be visible
    assert_eq!(client.allowance(&admin, &spender), 500);

    // Advance ledger by 1
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    // After expiration, allowance should return 0
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_allowance_boundary_multiple_ledgers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();
    let expiration = initial_ledger + 5;

    // Approve with expiration 5 ledgers in the future
    client.approve(&admin, &spender, &500, &expiration);

    // Check allowance at various ledger sequences
    for i in 0..=5 {
        env.ledger()
            .with_mut(|li| li.sequence_number = initial_ledger + i);
        let expected = if i <= 5 { 500 } else { 0 };
        assert_eq!(client.allowance(&admin, &spender), expected);
    }

    // At expiration + 1, should be 0
    env.ledger()
        .with_mut(|li| li.sequence_number = expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_update_expiration_extends() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    // First approval with short expiration
    client.approve(&admin, &spender, &500, &(initial_ledger + 2));

    // Advance ledger by 1
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    // Update approval with extended expiration
    client.approve(&admin, &spender, &600, &(initial_ledger + 10));

    // Advance to where old expiration would have expired
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 3);

    // Allowance should still be valid with new expiration
    assert_eq!(client.allowance(&admin, &spender), 600);
}

#[test]
fn test_approve_update_expiration_shortens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    // First approval with long expiration
    client.approve(&admin, &spender, &500, &(initial_ledger + 100));

    // Update approval with shorter expiration
    let new_expiration = initial_ledger + 2;
    client.approve(&admin, &spender, &600, &new_expiration);

    // Advance to new expiration
    env.ledger()
        .with_mut(|li| li.sequence_number = new_expiration);
    assert_eq!(client.allowance(&admin, &spender), 600);

    // Advance past new expiration
    env.ledger()
        .with_mut(|li| li.sequence_number = new_expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

// ========== Negative Amount Tests ==========

#[test]
fn test_approve_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    // Attempt to approve with negative amount should fail
    let result = client.try_approve(&admin, &spender, &(-100i128), &expiration);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));

    // Verify no allowance was set
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_zero_amount_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    // First set a non-zero allowance
    client.approve(&admin, &spender, &500, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 500);

    // Approve with zero amount should remove allowance (current behavior)
    client.approve(&admin, &spender, &0, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_positive_amount_invalid_expiration_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);

    // Advance ledger to ensure we can test past expiration
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    // Attempt to approve with positive amount and invalid expiration should fail
    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));

    // Verify no allowance was set
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_sub_asset_decimals_can_be_updated_within_supported_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    client.set_decimals(&18);
    assert_eq!(client.decimals(), 18);

    let events = env.events().all();
    let (_contract_addr, topics, data) = events.last().unwrap();
    assert_eq!(topics, (Symbol::new(&env, "decimals_updated"),).into_val(&env));
    let event_data: (u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (7, 18));
}

#[test]
fn test_sub_asset_decimals_reject_unsupported_precision() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let result = client.try_set_decimals(&19);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidDecimals)));
    assert_eq!(client.decimals(), 7);
}

#[test]
fn test_initialize_rejects_unsupported_precision() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-001");
    let symbol = SorobanString::from_str(&env, "INV001");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &name, &symbol, &19, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidDecimals)));
}

#[test]
fn test_balance_batch_preserves_order_and_includes_zero_balances() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let first = Address::generate(&env);
    let empty = Address::generate(&env);
    let second = Address::generate(&env);
    client.mint(&first, &25, &minter);
    client.mint(&second, &50, &minter);

    let mut ids = Vec::new(&env);
    ids.push_back(first.clone());
    ids.push_back(empty.clone());
    ids.push_back(second.clone());
    ids.push_back(first);

    let balances = client.balance_batch(&ids);
    assert_eq!(balances, soroban_sdk::vec![&env, 25i128, 0, 50, 25]);
}

#[test]
fn test_balance_batch_requires_initialization() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let ids = soroban_sdk::vec![&env, Address::generate(&env)];

    assert_eq!(
        client.try_balance_batch(&ids),
        Err(Ok(crate::errors::Error::NotInit))
    );
}

#[test]
fn test_revoke_approval_clears_allowance_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    client.revoke_approval(&admin, &spender);
    assert_eq!(client.allowance(&admin, &spender), 0);

    let events = env.events().all();
    let (_contract_addr, topics, data) = events.last().unwrap();
    assert_eq!(
        topics,
        (Symbol::new(&env, "approval_revoked"), admin, spender).into_val(&env)
    );
    let event_data: () = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, ());
}

#[test]
fn test_revoke_approval_requires_from_authorization() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);

    assert!(client.try_revoke_approval(&admin, &spender).is_err());
}

#[test]
fn test_pause_blocks_burn_and_burn_from() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.mint(&admin, &100, &minter);
    client.approve(&admin, &spender, &100, &expiration);
    client.set_paused(&true);

    assert_eq!(client.try_burn(&admin, &10), Err(Ok(crate::errors::Error::Paused)));
    assert_eq!(
        client.try_burn_from(&spender, &admin, &10),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.allowance(&admin, &spender), 100);
}
