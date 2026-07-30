#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]
//! Unit tests for the invoice token contract.

use super::{InvoiceToken, InvoiceTokenClient};
use crate::types::StorageKey;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

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

// ========== Original Tests ==========

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

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    assert_eq!(client.balance(&user), 1000);
    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    assert_eq!(client.balance(&admin), 1000);
    assert!(client.transfer_locked());

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

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user, &spender, &500, &expiration);

    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let result = client.try_transfer_from(&spender, &user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_from_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    assert!(client.transfer_locked());

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

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint(&user1, &1000, &minter);
    client.mint(&user2, &1000, &minter);

    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());

    let recipient = Address::generate(&env);
    client.transfer(&user1, &recipient, &100);
    assert_eq!(client.balance(&user1), 900);
    assert_eq!(client.balance(&recipient), 100);

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

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());
    client.transfer(&user, &recipient, &100);
    assert_eq!(client.balance(&user), 900);
    assert_eq!(client.balance(&recipient), 100);

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

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
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

    assert!(client.transfer_locked());
    client.set_transfer_locked(&minter, &false);
    assert!(!client.transfer_locked());

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
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
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
    assert!(client.transfer_locked());
}

#[test]
fn test_transfer_locked_with_sufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &10000, &minter);
    assert_eq!(client.balance(&user), 10000);

    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    assert_eq!(client.balance(&user), 10000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

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

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &250);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    client.mint(&recipient, &100, &admin);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.total_supply(), 100);

    client.mint(&recipient, &50, &minter);
    assert_eq!(client.balance(&recipient), 150);
    assert_eq!(client.total_supply(), 150);

    let unauthorized = client.try_mint(&recipient, &25, &stranger);
    assert_eq!(unauthorized, Err(Ok(crate::errors::Error::Unauthorized)));

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

    client.mint(&admin, &1000, &minter);

    let burn_amount = 300;
    client.burn(&admin, &burn_amount);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &200);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let burn_amount = 150;
    client.burn_from(&spender, &admin, &burn_amount);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

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

    client.approve(&admin, &spender, &50, &(current_ledger + 100));
    let allowance_fail = client.try_burn_from(&spender, &admin, &60);
    assert_eq!(
        allowance_fail,
        Err(Ok(crate::errors::Error::InsufficientAllowance))
    );

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

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let events_before = env.events().all().events().len();

    let result = client.try_transfer(&user, &recipient, &100);
    assert!(result.is_err());

    let events_after = env.events().all();

    for i in events_before..events_after.events().len() {
        let event = events_after.events().get(i).unwrap();
        let (_addr, topics, _data) = parse_event(&env, event);
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

    client.set_transfer_locked(&admin, &false);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.mint(&user1, &1000, &minter);
    client.transfer(&user1, &user2, &300);
    client.burn(&user2, &100);

    let events = env.events().all();
    let event_count = events.events().len();

    if event_count >= 3 {
        // Find and verify mint event (3rd from last)
        let mint_event = events.events().iter().rev().nth(2).unwrap();
        let (addr1, topics1, _data1) = parse_event(&env, mint_event);
        assert_eq!(
            topics1,
            (Symbol::new(&env, "mint"), user1.clone()).into_val(&env)
        );

        // Find and verify transfer event (2nd from last)
        let transfer_event = events.events().iter().rev().nth(1).unwrap();
        let (addr2, topics2, _data2) = parse_event(&env, transfer_event);
        assert_eq!(
            topics2,
            (Symbol::new(&env, "transfer"), user1.clone(), user2.clone()).into_val(&env)
        );

        // Find and verify burn event (last)
        let burn_event = events.events().last().unwrap();
        let (addr3, topics3, _data3) = parse_event(&env, burn_event);
        assert_eq!(
            topics3,
            (Symbol::new(&env, "burn"), user2.clone()).into_val(&env)
        );
    } else {
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
    client.approve(&admin, &spender, &500, &current_ledger);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_approve_expiration_below_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert!(result.is_err());
}

#[test]
fn test_approve_expiration_zero_amount_allows_past() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
    assert_eq!(client.allowance(&admin, &spender), 500);

    client.approve(&admin, &spender, &0, &(current_ledger - 5));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_transfer_from_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &current_ledger);
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

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &initial_ledger);
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    let result = client.try_transfer_from(&spender, &admin, &recipient, &200);
    assert!(result.is_err());

    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
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

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &current_ledger);
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

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &initial_ledger);
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    let result = client.try_burn_from(&spender, &admin, &200);
    assert!(result.is_err());

    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.total_supply(), 1000);
}

