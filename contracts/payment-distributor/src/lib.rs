#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::DistributionState;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

use errors::Error;
use types::MAX_FEE_BPS;

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
    ///
    /// Issue #132: Implements automated fee rounding loss minimization.
    /// Rounding losses are allocated to the seller to ensure exact total distribution.
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
        let fee_bps_u32 = amounts.get(3).ok_or(Error::InvalidAmount)? as u32;
        
        // Issue #124: Validate fee BPS does not exceed maximum (10,000 = 100%)
        if fee_bps_u32 > MAX_FEE_BPS {
            return Err(Error::InvalidBps);
        }

        let paid_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);
        let payment_amount = paid_amount
            .checked_sub(state.paid_distributed)
            .ok_or(Error::Overflow)?;

        if payment_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }

        // Issue #132: Automated fee rounding loss minimization
        // Calculate platform fee, investor amount, then allocate any rounding loss to seller
        let platform_fee = (payment_amount as i128)
            .checked_mul(fee_bps_u32 as i128)
            .ok_or(Error::Overflow)?
            .checked_div(10_000)
            .ok_or(Error::Overflow)?;

        let investor_amount = amounts.get(2).ok_or(Error::InvalidAmount)?;
        
        // Seller receives: payment_amount - investor_amount - platform_fee
        // This automatically absorbs any rounding loss
        let seller_amount = payment_amount
            .checked_sub(investor_amount)
            .ok_or(Error::Overflow)?
            .checked_sub(platform_fee)
            .ok_or(Error::Overflow)?;

        if seller_amount < 0 {
            return Err(Error::InvalidAmount);
        }

        // Verify total distribution equals payment_amount exactly
        let total_distribution = seller_amount
            .checked_add(investor_amount)
            .ok_or(Error::Overflow)?
            .checked_add(platform_fee)
            .ok_or(Error::Overflow)?;
        
        if total_distribution != payment_amount {
            return Err(Error::InvalidAmount);
        }

        // Issue #122: Use configured fee recipient (fallback to admin if not set)
        let fee_recipient = storage::get_fee_recipient(&env)
            .unwrap_or_else(|| storage::get_admin(&env).ok_or(Error::NotInit).unwrap());

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        
        token_client.transfer(&contract_addr, &seller, &seller_amount);
        token_client.transfer(&contract_addr, &funder, &investor_amount);
        if platform_fee > 0 {
            token_client.transfer(&contract_addr, &fee_recipient, &platform_fee);
        }

        state.paid_distributed = paid_amount;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        // Issue #123: Enhanced structured payment distribution audit event
        events::payment_distributed(
            &env,
            &escrow_contract,
            &invoice_id,
            &soroban_sdk::vec![&env, seller, funder, fee_recipient],
            &soroban_sdk::vec![
                &env,
                seller_amount,
                investor_amount,
                platform_fee,
                paid_amount
            ],
            escrow_status,
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

    /// Issue #122: Set the fee recipient address for platform fees.
    /// Only the admin can update the fee recipient.
    /// Emits a fee_recipient_updated event for audit trails.
    pub fn set_fee_recipient(env: Env, admin: Address, new_recipient: Address) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let old_recipient = storage::get_fee_recipient(&env);
        storage::set_fee_recipient(&env, &new_recipient);
        events::fee_recipient_updated(&env, old_recipient, &new_recipient);
        Ok(())
    }

    /// View: return the current fee recipient (defaults to admin if not set).
    pub fn get_fee_recipient(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_fee_recipient(&env).unwrap_or_else(|| {
            storage::get_admin(&env).expect("Admin must be set")
        }))
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
