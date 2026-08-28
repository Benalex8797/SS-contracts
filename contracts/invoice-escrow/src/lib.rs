//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, IntoVal, Symbol};

use types::MultiSigConfig;

// EscrowStatus is re-exported publicly; Config, EscrowData, and InvoiceData are crate-private.
pub use types::EscrowStatus;
use types::{Config, EscrowData, FundingInvoice, InvoiceData, InvoiceStatus};

use errors::Error;

/// Reject the zero address (all-zero 32-byte Ed25519 key) which is never a valid participant.
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    // Convert address to its string representation and check for the well-known
    // zero account (all 32 bytes are 0x00).  The StrKey encoding of the zero
    // account is the constant below.
    let zero_str = soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

const MAX_BPS: u32 = 10_000;
const DISTRIBUTE_PAYMENT_FN: &str = "distribute_payment";
const DISTRIBUTE_REFUND_FN: &str = "distribute_refund";

/// Minimum escrow duration: 1 hour (3600 seconds).
const MIN_ESCROW_DURATION_SECS: u64 = 3_600;
/// Maximum escrow duration: 365 days (31,536,000 seconds).
const MAX_ESCROW_DURATION_SECS: u64 = 31_536_000;

#[contract]
pub struct InvoiceEscrow;

fn ensure_not_paused(config: &Config) -> Result<(), Error> {
    if config.paused {
        return Err(Error::Paused);
    }
    Ok(())
}

#[contractimpl]
impl InvoiceEscrow {
    /// Initialize the contract with admin and platform fee (basis points, e.g. 300 = 3%).
    pub fn initialize(env: Env, admin: Address, platform_fee_bps: u32) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if platform_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let config = Config {
            admin: admin.clone(),
            fee_bps: platform_fee_bps,
            payment_distributor: None,
            paused: false,
            whitelist_enabled: false,
            min_investment: 0,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: set the minimum investment amount for `fund_escrow`.
    /// Pass `0` to disable the floor (deposits must still be strictly positive).
    pub fn set_min_investment(env: Env, admin: Address, min_investment: i128) -> Result<(), Error> {
        admin.require_auth();
        if min_investment < 0 {
            return Err(Error::InvalidAmount);
        }
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.min_investment = min_investment;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: enable/disable buyer whitelist enforcement on `fund_escrow`.
    pub fn set_whitelist_enabled(env: Env, admin: Address, enabled: bool) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.whitelist_enabled = enabled;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: add or remove a buyer from the whitelist.
    pub fn set_buyer_whitelisted(
        env: Env,
        admin: Address,
        buyer: Address,
        allowed: bool,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::set_whitelisted(&env, &buyer, allowed);
        Ok(())
    }

    /// View: is `buyer` whitelisted to fund escrows.
    pub fn is_buyer_whitelisted(env: Env, buyer: Address) -> bool {
        storage::is_whitelisted(&env, &buyer)
    }

    /// Create an escrow for an invoice. Caller (seller) must be authenticated.
    /// face_value: what the debtor owes (amount to be paid at settlement)
    /// purchase_price: what the investor pays (discount applied here)
    /// commitment: immutable on-chain anchor (SHA-256 hash of off-chain invoice data)
    pub fn create_escrow(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        debtor: Address,
        face_value: i128,
        purchase_price: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
        commitment: soroban_sdk::BytesN<32>,
        funding_milestone: Option<i128>,
    ) -> Result<(), Error> {
        seller.require_auth();
        if face_value <= 0 || purchase_price <= 0 {
            return Err(Error::InvalidAmount);
        }
        if due_date == 0 {
            return Err(Error::InvalidDueDate);
        }
        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(Error::InvalidDueDate);
        }
        let duration = due_date.saturating_sub(current_timestamp);
        if duration < MIN_ESCROW_DURATION_SECS || duration > MAX_ESCROW_DURATION_SECS {
            return Err(Error::InvalidDuration);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        // Ensure the payment token and invoice token use the same decimals to avoid
        // settlement/rounding mismatches during distribution and fee calculations.
        let inv_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &invoice_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        let pay_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &payment_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        if let (Some(inv_d), Some(pay_d)) = (inv_decimals, pay_decimals) {
            if inv_d != pay_d {
                return Err(Error::InvalidAssetDecimals);
            }
        }
        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            debtor: debtor.clone(),
            face_value,
            purchase_price,
            funded_amt: 0,
            funder: None,
            funders: soroban_sdk::Vec::new(&env),
            due_dt: due_date,
            token: payment_token.clone(),
            inv_token: invoice_token.clone(),
            paid_amt: 0,
            status: EscrowStatus::Created,
            funding_milestone,
            commitment: commitment.clone(),
            early_settlement: None,
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);
        
        // Store the invoice_id at the current index for pagination
        let current_count = storage::get_escrow_count(&env);
        storage::set_escrow_id_by_index(&env, current_count, &invoice_id);
        storage::increment_escrow_count(&env);
        
        events::escrow_created(
            &env,
            invoice_id.clone(),
            &seller,
            &debtor,
            face_value,
            purchase_price,
            due_date,
            &payment_token,
            &invoice_token,
            &commitment,
            data.funding_milestone,
        );
        events::escrow_status_changed(&env, invoice_id, EscrowStatus::Created, current_timestamp);
        Ok(())
    }

    /// Cancel an escrow in Created state, refunding any partial funds to the funders.
    /// Only the seller may cancel, and only while status is Created.
    /// Cancel an unfunded escrow. Only the seller may cancel, and only while status is Created
    /// AND no investor has contributed any funds yet.
    ///
    /// Locked out after partial payment: `fund_escrow` accepts partial contributions and only
    /// flips `status` to `Funded` once the escrow is fully subscribed, so an escrow with
    /// `funded_amt > 0` can still read as `Created`. Cancelling in that window would strand the
    /// investor's already-transferred funds (cancellation has no refund path), so any nonzero
    /// `funded_amt` blocks cancellation regardless of status.
    ///
    /// Emits `escrow_refunded` (if partial funds existed) and `escrow_cancelled`.
    pub fn cancel_escrow(env: Env, invoice_id: Symbol, seller: Address) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status == EscrowStatus::Funded {
            return Err(Error::EscrowFunded);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::CancelNotAllowed);
        }