#[test]
fn test_burn_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
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

    client.approve(&admin, &spender, &500, &initial_ledger);
    assert_eq!(client.allowance(&admin, &spender), 500);

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

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

    client.approve(&admin, &spender, &500, &expiration);

    for i in 0..=5 {
        env.ledger()
            .with_mut(|li| li.sequence_number = initial_ledger + i);
        let expected = if i <= 5 { 500 } else { 0 };
        assert_eq!(client.allowance(&admin, &spender), expected);
    }

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

    client.approve(&admin, &spender, &500, &(initial_ledger + 2));

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    client.approve(&admin, &spender, &600, &(initial_ledger + 10));

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 3);

    assert_eq!(client.allowance(&admin, &spender), 600);
}

#[test]
fn test_approve_update_expiration_shortens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(initial_ledger + 100));

    let new_expiration = initial_ledger + 2;
    client.approve(&admin, &spender, &600, &new_expiration);

    env.ledger()
        .with_mut(|li| li.sequence_number = new_expiration);
    assert_eq!(client.allowance(&admin, &spender), 600);

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

    let result = client.try_approve(&admin, &spender, &(-100i128), &expiration);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_zero_amount_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    client.approve(&admin, &spender, &500, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 500);

    client.approve(&admin, &spender, &0, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_positive_amount_invalid_expiration_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

// ========== Issue #106: Nonce Tests ==========

#[test]
fn test_get_nonce_initial_is_zero() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    let nonce = client.get_nonce(&admin);
    assert_eq!(nonce, 0);
}

#[test]
fn test_get_nonce_returns_zero_for_unknown_account() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    let unknown = Address::generate(&env);
    let nonce = client.get_nonce(&unknown);
    assert_eq!(nonce, 0);
}

#[test]
fn test_get_nonce_emits_event() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    let _nonce = client.get_nonce(&admin);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(topics, (Symbol::new(&env, "nonce_queried"),).into_val(&env));

    let (account, emitted_nonce): (Address, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(account, admin);
    assert_eq!(emitted_nonce, 0);
}

// ========== Issue #113: Fee Deduction Tests ==========

#[test]
fn test_get_fee_bps_initial_zero() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    client.set_fee_bps(&admin, &250); // 2.5%
    assert_eq!(client.get_fee_bps(), 250);
}

#[test]
fn test_set_fee_bps_unauthorized_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let stranger = Address::generate(&env);
    let result = client.try_set_fee_bps(&stranger, &250);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_invalid_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let result = client.try_set_fee_bps(&admin, &10_001);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidFeeBps)));
    assert_eq!(client.get_fee_bps(), 0);

    let result = client.try_set_fee_bps(&admin, &-1);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidFeeBps)));
    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    client.set_fee_bps(&admin, &250);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    // redundant destructuring removed

    assert_eq!(topics, (Symbol::new(&env, "fee_updated"),).into_val(&env));

    let event_data: (i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (0, 250));
}

