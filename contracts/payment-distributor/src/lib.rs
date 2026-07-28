#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{AssetRoute, DistributionPreview, DistributionSplit, DistributionState};

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

const OPERATOR_ROLE: &str = "operator";

use errors::Error;
use types::MAX_FEE_BPS;

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

/// Issue #127: Acquire the re-entrancy lock. Returns `ReentrancyDetected` if a
/// guarded entrypoint is already executing. Storage writes made after acquiring
/// the lock are rolled back automatically on any error return, so the lock is
/// cleared on both the success path (via `release_lock`) and any error path.
fn acquire_lock(env: &Env) -> Result<(), Error> {
    if storage::is_locked(env) {
        return Err(Error::ReentrancyDetected);
    }
    storage::set_lock(env, true);
    Ok(())
}

/// Issue #127: Release the re-entrancy lock on normal completion.
fn release_lock(env: &Env) {
    storage::set_lock(env, false);
}

/// Issue #129: Shared split-calculation core used by both `distribute_payment`
/// and the `calculate_distribution_splits` dry-run getter. Computes the
/// seller/investor/fee split for a payment delta with no storage writes or
/// token transfers.
///
/// Issue #132: Rounding losses are allocated to the seller so the total
/// distribution equals `payment_amount` exactly.
fn compute_split(
    paid_amount: i128,
    already_distributed: i128,
    investor_amount: i128,
    fee_bps_u32: u32,
) -> Result<types::DistributionPreview, Error> {
    if fee_bps_u32 > MAX_FEE_BPS {
        return Err(Error::InvalidBps);
    }

    let payment_amount = paid_amount
        .checked_sub(already_distributed)
        .ok_or(Error::Overflow)?;

    if payment_amount <= 0 {
        return Err(Error::NothingToDistribute);
    }

    let platform_fee = payment_amount
        .checked_mul(fee_bps_u32 as i128)
        .ok_or(Error::Overflow)?
        .checked_div(10_000)
        .ok_or(Error::Overflow)?;

    let seller_amount = payment_amount
        .checked_sub(investor_amount)
        .ok_or(Error::Overflow)?
        .checked_sub(platform_fee)
        .ok_or(Error::Overflow)?;

    if seller_amount < 0 {
        return Err(Error::InvalidAmount);
    }

    let total_distribution = seller_amount
        .checked_add(investor_amount)
        .ok_or(Error::Overflow)?
        .checked_add(platform_fee)
        .ok_or(Error::Overflow)?;

    if total_distribution != payment_amount {
        return Err(Error::InvalidAmount);
    }

    Ok(types::DistributionPreview {
        seller_amount,
        investor_amount,
        platform_fee,
        total_distribution,
    })
}

