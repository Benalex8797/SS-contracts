# Escrow State Machine Invariants & Transition Matrix

This specification formalizes state transitions, invariants, authorization controls, and token lifecycle rules across the StellarSettle invoice escrow smart contracts.

## 🔄 State Transition Matrix

| Current State | Action / Method | Next State | Allowed Roles / Auth | Pre-Conditions | Post-Conditions & Invariants |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `None` | `create_escrow` | `Created` | Seller | `face_value > 0`, `purchase_price > 0`, `due_date > current_time` | Escrow struct initialized; status set to `Created` |
| `Created` | `cancel_escrow` | `Cancelled` | Seller | Unfunded (`funded_amt == 0`); seller authenticated | Escrow status set to `Cancelled`; no collateral movements |
| `Created` | `fund_escrow` | `Created` / `Funded` | Buyer / Investor | `amount > 0`, `funded_amt + amount <= purchase_price` | Payment tokens transferred to escrow; Invoice tokens minted to buyer; status set to `Funded` if fully subscribed |
| `Funded` | `record_payment` | `Funded` / `Settled` | Debtor | Authorized debtor; `amount <= remaining_face_value` | Debtor payment pulled into escrow; pro-rata investor payout and platform fee distributed; status set to `Settled` when `paid_amt == face_value` |
| `Funded` | `refund` | `Refunded` | Anyone | `ledger_timestamp >= due_date`; status is `Funded` | Unpaid balance refunded pro-rata to investors; status set to `Refunded`; invoice token transfer locks released |

## 🛡️ Financial Invariants

1. **Conservation of Balance**: At any point in time, the token balance held in contract storage MUST equal `funded_amt + paid_amt - (investor_payouts + platform_fees + refunds)`.
2. **Fee Calculation Precision**: Platform fees are calculated on the payment amount using basis points (`MAX_BPS = 10_000`): `fee = amount * fee_bps / 10_000`.
3. **Token Transfer Locking**: Invoice token transfers remain locked (`set_transfer_locked(true)`) while escrow is active, and are unlocked (`false`) only upon transitioning to `Settled` or `Refunded`.