#[test]
fn test_transfer_with_fee_deducts_from_sender() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 10% (1000 bps)
    client.set_fee_bps(&admin, &1000);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    // Mint sufficient balance (amount + fee needed)
    let user = Address::generate(&env);
    client.mint(&user, &5000, &minter);
    assert_eq!(client.balance(&user), 5000);

    // Transfer 1000: fee = 1000 * 1000 / 10000 = 100
    let recipient = Address::generate(&env);
    client.transfer(&user, &recipient, &1000);

    assert_eq!(client.balance(&user), 5000 - 1000 - 100); // 3900
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.balance(&admin), 100); // admin received fee
}

#[test]
fn test_transfer_fee_insufficient_for_total_debit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 10% (1000 bps)
    client.set_fee_bps(&admin, &1000);
    client.set_transfer_locked(&admin, &false);

    // Mint exactly 1050 to a user (amount=1000 + fee=100 = 1100, but only 1050 available)
    let user = Address::generate(&env);
    client.mint(&user, &1050, &minter);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &1000);
    // 1000 * 1000/10000 = 100, total_debit = 1100 > 1050
    // Balance >= amount (1050 >= 1000), but balance < total_debit
    assert_eq!(
        result,
        Err(Ok(crate::errors::Error::InsufficientBalanceForFee))
    );
    assert_eq!(client.balance(&user), 1050);
}

#[test]
fn test_transfer_zero_fee_no_deduction() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Fee is 0 by default
    assert_eq!(client.get_fee_bps(), 0);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let recipient = Address::generate(&env);
    client.transfer(&user, &recipient, &500);

    assert_eq!(client.balance(&user), 500);
    assert_eq!(client.balance(&recipient), 500);
    assert_eq!(client.balance(&admin), 0); // no fee collected
}

#[test]
fn test_transfer_from_with_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 5% (500 bps)
    client.set_fee_bps(&admin, &500);
    client.set_transfer_locked(&admin, &false);

    // Mint 2000 to admin, approve spender
    client.mint(&admin, &2000, &minter);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &1000, &expiration);

    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &1000);

    // fee = 1000 * 500 / 10000 = 50
    // Admin debited: 1000 (amount) + 50 (fee) = 1050
    // Admin credited: 50 (fee goes to admin)
    // Net admin: 2000 - 1050 + 50 = 1000
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_sub_asset_decimals_can_be_updated_within_supported_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    client.set_decimals(&18);
    let events = env.events().all();
    assert_eq!(client.decimals(), 18);

    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "decimals_updated"),).into_val(&env)
    );
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
    let events = env.events().all();
    assert_eq!(client.allowance(&admin, &spender), 0);

    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
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

    assert_eq!(
        client.try_burn(&admin, &10),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_burn_from(&spender, &admin, &10),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.allowance(&admin, &spender), 100);
}

#[test]
fn test_initialize_rejects_empty_name() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let empty_name = SorobanString::from_str(&env, "");
    let symbol = SorobanString::from_str(&env, "INV001");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &empty_name, &symbol, &7u32, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidMetadata)));
}

#[test]
fn test_initialize_rejects_empty_symbol() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-001");
    let empty_symbol = SorobanString::from_str(&env, "");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &name, &empty_symbol, &7u32, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidMetadata)));
}

#[test]
fn test_extend_allowance_updates_expiration_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let new_expiration = expiration + 200;
    client.extend_allowance(&admin, &spender, &new_expiration);

    // Amount is unchanged; only the expiration ledger moved forward.
    assert_eq!(client.allowance(&admin, &spender), 500);

    // Ledger advances past the original expiration but before the extended one:
    // the allowance must still be usable.
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_extend_allowance_requires_later_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let result = client.try_extend_allowance(&admin, &spender, &expiration);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));

    let result = client.try_extend_allowance(&admin, &spender, &(expiration - 1));
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));
}

#[test]
fn test_extend_allowance_fails_without_existing_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);

    let result = client.try_extend_allowance(&admin, &spender, &(env.ledger().sequence() + 100));
    assert_eq!(result, Err(Ok(crate::errors::Error::AllowanceNotFound)));
}