/// Issue #126 / #130: Distribute a single asset `amount` across `split`, taking the
/// optional referral cut off the top and allocating any rounding remainder to the
/// primary (index 0) recipient so the full amount is distributed with no dust.
fn distribute_split(
    env: &Env,
    token: &Address,
    from: &Address,
    amount: i128,
    split: &DistributionSplit,
) -> Result<(), Error> {
    split.validate()?;
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }

    let token_client = token::Client::new(env, token);

    // Referral cut off the top.
    let referral_amount = if split.referral_bps > 0 {
        amount
            .checked_mul(split.referral_bps as i128)
            .ok_or(Error::Overflow)?
            .checked_div(10_000)
            .ok_or(Error::Overflow)?
    } else {
        0
    };

    // Compute recipient amounts: recipients[1..] by their basis-point share and
    // recipients[0] as the exact residual so the total equals `amount`.
    let n = split.recipients.len();
    let mut allocated = referral_amount;
    let mut rec_amounts: Vec<i128> = Vec::new(env);
    for i in 0..n {
        if i == 0 {
            rec_amounts.push_back(0); // residual placeholder, set below
        } else {
            let share = split.shares_bps.get(i).ok_or(Error::InvalidSplit)?;
            let amt = amount
                .checked_mul(share as i128)
                .ok_or(Error::Overflow)?
                .checked_div(10_000)
                .ok_or(Error::Overflow)?;
            allocated = allocated.checked_add(amt).ok_or(Error::Overflow)?;
            rec_amounts.push_back(amt);
        }
    }
    let primary = amount.checked_sub(allocated).ok_or(Error::Overflow)?;
    if primary < 0 {
        return Err(Error::InvalidSplit);
    }
    rec_amounts.set(0, primary);

    let mut recipients_out: Vec<Address> = Vec::new(env);
    let mut amounts_out: Vec<i128> = Vec::new(env);

    // Pay the referral cut first.
    if referral_amount > 0 {
        let referral = split.referral.clone().ok_or(Error::InvalidReferralCut)?;
        token_client.transfer(from, &referral, &referral_amount);
        events::referral_paid(env, token, &referral, referral_amount);
        recipients_out.push_back(referral);
        amounts_out.push_back(referral_amount);
    }

    // Pay the primary recipients.
    for i in 0..n {
        let recipient = split.recipients.get(i).ok_or(Error::InvalidSplit)?;
        let amt = rec_amounts.get(i).ok_or(Error::InvalidSplit)?;
        if amt > 0 {
            token_client.transfer(from, &recipient, &amt);
        }
        recipients_out.push_back(recipient);
        amounts_out.push_back(amt);
    }

    events::asset_distributed(env, token, &recipients_out, &amounts_out, amount);
    Ok(())
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
        // Issue #127: Re-entrancy barrier. Any error return below rolls back this
        // storage write, so the lock is cleared on both success and failure paths.
        acquire_lock(&env)?;

        storage::get_admin(&env).ok_or(Error::NotInit)?;
        escrow_contract.require_auth();

        // Issue #131: Opt-in whitelist enforcement. If no escrow contract has been
        // bound via `set_escrow_contract`, behavior is unchanged (open). Once bound,
        // only that exact address may call `distribute_payment`.
        if let Some(whitelisted) = storage::get_escrow_contract(&env) {
            if whitelisted != escrow_contract {
                return Err(Error::UnauthorizedEscrow);
            }
        }

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

        let paid_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let investor_amount = amounts.get(2).ok_or(Error::InvalidAmount)?;
        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);

        // Issue #132: Automated fee rounding loss minimization, computed via the
        // shared `compute_split` core also used by the Issue #129 dry-run getter.
        let preview = compute_split(
            paid_amount,
            state.paid_distributed,
            investor_amount,
            fee_bps_u32,
        )?;
        let platform_fee = preview.platform_fee;
        let seller_amount = preview.seller_amount;

        // Issue #122: Use configured fee recipient (fallback to admin if not set)
        let fee_recipient = storage::get_fee_recipient(&env)
            .unwrap_or_else(|| storage::get_admin(&env).ok_or(Error::NotInit).unwrap());

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();

        if token_client.balance(&contract_addr) < payment_amount {
            return Err(Error::InsufficientBalance);
        }

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

        // Issue #127: Clear the re-entrancy lock on normal completion.
        release_lock(&env);
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
        acquire_lock(&env)?;
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

        if token_client.balance(&contract_addr) < refund_amount {
            return Err(Error::InsufficientBalance);
        }

        token_client.transfer(&contract_addr, &funder, &refund_amount);

        state.refund_distributed = true;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        events::refund_distributed(&env, &escrow_contract, &invoice_id, &funder, refund_amount);
        release_lock(&env);
        Ok(())
    }

    /// Issue #126: Multi-currency payment distribution routing.
    ///
    /// Routes each supplied asset to its recipients according to its split config.
    /// The tokens must already be held by this contract. Admin-only.
    ///
    /// Reuses the single-asset [`distribute_split`] logic and emits an
    /// `AssetDistributed` event per asset (and a `referral_paid` event when a
    /// referral cut is taken).
    pub fn distribute_multi_asset(
        env: Env,
        caller: Address,
        routes: Vec<AssetRoute>,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        let operator_role = Symbol::new(&env, OPERATOR_ROLE);
        let is_operator = storage::has_role(&env, &operator_role, &caller);
        if caller != stored_admin && !is_operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        if routes.is_empty() {
            return Err(Error::EmptyAssetList);
        }

        // Reject duplicate assets: each token must appear at most once.
        let len = routes.len();
        for i in 0..len {
            let token_i = routes.get(i).ok_or(Error::EmptyAssetList)?.token;
            for j in (i + 1)..len {
                let token_j = routes.get(j).ok_or(Error::EmptyAssetList)?.token;
                if token_i == token_j {
                    return Err(Error::AssetMismatch);
                }
            }
        }

        // Issue #127: Guard the external transfers against re-entrancy.
        acquire_lock(&env)?;

        let contract_addr = env.current_contract_address();
        for i in 0..len {
            let route = routes.get(i).ok_or(Error::EmptyAssetList)?;
            distribute_split(
                &env,
                &route.token,
                &contract_addr,
                route.amount,
                &route.split,
            )?;
        }

        release_lock(&env);
        Ok(())
    }

    /// Issue #125: Emergency withdrawal safeguard.
    ///
    /// Admin-only escape hatch that transfers the contract's entire held balance of
    /// `token` to a safe `to` address. Fails with `NothingToWithdraw` when the
    /// contract holds no balance of the token. Emits an `EmergencyWithdrawal` event.
    pub fn emergency_withdraw(
        env: Env,
        admin: Address,
        token: Address,
        to: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        let balance = token_client.balance(&contract_addr);
        if balance <= 0 {
            return Err(Error::NothingToWithdraw);
        }

        token_client.transfer(&contract_addr, &to, &balance);
        events::emergency_withdrawal(&env, &admin, &token, &to, balance);
        Ok(())
    }

    /// Issue #119: Implement Dust Amount Collector and Sweep Function.
    ///
    /// Admin-only function to sweep leftover token balances to the configured
    /// fee recipient (or admin if not set).
    pub fn sweep_dust(
        env: Env,
        admin: Address,
        token: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        let balance = token_client.balance(&contract_addr);
        if balance <= 0 {
            return Err(Error::NothingToSweep);
        }

        let fee_recipient = storage::get_fee_recipient(&env)
            .unwrap_or_else(|| stored_admin.clone());

        token_client.transfer(&contract_addr, &fee_recipient, &balance);
        events::dust_swept(&env, &admin, &token, &fee_recipient, balance);
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

    /// Issue #122: Set the fee recipient address for platform fees.
    /// Only the admin can update the fee recipient.
    /// Emits a fee_recipient_updated event for audit trails.
    pub fn set_fee_recipient(
        env: Env,
        admin: Address,
        new_recipient: Address,
    ) -> Result<(), Error> {
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
        Ok(storage::get_fee_recipient(&env)
            .unwrap_or_else(|| storage::get_admin(&env).expect("Admin must be set")))
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

    /// Issue #121: Bind the whitelisted escrow contract address allowed to call
    /// `distribute_payment`. Only the admin can update this binding.
    /// Emits an `escrow_contract_updated` event for audit trails.
    pub fn set_escrow_contract(env: Env, admin: Address, new_escrow: Address) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let old_escrow = storage::get_escrow_contract(&env);
        storage::set_escrow_contract(&env, &new_escrow);
        events::escrow_contract_updated(&env, old_escrow, &new_escrow);
        Ok(())
    }

    /// View: return the currently whitelisted escrow contract, if any bound.
    pub fn get_escrow_contract(env: Env) -> Result<Option<Address>, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_escrow_contract(&env))
    }

    /// Issue #129: Dry-run preview of a `distribute_payment` split. Performs no
    /// storage writes, no token transfers, and no auth checks — a pure view
    /// into what `distribute_payment` would compute for the same inputs.
    pub fn calculate_distribution_splits(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
        addresses: Vec<Address>,
        amounts: Vec<i128>,
    ) -> Result<DistributionPreview, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;

        if addresses.len() != 4 || amounts.len() != 4 {
            return Err(Error::InvalidAmount);
        }

        let fee_bps_u32 = amounts.get(3).ok_or(Error::InvalidAmount)? as u32;
        let paid_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let investor_amount = amounts.get(2).ok_or(Error::InvalidAmount)?;
        let state = get_distribution_state(&env, &escrow_contract, &invoice_id);

        compute_split(
            paid_amount,
            state.paid_distributed,
            investor_amount,
            fee_bps_u32,
        )
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
