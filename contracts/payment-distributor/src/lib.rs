#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::DistributionState;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

use errors::Error;

const ESCROW_STATUS_FUNDED: u32 = 1;
const ESCROW_STATUS_SETTLED: u32 = 2;
const ESCROW_STATUS_REFUNDED: u32 = 3;

#[contract]
pub struct PaymentDistributor;

fn get_distribution_state(
    env: &Env,
    escrow_contract: &Address,
    invoice_id: &Symbol,
) -> types::DistributionState {
    storage::get_distribution(env, escrow_contract, invoice_id).unwrap_or(
        types::DistributionState {
            paid_distributed: 0,
            refund_distributed: false,
        },
    )
}

#[contractimpl]
impl PaymentDistributor {
    /// Initialize the contract with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Distribute the latest settled payment delta for an escrow.
    ///
    /// The escrow contract must:
    /// 1. update its escrow state first,
    /// 2. transfer the settlement funds into this contract, and then
    /// 3. invoke this function as the configured distributor.
    pub fn distribute_payment(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
        addresses: Vec<Address>,
        amounts: Vec<i128>,
        escrow_status: u32,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        escrow_contract.require_auth();

        if escrow_status != ESCROW_STATUS_FUNDED && escrow_status != ESCROW_STATUS_SETTLED {
            return Err(Error::InvalidEscrowStatus);
        }
        if addresses.len() != 4 || amounts.len() != 4 {
            return Err(Error::InvalidAmount);
        }

        let token = addresses.get(0).ok_or(Error::InvalidAmount)?;
        let seller = addresses.get(1).ok_or(Error::InvalidAmount)?;
        let funder = addresses.get(2).ok_or(Error::InvalidAmount)?;
        let admin = addresses.get(3).ok_or(Error::InvalidAmount)?;
        let paid_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);
        let payment_amount = paid_amount
            .checked_sub(state.paid_distributed)
            .ok_or(Error::Overflow)?;

        if payment_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }
        let seller_amount = amounts.get(1).ok_or(Error::InvalidAmount)?;
        let investor_amount = amounts.get(2).ok_or(Error::InvalidAmount)?;
        let platform_fee = amounts.get(3).ok_or(Error::InvalidAmount)?;
        if seller_amount != payment_amount {
            return Err(Error::InvalidAmount);
        }
        let total_payer_distribution = investor_amount
            .checked_add(platform_fee)
            .ok_or(Error::Overflow)?;
        if total_payer_distribution != payment_amount {
            return Err(Error::InvalidAmount);
        }

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        token_client.transfer(&contract_addr, &seller, &seller_amount);
        token_client.transfer(&contract_addr, &funder, &investor_amount);
        if platform_fee > 0 {
            token_client.transfer(&contract_addr, &admin, &platform_fee);
        }

        state.paid_distributed = paid_amount;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        events::payment_distributed(
            &env,
            &escrow_contract,
            &invoice_id,
            &soroban_sdk::vec![&env, seller, funder, admin],
            &soroban_sdk::vec![
                &env,
                seller_amount,
                investor_amount,
                platform_fee,
                paid_amount
            ],
        );

        Ok(())
    }

    /// Distribute the final refund for a refunded escrow.
    pub fn distribute_refund(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
        addresses: Vec<Address>,
        amounts: Vec<i128>,
        escrow_status: u32,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        escrow_contract.require_auth();

        if escrow_status != ESCROW_STATUS_REFUNDED {
            return Err(Error::InvalidEscrowStatus);
        }
        if addresses.len() != 2 || amounts.len() != 1 {
            return Err(Error::InvalidAmount);
        }

        let token = addresses.get(0).ok_or(Error::InvalidAmount)?;
        let funder = addresses.get(1).ok_or(Error::InvalidAmount)?;
        let refund_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);
        if state.refund_distributed {
            return Err(Error::RefundAlreadyDistributed);
        }
        if refund_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        token_client.transfer(&contract_addr, &funder, &refund_amount);

        state.refund_distributed = true;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        events::refund_distributed(&env, &escrow_contract, &invoice_id, &funder, refund_amount);
        Ok(())
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }

    /// View: return tracked distribution progress for an escrow invoice.
    pub fn get_distribution_state(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
    ) -> Result<types::DistributionState, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(get_distribution_state(&env, &escrow_contract, &invoice_id))
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