#[test]
fn test_extend_allowance_fails_after_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let current = env.ledger().sequence();
    let expiration = current + 10;
    client.approve(&admin, &spender, &500, &expiration);

    // Move past expiration before attempting to extend.
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);
    let result = client.try_extend_allowance(&admin, &spender, &(expiration + 100));
    assert_eq!(result, Err(Ok(crate::errors::Error::AllowanceExpired)));
}

// ========== Storage Key Serialization Tests (Issue #161) ==========

/// Helper: assert a StorageKey roundtrips losslessly through Val.
fn assert_key_roundtrip(env: &Env, key: StorageKey) {
    let val: Val = key.clone().into_val(env);
    let back: StorageKey = StorageKey::try_from_val(env, &val).expect("roundtrip failed");
    assert_eq!(key, back, "StorageKey roundtrip equality");
}

/// Each StorageKey variant serializes and deserializes without data loss.
#[test]
fn test_storage_key_serialization_roundtrip_metadata() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::Metadata);
}

#[test]
fn test_storage_key_serialization_roundtrip_total_supply() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::TotalSupply);
}

#[test]
fn test_storage_key_serialization_roundtrip_fee_bps() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::FeeBps);
}

#[test]
fn test_storage_key_serialization_roundtrip_balance() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Balance(addr));
}

#[test]
fn test_storage_key_serialization_roundtrip_allowance() {
    let env = Env::default();
    let from = Address::generate(&env);
    let spender = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Allowance(from, spender));
}

#[test]
fn test_storage_key_serialization_roundtrip_role_admin() {
    let env = Env::default();
    let role = Symbol::new(&env, "admin");
    assert_key_roundtrip(&env, StorageKey::RoleAdmin(role));
}

#[test]
fn test_storage_key_serialization_roundtrip_role_grant() {
    let env = Env::default();
    let role = Symbol::new(&env, "minter");
    let account = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::RoleGrant(role, account));
}

#[test]
fn test_storage_key_serialization_roundtrip_nonce() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Nonce(addr));
}

#[test]
fn test_storage_key_serialization_roundtrip_history() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::History(addr));
}

/// All StorageKey variants produce distinct serialized values (no collisions).
#[test]
fn test_storage_key_uniqueness_all_variants_distinct() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let role_admin = Symbol::new(&env, "admin");

    let keys = [
        StorageKey::Metadata,
        StorageKey::TotalSupply,
        StorageKey::FeeBps,
        StorageKey::Balance(addr_a.clone()),
        StorageKey::Allowance(addr_a.clone(), addr_b.clone()),
        StorageKey::RoleAdmin(role_admin.clone()),
        StorageKey::RoleGrant(role_admin.clone(), addr_a.clone()),
        StorageKey::Nonce(addr_a.clone()),
        StorageKey::History(addr_a.clone()),
    ];

    let n = keys.len();
    for i in 0..n {
        for j in (i + 1)..n {
            assert_ne!(
                keys[i], keys[j],
                "StorageKey variants at indices {i} and {j} must be distinct"
            );
        }
    }
}

