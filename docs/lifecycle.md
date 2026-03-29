# Invoice Escrow Lifecycle

This document describes the end-to-end invoice lifecycle across `invoice-escrow`, `invoice-token`, and the configured `payment-distributor`.

## 1. Creation (`create_escrow`)
- **Actor:** Seller
- **Action:** Registers a new invoice in `invoice-escrow`.
- **State changes:** Escrow status becomes `Created`.
- **Tracked fields:** `payment_token`, `invoice_token`, seller, amount, due date.
- **Tokenization:** No invoice tokens are minted yet.

## 2. Funding (`fund_escrow`)
- **Actor:** Buyer / Investor
- **Action:** Transfers the invoice amount in `payment_token` into the escrow contract.
- **State changes:** Escrow status becomes `Funded`, and `funder` is recorded.
- **Tokenization:** Escrow uses its minter authorization to call `mint` on `invoice-token`, giving the investor invoice ownership tokens.

## 3. Settlement (`record_payment`)
- **Actor:** Payer
- **Action:** Sends a payment amount into the escrow contract.
- **Escrow accounting:**
  The payer payment enters escrow.
  The matching amount of the investor’s locked collateral is released from escrow.
- **Distribution path when a distributor is configured:**
  Escrow transfers the settlement funds into `payment-distributor`.
  Escrow invokes `payment-distributor.distribute_payment(...)`.
  Distributor pays:
  `seller_amount = payment amount`
  `investor_amount = payment amount - platform fee`
  `platform_fee = payment amount * fee_bps / 10_000`
- **Distribution path without a distributor:**
  Escrow falls back to direct transfers to seller, investor, and admin.
- **State changes:** Escrow remains `Funded` while partial payments are outstanding and becomes `Settled` only when cumulative paid amount reaches the invoice amount.
- **Tokenization:** On full settlement, escrow unlocks `invoice-token` transfers by calling `set_transfer_locked(false)`.

## 4. Refund (`refund`)
- **Actor:** Anyone, typically investor or admin
- **Precondition:** Escrow is still `Funded` and the due date has passed.
- **Action:** Calculates the remaining unreleased collateral: `amount - paid_amt`.
- **Distribution path when a distributor is configured:**
  Escrow transfers the refund amount into `payment-distributor`.
  Escrow invokes `payment-distributor.distribute_refund(...)`.
  Distributor returns the remaining collateral to the investor.
- **Distribution path without a distributor:**
  Escrow refunds the investor directly.
- **State changes:** Escrow becomes `Refunded`.
- **Tokenization:** Escrow unlocks `invoice-token` transfers after refund.

## Transfer Lock Policy

| Escrow State | `transfer_locked` | Who Can Transfer |
|---|---|---|
| `Created` | `true` | Admin only |
| `Funded` | `true` | Admin only |
| `Settled` | `false` | All holders |
| `Refunded` | `false` | All holders |

Only the token `admin` or configured `minter` may call `set_transfer_locked`, so transfer lock changes stay tied to the escrow lifecycle.

## Emergency Pause

Both lifecycle contracts now expose an admin-controlled emergency pause.

### Escrow pause
- `invoice-escrow.set_paused(bool)` halts lifecycle-changing operations.
- While paused, `create_escrow`, `cancel_escrow`, `fund_escrow`, `record_payment`, and `refund` are rejected.

### Token pause
- `invoice-token.set_paused(bool)` halts sensitive token actions.
- While paused, `mint`, `transfer`, `approve`, `transfer_from`, `burn`, and `burn_from` are rejected.

Both contracts emit `paused_updated` when their pause state changes.
