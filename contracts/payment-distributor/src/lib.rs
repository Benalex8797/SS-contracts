#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{BatchPaymentEntry, DistributionState};

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

use errors::Error;

const ESCROW_STATUS_FUNDED: u32 = 1;
const ESCROW_STATUS_SETTLED: u32 = 2;
const ESCROW_STATUS_REFUNDED: u32 = 3;

/// Maximum entries allowed in a single `distribute_batch` call.
/// Soroban transactions have bounded CPU/memory; this cap keeps batches safe.
const MAX_BATCH_SIZE: u32 = 50;

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

    /// Batch payment fanout: distribute settled payments for multiple invoices in one call.
    ///
    /// This function applies the same per-entry validation as `distribute_payment` but
    /// processes all entries atomically — either every transfer succeeds or the whole
    /// transaction is rolled back by the Soroban runtime.
    ///
    /// # Authorization
    /// Each entry's `escrow` address must have already authorised the corresponding
    /// transfer into this contract before this function is invoked.  Because Soroban
    /// host auth is checked lazily, callers should ensure all required auths are
    /// present in the transaction's auth envelope.  In practice the calling escrow
    /// contract invokes this function and the SDK records its auth automatically.
    ///
    /// # Constraints
    /// - `entries` must be non-empty.
    /// - `entries` must contain at most `MAX_BATCH_SIZE` (50) items.
    /// - Per entry: `status` must be `Funded` (1) or `Settled` (2).
    /// - Per entry: `seller_amt` must equal the new payment delta for this call.
    /// - Per entry: `investor_amt + fee_amt` must equal `seller_amt`.
    /// - Per entry: cumulative `paid_amt` must be strictly greater than the amount
    ///   already recorded in storage (no double-distribution).
    pub fn distribute_batch(
        env: Env,
        entries: Vec<BatchPaymentEntry>,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;

        let batch_len = entries.len();
        if batch_len == 0 {
            return Err(Error::EmptyBatch);
        }
        if batch_len > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }

        let contract_addr = env.current_contract_address();
        let mut total_distributed: i128 = 0;

        for i in 0..batch_len {
            let entry = entries.get(i).ok_or(Error::InvalidAmount)?;

            // Validate escrow status.
            if entry.status != ESCROW_STATUS_FUNDED && entry.status != ESCROW_STATUS_SETTLED {
                return Err(Error::InvalidEscrowStatus);
            }

            // Validate amounts are positive.
            if entry.paid_amt <= 0 || entry.seller_amt <= 0 {
                return Err(Error::InvalidAmount);
            }

            // Compute the new payment delta for this call.
            let mut state =
                get_distribution_state(&env, &entry.escrow, &entry.inv_id);
            let payment_delta = entry
                .paid_amt
                .checked_sub(state.paid_distributed)
                .ok_or(Error::Overflow)?;

            if payment_delta <= 0 {
                return Err(Error::NothingToDistribute);
            }

            // seller_amt must equal the delta for this call.
            if entry.seller_amt != payment_delta {
                return Err(Error::InvalidAmount);
            }

            // investor_amt + fee_amt must equal the delta.
            let investor_plus_fee = entry
                .investor_amt
                .checked_add(entry.fee_amt)
                .ok_or(Error::Overflow)?;
            if investor_plus_fee != payment_delta {
                return Err(Error::InvalidAmount);
            }

            // Transfer funds out of this contract to each recipient.
            let token_client = token::Client::new(&env, &entry.token);

            // Seller receives the face-value portion for this settlement.
            token_client.transfer(&contract_addr, &entry.seller, &entry.seller_amt);

            // Investor receives their net share.
            if entry.investor_amt > 0 {
                token_client.transfer(&contract_addr, &entry.funder, &entry.investor_amt);
            }

            // Admin receives the platform fee.
            if entry.fee_amt > 0 {
                token_client.transfer(&contract_addr, &entry.admin, &entry.fee_amt);
            }

            // Update persistent distribution state.
            state.paid_distributed = entry.paid_amt;
            storage::set_distribution(&env, &entry.escrow, &entry.inv_id, &state);

            // Accumulate for the batch event.
            total_distributed = total_distributed
                .checked_add(payment_delta)
                .ok_or(Error::Overflow)?;
        }

        events::batch_distributed(&env, batch_len, total_distributed);

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