/// Same variant with different data produces different keys.
#[test]
fn test_storage_key_same_variant_different_data_distinct() {
    let env = Env::default();
    let addr = Address::generate(&env);
    let other_addr = Address::generate(&env);

    // Balance with different addresses
    assert_ne!(
        StorageKey::Balance(addr.clone()),
        StorageKey::Balance(other_addr.clone()),
        "Balance keys with different addresses must differ"
    );

    // Nonce with different addresses
    assert_ne!(
        StorageKey::Nonce(addr.clone()),
        StorageKey::Nonce(other_addr.clone()),
        "Nonce keys with different addresses must differ"
    );

    // History with different addresses
    assert_ne!(
        StorageKey::History(addr.clone()),
        StorageKey::History(other_addr.clone()),
        "History keys with different addresses must differ"
    );

    // Allowance with different pairs
    let third = Address::generate(&env);
    assert_ne!(
        StorageKey::Allowance(addr.clone(), other_addr.clone()),
        StorageKey::Allowance(other_addr.clone(), addr.clone()),
        "Allowance (A,B) must differ from (B,A)"
    );
    assert_ne!(
        StorageKey::Allowance(addr.clone(), other_addr.clone()),
        StorageKey::Allowance(addr.clone(), third.clone()),
        "Allowance with different spender must differ"
    );
    assert_ne!(
        StorageKey::Allowance(other_addr.clone(), addr.clone()),
        StorageKey::Allowance(addr.clone(), third.clone()),
        "Allowance with different from must differ"
    );

    // RoleAdmin with different symbols
    assert_ne!(
        StorageKey::RoleAdmin(Symbol::new(&env, "admin")),
        StorageKey::RoleAdmin(Symbol::new(&env, "minter")),
        "RoleAdmin keys with different roles must differ"
    );

    // RoleGrant with different roles or accounts
    assert_ne!(
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), addr.clone()),
        StorageKey::RoleGrant(Symbol::new(&env, "admin"), addr.clone()),
        "RoleGrant keys with different roles must differ"
    );
    assert_ne!(
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), addr.clone()),
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), other_addr.clone()),
        "RoleGrant keys with different accounts must differ"
    );
}

/// Instance-storage keys are distinct from persistent-storage keys.
/// This test verifies that the XDR serialization preserves the variant
/// discriminant so instance keys never collide with persistent keys.
#[test]
fn test_storage_key_instance_vs_persistent_distinct() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let instance_keys = [
        StorageKey::Metadata,
        StorageKey::TotalSupply,
        StorageKey::FeeBps,
        StorageKey::RoleAdmin(Symbol::new(&env, "role")),
        StorageKey::RoleGrant(Symbol::new(&env, "role"), addr.clone()),
    ];

    let persistent_keys = [
        StorageKey::Balance(addr.clone()),
        StorageKey::Allowance(addr.clone(), Address::generate(&env)),
        StorageKey::Nonce(addr.clone()),
        StorageKey::History(addr.clone()),
    ];

    for ik in &instance_keys {
        for pk in &persistent_keys {
            assert_ne!(
                ik, pk,
                "Instance key {:?} must not collide with persistent key {:?}",
                ik, pk
            );
        }
    }
}

/// Deterministic serialization: the same key produces the same serialized value.
/// Verified through StorageKey equality (which derives PartialEq) since XDR
/// serialization is deterministic for the same key data.
#[test]
fn test_storage_key_deterministic_serialization() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    // Roundtrip: StorageKey -> Val -> StorageKey preserves identity
    let key = StorageKey::Balance(addr_a.clone());
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic Balance roundtrip");

    let key = StorageKey::Allowance(addr_a.clone(), addr_b.clone());
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic Allowance roundtrip");

    let key = StorageKey::RoleAdmin(Symbol::new(&env, "admin"));
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic RoleAdmin roundtrip");

    // Identically constructed keys are equal (deterministic construction)
    assert_eq!(
        StorageKey::Balance(addr_a.clone()),
        StorageKey::Balance(addr_a),
        "Identical Balance keys must be equal"
    );
}

/// Edge case: empty Symbol in RoleAdmin and RoleGrant.
#[test]
fn test_storage_key_empty_symbol_serialization() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let empty_role = Symbol::new(&env, "");
    let key_admin = StorageKey::RoleAdmin(empty_role.clone());
    let key_grant = StorageKey::RoleGrant(empty_role.clone(), addr);

    assert_key_roundtrip(&env, key_admin);
    assert_key_roundtrip(&env, key_grant);
}