        if data.funded_amt > 0 {
            let amount_to_refund = data.funded_amt;
            let token = token::Client::new(&env, &data.token);
            let contract = env.current_contract_address();
            let funder_opt = data.funder.clone();

            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (EscrowStatus::Cancelled as u32).into_val(&env)
                    ],
                );
            } else {
                if let Some(funder) = &funder_opt {
                    let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                    if funder_amt > 0 {
                        token.transfer(&contract, funder, &funder_amt);
                    }
                }
            }

            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
            events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        }
        data.status = EscrowStatus::Cancelled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_cancelled(&env, invoice_id.clone(), &seller);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Cancelled,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Seller-only: attach or update the early-settlement discount hook for a Created or Funded escrow.
    ///
    /// Rules:
    /// - Only callable by the escrow's seller.
    /// - `discount_bps` must be in [1, 9999]. A zero discount is meaningless; 10 000 bps
    ///   (100%) would collapse the effective face value to zero, so it is rejected.
    /// - `cutoff_date` must be strictly in the future and must not exceed `due_dt`.
    /// - Cannot be set on an escrow that has already reached a terminal state
    ///   (Settled, Refunded, Cancelled).
    /// - Can be called multiple times to update the config (e.g., extend the window
    ///   or adjust the rate) as long as the escrow is still live.
    pub fn set_early_settlement(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        discount_bps: u32,
        cutoff_date: u64,
    ) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }

        // Terminal states: hook can no longer be meaningful
        match data.status {
            EscrowStatus::Settled | EscrowStatus::Refunded | EscrowStatus::Cancelled => {
                return Err(Error::InvalidEarlySettlement);
            }
            _ => {}
        }

        // Validate discount_bps: must be [1, 9999]
        if discount_bps == 0 || discount_bps >= MAX_BPS {
            return Err(Error::InvalidEarlySettlement);
        }

        // cutoff_date must be strictly in the future
        let now = env.ledger().timestamp();
        if cutoff_date <= now {
            return Err(Error::InvalidEarlySettlement);
        }
        // cutoff_date must not exceed due_dt (no discount window past maturity)
        if cutoff_date > data.due_dt {
            return Err(Error::InvalidEarlySettlement);
        }

        data.early_settlement = Some(EarlySettlementConfig {
            discount_bps,
            cutoff_date,
        });
        storage::set_escrow(&env, invoice_id, &data);
        Ok(())
    }

    /// Fund the escrow (investor buys part or all of the invoice at purchase_price).
    /// Transfers `amount` from buyer to this contract. Multiple investors can fund until fully subscribed.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        buyer.require_auth();
        Self::fund_escrow_core(&env, invoice_id, &buyer, amount)
    }

    /// Fund the escrow on behalf of `buyer` using a signed off-chain approval that a relayer
    /// submits on their behalf. `buyer` authorizes exactly this `(invoice_id, amount, nonce, expiry)`
    /// tuple, and `nonce` must be strictly greater than the last nonce consumed by `buyer` so
    /// the same signed approval cannot be replayed.
    ///
    /// Issue #183: Includes an `expiry` timestamp. If the ledger timestamp exceeds `expiry`
    /// the signature is rejected, limiting the window for replay attacks.
    pub fn fund_escrow_signed(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        nonce: u64,
        expiry: u64,
    ) -> Result<(), Error> {
        buyer.require_auth_for_args((invoice_id.clone(), amount, nonce, expiry).into_val(&env));

        let current_ts = env.ledger().timestamp();
        if current_ts > expiry {
            return Err(Error::SignatureExpired);
        }

        let last_nonce = storage::get_nonce(&env, &buyer);
        if nonce <= last_nonce {
            return Err(Error::NonceAlreadyUsed);
        }

        Self::fund_escrow_core(&env, invoice_id.clone(), &buyer, amount)?;

        storage::set_nonce(&env, &buyer, nonce);
        events::escrow_funded_signed(&env, invoice_id, &buyer, amount, nonce);
        Ok(())
    }

    /// Shared funding logic used by both the directly-authorized and signed-approval entry points.
    fn fund_escrow_core(
        env: &Env,
        invoice_id: Symbol,
        buyer: &Address,
        amount: i128,
    ) -> Result<(), Error> {
        // Fail fast: validate amount before hitting storage.
        if amount == 0 {
            return Err(Error::ZeroAmount);
        }
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if config.whitelist_enabled && !storage::is_whitelisted(env, buyer) {
            return Err(Error::NotWhitelisted);
        }

        let mut data = storage::get_escrow(env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }

        // Check that funding doesn't exceed purchase_price
        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        let remaining_to_fund = data
            .purchase_price
            .checked_sub(data.funded_amt)
            .ok_or(Error::Overflow)?;

        // Enforce global minimum investment to prevent dust deposits, except when
        // the funder is completing the exact remaining capacity.
        if config.min_investment > 0
            && amount != remaining_to_fund
            && amount < config.min_investment
        {
            return Err(Error::AmountBelowMinimum);
        }

        // Validate milestone constraints if a milestone is set
        if let Some(milestone) = data.funding_milestone {
            // Funder is always allowed to just fund exactly the remaining amount to complete the escrow.
            // If they are not completing the escrow, the amount must be at least the milestone and a multiple of it.
            if amount != remaining_to_fund && (amount < milestone || amount % milestone != 0) {
                return Err(Error::InvalidMilestoneAmount);
            }
        }

        let token = token::Client::new(env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent their ownership share
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(env, "mint"),
            soroban_sdk::vec![env, buyer.to_val(), amount.into_val(env), contract.to_val()],
        );

        // Track this funder's contribution
        let current_funder_amt = storage::get_funder_amount(env, invoice_id.clone(), buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(env, invoice_id.clone(), buyer, new_funder_amt);

        data.funded_amt = new_funded;

        let mut already_recorded = false;
        for funder in data.funders.iter() {
            if funder == buyer.clone() {
                already_recorded = true;
                break;
            }
        }
        if !already_recorded {
            data.funders.push_back(buyer.clone());
        }

        // MVP: Store the first funder for direct distribution
        if data.funder.is_none() {
            data.funder = Some(buyer.clone());
        }

        // If fully funded, transition to Funded status
        if data.funded_amt == data.purchase_price {
            data.status = EscrowStatus::Funded;
        }

        storage::set_escrow(env, invoice_id.clone(), &data);
        events::escrow_funded(
            env,
            invoice_id.clone(),
            buyer,
            amount,
            data.funded_amt,
            data.purchase_price,
        );
        if data.status == EscrowStatus::Funded {
            events::escrow_status_changed(
                env,
                invoice_id,
                EscrowStatus::Funded,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Record payment: distribute to investors and platform fee. Payer must auth.
    /// Payer must be the authorized debtor for this invoice.
    /// Payment is applied toward face_value; fees are calculated on the payment amount.
    /// MVP: Distributes pro-rata to all funders based on their contribution.
    pub fn record_payment(
        env: Env,
        invoice_id: Symbol,
        payer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        payer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Enforce payer role: payer must be the authorized debtor
        if payer != data.debtor {
            return Err(Error::InvalidPayer);
        }

        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }

        // Compute effective face value: apply early-settlement discount if the hook
        // is configured and the payment arrives strictly before the cutoff date.
        let current_ts = env.ledger().timestamp();
        let effective_face_value =
            if let Some(ref es) = data.early_settlement {
                if current_ts < es.cutoff_date {
                    let discount = data
                        .face_value
                        .checked_mul(i128::from(es.discount_bps))
                        .ok_or(Error::Overflow)?
                        .checked_div(i128::from(MAX_BPS))
                        .ok_or(Error::Overflow)?;
                    let discounted = data
                        .face_value
                        .checked_sub(discount)
                        .ok_or(Error::Overflow)?
                        .max(1); // floor at 1 stroop
                    // Emit the hook application event (only on first payment in the window
                    // to avoid redundant emissions on subsequent partial payments).
                    if data.paid_amt == 0 {
                        events::early_settlement_applied(
                            &env,
                            invoice_id.clone(),
                            es.discount_bps,
                            data.face_value,
                            discounted,
                        );
                    }
                    discounted
                } else {
                    data.face_value
                }
            } else {
                data.face_value
            };

        // Remaining balance toward effective face value
        let remaining = effective_face_value
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
            return Err(Error::InvalidAmount);
        }

        let fee_bps = i128::from(config.fee_bps);
        // Fee is calculated on the payment amount (not face_value)
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // 1. Pull payer's funds into escrow
        token.transfer(&payer, &contract, &amount);

        data.paid_amt = data.paid_amt.checked_add(amount).ok_or(Error::Overflow)?;

        // Settlement occurs when paid_amt reaches effective face value
        if data.paid_amt == effective_face_value {
            data.status = EscrowStatus::Settled;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);

        let funder_addr = data.funder.clone().unwrap_or_else(|| data.seller.clone());

        if let Some(distributor) = config.payment_distributor.as_ref() {
            // The distributor must pay seller_amount (== amount) plus investor_amount + platform_fee
            // (== amount), mirroring the direct path below which releases the payer's `amount` to the
            // seller in addition to paying the investor/admin out of escrow's held funding.
            let total_to_distributor = amount.checked_add(amount).ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);
            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    invoice_id.clone().into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.token, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.seller, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&funder_addr, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&config.admin, &env)
                    ]
                    .into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        data.paid_amt,
                        amount,
                        investor_amount,
                        config.fee_bps as i128,
                    ]
                    .into_val(&env),
                    (data.status as u32).into_val(&env)
                ],
            );
        } else {
            // 2. Platform fee to admin
            token.transfer(&contract, &config.admin, &platform_fee);

            // 3. Pro-rata investor distribution
            if let Some(funder) = &data.funder {
                if data.funded_amt > 0 && investor_amount > 0 {
                    let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                    let pro_rata_share = investor_amount
                        .checked_mul(funder_amt)
                        .ok_or(Error::Overflow)?
                        .checked_div(data.funded_amt)
                        .ok_or(Error::Overflow)?;
                    if pro_rata_share > 0 {
                        token.transfer(&contract, funder, &pro_rata_share);
                    }
                }
            }
            // Seller receives the full payment amount
            token.transfer(&contract, &data.seller, &amount);
        }

        if data.status == EscrowStatus::Settled {
            // Unlock invoice token transfers only when the invoice is completely settled.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

        events::payment_settled(
            &env,
            invoice_id.clone(),
            amount,
            platform_fee,
            investor_amount,
        );
        if data.status == EscrowStatus::Settled {
            events::escrow_status_changed(
                &env,
                invoice_id,
                EscrowStatus::Settled,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Refund the investors if the invoice was not paid by due date. Anyone may call.
    /// Refunds are distributed pro-rata based on each investor's contribution.
    pub fn refund_escrow(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::RefundNotAllowed);
        }
        let ledger_ts = env.ledger().timestamp();
        if ledger_ts < data.due_dt {
            return Err(Error::RefundNotAllowed);
        }

        // Refund the remaining collateral (purchase_price minus already released partial payments)
        let amount_to_refund = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Extract funder address before status mutation so it is available in both paths.
        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (data.status as u32).into_val(&env)
                    ],
                );
            } else {
                // Pro-rata refund to funders
                if let Some(funder) = &funder_opt {
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), funder);
                        let pro_rata_refund = amount_to_refund
                            .checked_mul(funder_amt)
                            .ok_or(Error::Overflow)?
                            .checked_div(data.funded_amt)
                            .ok_or(Error::Overflow)?;
                        if pro_rata_refund > 0 {
                            token.transfer(&contract, funder, &pro_rata_refund);
                        }
                    }
                }
            }
        }

        // Unlock invoice token transfers now that the invoice is refunded
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Refunded,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Update platform fee (basis points). Admin only.
    pub fn update_platform_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        if new_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let old_fee_bps = config.fee_bps;
        config.fee_bps = new_fee_bps;
        storage::set_config(&env, &config);
        events::platform_fee_updated(&env, old_fee_bps, new_fee_bps);
        Ok(())
    }

    /// Set the payment distributor used for settlement/refund fan-out. Admin only.
    pub fn set_payment_distributor(env: Env, payment_distributor: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_distributor = config.payment_distributor.clone();
        config.payment_distributor = Some(payment_distributor.clone());
        storage::set_config(&env, &config);
        events::payment_distributor_updated(&env, old_distributor.is_some(), &payment_distributor);
        Ok(())
    }

    /// Toggle the emergency pause flag. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_paused = config.paused;
        config.paused = paused;
        storage::set_config(&env, &config);
        events::paused_updated(&env, old_paused, paused);
        Ok(())
    }

    /// View: return escrow data for an invoice, or Err(Error::EscrowNotFound) if not found.
    pub fn get_escrow(env: Env, invoice_id: Symbol) -> Result<EscrowData, Error> {
        storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return current config (admin and fee_bps).
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return escrow status for an invoice.
    pub fn get_escrow_status(env: Env, invoice_id: Symbol) -> Result<EscrowStatus, Error> {
        let data = storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(data.status)
    }

    /// View: return the current pause state.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.paused)
    }

    /// Admin-only: configure the emergency multi-sig admin set and threshold.
    pub fn set_emergency_config(
        env: Env,
        admin: Address,
        config: MultiSigConfig,
    ) -> Result<(), Error> {
        admin.require_auth();
        let stored_config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if stored_config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if config.threshold == 0 || config.threshold > config.admins.len() as u32 {
            return Err(Error::InvalidFeeBps); // reuse for invalid threshold
        }
        storage::set_emergency_config(&env, &config);
        Ok(())
    }

    /// Emergency multi-sig release: an admin approves releasing funds for an invoice.
    /// When the threshold is reached, funds are paid out to the seller and the escrow
    /// is marked as Settled.
    pub fn emergency_release(env: Env, caller: Address, invoice_id: Symbol) -> Result<bool, Error> {
        caller.require_auth();
        let config = storage::get_emergency_config(&env).ok_or(Error::EmergencyNotConfigured)?;

        // Verify caller is an emergency admin
        let mut is_admin = false;
        for admin in config.admins.iter() {
            if admin == caller {
                is_admin = true;
                break;
            }
        }
        if !is_admin {
            return Err(Error::NotEmergencyAdmin);
        }

        let mut approvals = storage::get_emergency_approvals(&env, &invoice_id);

        // Check for duplicate approval
        for addr in approvals.approvals.iter() {
            if addr == caller {
                return Err(Error::AlreadyApproved);
            }
        }

        approvals.approvals.push_back(caller.clone());
        storage::set_emergency_approvals(&env, &invoice_id, &approvals);

        if (approvals.approvals.len() as u32) < config.threshold {
            return Ok(false);
        }

        // Threshold reached — execute emergency release
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        if data.status == EscrowStatus::Settled
            || data.status == EscrowStatus::Refunded
            || data.status == EscrowStatus::Cancelled
        {
            return Err(Error::AlreadySettled);
        }

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        let remaining = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        // Pay remaining to seller
        if remaining > 0 {
            token.transfer(&contract, &data.seller, &remaining);
        }

        data.status = EscrowStatus::Settled;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        events::escrow_status_changed(
            &env,
            invoice_id.clone(),
            EscrowStatus::Settled,
            env.ledger().timestamp(),
        );
        Ok(true)
    }

    /// Reclaim persistent storage for an escrow that has reached a terminal state
    /// (Settled, Refunded, or Cancelled). Callable only by the seller or the admin.
    /// The escrow and its per-funder contribution record are removed permanently;
    /// terminal-state escrows are never mutated again, so this is safe to prune.
    pub fn cleanup_escrow(env: Env, invoice_id: Symbol, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let data = storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if caller != data.seller && caller != config.admin {
            return Err(Error::Unauthorized);
        }
        match data.status {
            EscrowStatus::Settled | EscrowStatus::Refunded | EscrowStatus::Cancelled => {}
            _ => return Err(Error::EscrowNotSettled),
        }
        storage::remove_escrow_state(&env, invoice_id.clone(), &data.funders);
        events::escrow_cleaned_up(&env, invoice_id);
        Ok(())
    }

    // ── Position management: top_up / partial_refund / transfer_position / finalise_funding ──

    /// Create a funding invoice (BytesN<32> id) for the new position management flows.
    /// This is the setup entrypoint for tests and admin tooling for the
    /// top_up / partial_refund / transfer_position / finalise_funding lifecycle.
    pub fn create_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        seller: Address,
        funding_target: i128,
        deadline_ledger: u32,
        min_investment: i128,
        per_investor_cap: Option<i128>,
        token: Address,
    ) -> Result<(), Error> {
        seller.require_auth();
        if funding_target <= 0 {
            return Err(Error::InvalidAmount);
        }
        if min_investment < 0 {
            return Err(Error::InvalidAmount);
        }
        if storage::has_invoice(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        let invoice = FundingInvoice {
            seller: seller.clone(),
            funding_target,
            total_raised: 0,
            deadline_ledger,
            min_investment,
            per_investor_cap,
            status: InvoiceStatus::Open,
            token: token.clone(),
        };
        storage::set_invoice(&env, invoice_id, &invoice);
        Ok(())
    }

    /// Top up an existing investor position.
    /// Validates invoice is Open, caller has non-zero position, and cap not exceeded.
    pub fn top_up(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        additional_amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if additional_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Open {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current = storage::get_investor_position(&env, &invoice_id, &investor);
        if current == 0 {
            return Err(Error::NoPositionFound);
        }
        let new_total_position = current
            .checked_add(additional_amount)
            .ok_or(Error::Overflow)?;
        if let Some(cap) = invoice.per_investor_cap {
            if new_total_position > cap {
                return Err(Error::InvalidAmount);
            }
        }
        let new_total_raised = invoice
            .total_raised
            .checked_add(additional_amount)
            .ok_or(Error::Overflow)?;
        if new_total_raised > invoice.funding_target {
            return Err(Error::InvalidAmount);
        }
        // Transfer additional_amount from investor to contract
        let token_client = token::Client::new(&env, &invoice.token);
        token_client.transfer(
            &investor,
            &env.current_contract_address(),
            &additional_amount,
        );
        storage::set_investor_position(&env, &invoice_id, &investor, new_total_position);
        invoice.total_raised = new_total_raised;
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
        events::investment_topped_up(
            &env,
            &investor,
            invoice_id,
            additional_amount,
            new_total_position,
        );
        Ok(())
    }

    // ---------- Invoice Registration, Investment, Refund, Settlement & TTL Refresh ----------

    /// Register invoice metadata and funding parameters on-chain. Callable only by admin.
    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        face_value: i128,
        funding_target: i128,
        yield_bps: u32,
        deadline_ledger: u32,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        if face_value <= 0 || funding_target <= 0 {
            return Err(Error::InvalidAmount);
        }
        if !(1..=5000).contains(&yield_bps) {
            return Err(Error::InvalidYield);
        }
        if storage::has_invoice_record(&env, &invoice_id) {
            return Err(Error::InvoiceAlreadyExists);
        }

        let data = InvoiceData {
            invoice_id: invoice_id.clone(),
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
            total_raised: 0,
            status: EscrowStatus::Created,
            investors: soroban_sdk::Vec::new(&env),
        };

        storage::set_invoice_record(&env, &invoice_id, &data);
        events::invoice_registered(
            &env,
            &invoice_id,
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
        );
        Ok(())
    }

    /// Invest in a registered invoice or funding invoice.
    pub fn invest(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if let Some(mut invoice) = storage::get_invoice(&env, invoice_id.clone()) {
            if invoice.status != InvoiceStatus::Open {
                return Err(Error::InvalidInvoiceStatus);
            }
            if amount < invoice.min_investment {
                return Err(Error::BelowMinimumInvestment);
            }
            if let Some(cap) = invoice.per_investor_cap {
                if amount > cap {
                    return Err(Error::InvalidAmount);
                }
            }
            let new_total = invoice
                .total_raised
                .checked_add(amount)
                .ok_or(Error::Overflow)?;
            if new_total > invoice.funding_target {
                return Err(Error::InvalidAmount);
            }
            let current = storage::get_investor_position(&env, &invoice_id, &investor);
            let new_pos = current.checked_add(amount).ok_or(Error::Overflow)?;
            if let Some(cap) = invoice.per_investor_cap {
                if new_pos > cap {
                    return Err(Error::InvalidAmount);
                }
            }
            let token_client = token::Client::new(&env, &invoice.token);
            token_client.transfer(&investor, &env.current_contract_address(), &amount);
            storage::set_investor_position(&env, &invoice_id, &investor, new_pos);
            invoice.total_raised = new_total;
            storage::set_invoice(&env, invoice_id, &invoice);
            return Ok(());
        }

        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current_ledger = env.ledger().sequence();
        if current_ledger > record.deadline_ledger {
            return Err(Error::FundingDeadlineNotPassed);
        }

        let new_raised = record
            .total_raised
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        if new_raised > record.funding_target {
            return Err(Error::InvalidAmount);
        }

        let current_pos = storage::get_investor_position(&env, &invoice_id, &investor);
        let new_pos = current_pos.checked_add(amount).ok_or(Error::Overflow)?;
        storage::set_investor_position(&env, &invoice_id, &investor, new_pos);

        let mut already_in = false;
        for inv in record.investors.iter() {
            if inv == investor {
                already_in = true;
                break;
            }
        }
        if !already_in {
            record.investors.push_back(investor.clone());
        }

        record.total_raised = new_raised;
        if record.total_raised == record.funding_target {
            record.status = EscrowStatus::Funded;
        }

        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// Partially refund an investor's position before deadline.
    pub fn partial_refund(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Open {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current_ledger = env.ledger().sequence();
        if current_ledger >= invoice.deadline_ledger {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current = storage::get_investor_position(&env, &invoice_id, &investor);
        if current == 0 {
            return Err(Error::NoPositionFound);
        }
        if amount > current {
            return Err(Error::InvalidAmount);
        }
        let remaining = current.checked_sub(amount).ok_or(Error::Overflow)?;
        if remaining != 0 && remaining < invoice.min_investment {
            return Err(Error::BelowMinimumInvestment);
        }
        let new_total_raised = invoice
            .total_raised
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        // Transfer back to caller
        let token_client = token::Client::new(&env, &invoice.token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);
        storage::set_investor_position(&env, &invoice_id, &investor, remaining);
        invoice.total_raised = new_total_raised;
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
        events::investment_partially_refunded(&env, &investor, invoice_id, amount, remaining);
        Ok(())
    }

    /// Transfer a funded position from seller to buyer for an agreed price.
    pub fn transfer_position(
        env: Env,
        from: Address,
        invoice_id: BytesN<32>,
        to: Address,
        price: i128,
    ) -> Result<(), Error> {
        from.require_auth();
        if price < 0 {
            return Err(Error::InvalidAmount);
        }
        let invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        let position = storage::get_investor_position(&env, &invoice_id, &from);
        if position == 0 {
            return Err(Error::NoPositionFound);
        }
        to.require_auth();
        if price > 0 {
            let token_client = token::Client::new(&env, &invoice.token);
            token_client.transfer(&to, &from, &price);
        }
        let buyer_existing = storage::get_investor_position(&env, &invoice_id, &to);
        let new_buyer_pos = buyer_existing
            .checked_add(position)
            .ok_or(Error::Overflow)?;
        if let Some(cap) = invoice.per_investor_cap {
            if new_buyer_pos > cap {
                return Err(Error::InvalidAmount);
            }
        }
        storage::set_investor_position(&env, &invoice_id, &from, 0);
        storage::set_investor_position(&env, &invoice_id, &to, new_buyer_pos);
        events::position_transferred(&env, &from, &to, invoice_id, position, price);
        Ok(())
    }

    /// Finalise funding: transition Open->Funded when target reached, release proceeds to seller.
    pub fn finalise_funding(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        if let Some(mut invoice) = storage::get_invoice(&env, invoice_id.clone()) {
            let config = storage::get_config(&env).ok_or(Error::NotInit)?;
            config.admin.require_auth();
            if invoice.status != InvoiceStatus::Open {
                return Err(Error::InvalidInvoiceStatus);
            }
            if invoice.total_raised < invoice.funding_target {
                return Err(Error::FundingTargetNotReached);
            }
            invoice.status = InvoiceStatus::Funded;
            storage::set_invoice(&env, invoice_id.clone(), &invoice);
            if invoice.total_raised > 0 {
                let token_client = token::Client::new(&env, &invoice.token);
                token_client.transfer(
                    &env.current_contract_address(),
                    &invoice.seller,
                    &invoice.total_raised,
                );
            }
            events::funding_finalised(&env, invoice_id, invoice.total_raised, &invoice.seller);
            return Ok(());
        }

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created && record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        record.status = EscrowStatus::Funded;
        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// Refund an investor's committed position if deadline passed without reaching target.
    pub fn refund(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<(), Error> {
        investor.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;

        if record.status == EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger <= record.deadline_ledger {
            return Err(Error::FundingDeadlineNotPassed);
        }

        let committed = storage::get_investor_position(&env, &invoice_id, &investor);
        if committed <= 0 {
            return Err(Error::NoPositionFound);
        }

        storage::remove_investor_position(&env, &invoice_id, &investor);

        record.total_raised = record
            .total_raised
            .checked_sub(committed)
            .ok_or(Error::Overflow)?;

        storage::set_invoice_record(&env, &invoice_id, &record);
        events::investment_refunded(&env, &investor, &invoice_id, committed);
        Ok(())
    }

    /// Settle invoice pro-rata across investors when seller repays. Callable only by admin.
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;

        if record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }

        if repayment_amount < record.total_raised {
            return Err(Error::InsufficientRepayment);
        }

        let mut total_payouts: i128 = 0;
        for investor in record.investors.iter() {
            let committed = storage::get_investor_position(&env, &invoice_id, &investor);
            if committed > 0 {
                let payout = committed
                    .checked_mul(repayment_amount)
                    .ok_or(Error::Overflow)?
                    .checked_div(record.total_raised)
                    .ok_or(Error::Overflow)?;
                let yield_earned = payout.saturating_sub(committed);
                total_payouts = total_payouts.checked_add(payout).ok_or(Error::Overflow)?;
                events::settlement_paid(&env, &investor, &invoice_id, payout, yield_earned);
            }
        }

        let _dust = repayment_amount
            .checked_sub(total_payouts)
            .ok_or(Error::Overflow)?;

        record.status = EscrowStatus::Settled;
        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// Admin-only: refresh TTL for invoice record and all investor position entries.
    pub fn refresh_all_ttls(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        for investor in record.investors.iter() {
            let _ = storage::get_investor_position(&env, &invoice_id, &investor);
        }
        Ok(())
    }

    /// View: get funding invoice (BytesN<32>).
    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<FundingInvoice, Error> {
        storage::get_invoice(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return registered invoice data.
    pub fn get_invoice_record(env: Env, invoice_id: BytesN<32>) -> Result<InvoiceData, Error> {
        storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return investor position amount for an invoice.
    pub fn get_investor_position(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<i128, Error> {
        Ok(storage::get_investor_position(&env, &invoice_id, &investor))
    }

    /// Paginated query to retrieve multiple escrows by sequential creation order.
    pub fn get_escrows(env: Env, start: u32, limit: u32) -> Result<soroban_sdk::Vec<EscrowData>, Error> {
        const MAX_PAGE_SIZE: u32 = 100;
        
        if limit == 0 {
            return Err(Error::InvalidLimit);
        }
        if limit > MAX_PAGE_SIZE {
            return Err(Error::LimitExceeded);
        }
        
        let total_count = storage::get_escrow_count(&env);
        
        if start >= total_count {
            return Ok(soroban_sdk::Vec::new(&env));
        }
        
        let end = core::cmp::min(start + limit, total_count);
        let mut results = soroban_sdk::Vec::new(&env);
        
        for index in start..end {
            if let Some(invoice_id) = storage::get_escrow_id_by_index(&env, index) {
                if let Some(escrow_data) = storage::get_escrow(&env, invoice_id) {
                    results.push_back(escrow_data);
                }
            }
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