/// Edge case: multi-byte Symbol values in StorageKey.
#[test]
fn test_storage_key_multibyte_symbol_serialization() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let long_role = Symbol::new(&env, "a_really_long_role_name_12345");
    let key = StorageKey::RoleGrant(long_role.clone(), addr.clone());
    assert_key_roundtrip(&env, key);

    let key = StorageKey::RoleAdmin(long_role);
    assert_key_roundtrip(&env, key);
}

/// StorageKey::Balance(addr) produces the same key value when cloned.
#[test]
fn test_storage_key_balance_clone_produces_same_key() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let key1 = StorageKey::Balance(addr.clone());
    let key2 = StorageKey::Balance(addr);
    assert_eq!(
        key1, key2,
        "Cloned Balance keys must produce identical values"
    );
}

/// Many different addresses produce distinct Balance storage keys.
#[test]
fn test_storage_key_many_balance_addresses_distinct() {
    let env = Env::default();
    let mut seen: Vec<StorageKey> = Vec::new(&env);

    for _ in 0..20 {
        let addr = Address::generate(&env);
        let key = StorageKey::Balance(addr);

        for existing in seen.iter() {
            assert_ne!(key, existing, "Generated Balance keys must be unique");
        }
        seen.push_back(key);
    }
}

/// Allowance key uniqueness across many address pairs.
#[test]
fn test_storage_key_many_allowance_pairs_distinct() {
    let env = Env::default();
    let mut seen: Vec<StorageKey> = Vec::new(&env);

    for _ in 0..15 {
        let from = Address::generate(&env);
        let spender = Address::generate(&env);
        let key = StorageKey::Allowance(from, spender);

        for existing in seen.iter() {
            assert_ne!(key, existing, "Generated Allowance keys must be unique");
        }
        seen.push_back(key);
    }

    // Also verify that the same pair repeated gives the same key (determinism)
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let k1 = StorageKey::Allowance(addr_a.clone(), addr_b.clone());
    let k2 = StorageKey::Allowance(addr_a, addr_b);
    assert_eq!(k1, k2, "Same Allowance pair must produce identical key");
}

/// Full lifecycle: write data via storage key, read it back, verify integrity.
#[test]
fn test_storage_key_total_supply_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Test");
    let symbol = SorobanString::from_str(&env, "TST");
    let invoice_id = Symbol::new(&env, "inv");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);

    // Initial total supply should be 0
    assert_eq!(client.total_supply(), 0);

    // Mint and verify total supply increases
    let user = Address::generate(&env);
    client.mint(&user, &5000, &minter);
    assert_eq!(client.total_supply(), 5000);

    // Burn and verify total supply decreases
    client.burn(&user, &2000);
    assert_eq!(client.total_supply(), 3000);
}

/// Verify StorageKey::Metadata lifecycle through full initialize + read flow.
#[test]
fn test_storage_key_metadata_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);

    // Verify metadata is persisted and accessible
    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert!(client.transfer_locked());

    // Update decimals - verify metadata mutation works via storage key
    client.set_decimals(&12);
    assert_eq!(client.decimals(), 12);
}

/// Verify StorageKey::Balance lifecycle: mint, transfer, balance queries.
#[test]
fn test_storage_key_balance_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    client.mint(&alice, &1000, &minter);
    assert_eq!(client.balance(&alice), 1000);

    // Transfer from alice to bob
    client.transfer(&alice, &bob, &400);
    assert_eq!(client.balance(&alice), 600);
    assert_eq!(client.balance(&bob), 400);

    // Verify uninitialized address returns 0
    let charlie = Address::generate(&env);
    assert_eq!(client.balance(&charlie), 0);
}

/// Verify StorageKey::Allowance lifecycle: approve, transfer_from, allowance queries.
#[test]
fn test_storage_key_allowance_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);
    client.mint(&admin, &2000, &minter);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    // Approve
    client.approve(&admin, &spender, &1000, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 1000);

    // Transfer from reduces allowance
    client.transfer_from(&spender, &admin, &recipient, &300);
    assert_eq!(client.allowance(&admin, &spender), 700);
    assert_eq!(client.balance(&recipient), 300);

    // Revoke approval
    client.revoke_approval(&admin, &spender);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

/// Verify StorageKey::Nonce lifecycle.
#[test]
fn test_storage_key_nonce_storage_lifecycle() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    // Initial nonce is 0
    assert_eq!(client.get_nonce(&admin), 0);

    // Querying nonce doesn't change it
    assert_eq!(client.get_nonce(&admin), 0);

    // Unknown account also returns 0
    let unknown = Address::generate(&env);
    assert_eq!(client.get_nonce(&unknown), 0);
}

/// Verify StorageKey::History lifecycle.
#[test]
fn test_storage_key_history_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice (no history entry for mint)
    client.mint(&alice, &1000, &minter);

    // Transfer from alice to bob should create a history record
    client.transfer(&alice, &bob, &400);

    // Check history for alice
    let alice_history = client.get_token_history(&alice);
    assert!(
        alice_history.len() > 0,
        "Alice should have history after transferring out"
    );

    // Check history for bob
    let bob_history = client.get_token_history(&bob);
    assert!(
        bob_history.len() > 0,
        "Bob should have history after receiving tokens"
    );
}

/// Verify StorageKey::FeeBps lifecycle.
#[test]
fn test_storage_key_fee_bps_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);

    // Default fee is 0
    assert_eq!(client.get_fee_bps(), 0);

    // Set fee
    client.set_fee_bps(&admin, &500);
    assert_eq!(client.get_fee_bps(), 500);

    // Update fee
    client.set_fee_bps(&admin, &1000);
    assert_eq!(client.get_fee_bps(), 1000);
}

/// Verify RoleAdmin and RoleGrant storage key lifecycles.
#[test]
fn test_storage_key_role_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);

    let account = Address::generate(&env);

    // Use a pre-configured role (admin) since `set_role_admin` requires the caller
    // to be the current admin of that role, and new roles have no admin.
    let role = Symbol::new(&env, "admin");

    // Initially the admin is the role admin (set during initialize)
    let role_admin: Address = client.get_role_admin(&role);
    assert_eq!(role_admin, admin.clone());

    // Change the role admin to someone else
    let new_admin = Address::generate(&env);
    client.set_role_admin(&admin, &role, &new_admin);
    let updated_admin: Address = client.get_role_admin(&role);
    assert_eq!(updated_admin, new_admin.clone());

    // Initially role not granted for a new account
    assert!(!client.has_role(&role, &account));

    // Grant role
    client.grant_role(&new_admin, &role, &account);
    assert!(client.has_role(&role, &account));

    // Revoke role
    client.revoke_role(&new_admin, &role, &account);
    assert!(!client.has_role(&role, &account));
}

/// Unconfigured roles return RoleNotGranted when queried.
#[test]
fn test_storage_key_unconfigured_role_returns_error() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    let custom_role = Symbol::new(&env, "nonexistent_role");
    let result = client.try_get_role_admin(&custom_role);
    assert_eq!(result, Err(Ok(crate::errors::Error::RoleNotGranted)));
}

/// Verify that two different roles with the same account produce distinct keys.
#[test]
fn test_storage_key_role_grant_cross_role_distinct() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);
    let account = Address::generate(&env);

    let role_a = Symbol::new(&env, "admin");
    let role_b = Symbol::new(&env, "minter");

    client.grant_role(&admin, &role_a, &account);
    client.grant_role(&admin, &role_b, &account);

    assert!(client.has_role(&role_a, &account));
    assert!(client.has_role(&role_b, &account));

    // Revoke only role_a
    client.revoke_role(&admin, &role_a, &account);
    assert!(!client.has_role(&role_a, &account));
    assert!(
        client.has_role(&role_b, &account),
        "Role B must still be granted after revoking role A"
    );
}
