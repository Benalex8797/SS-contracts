# End-to-End Invoice Financing Lifecycle

This document visualizes and explains the full end-to-end lifecycle of an invoice asset on StellarSettle, from issuance to final settlement or refund.

---

## 🔄 End-to-End Sequence Diagram

```
Seller                Escrow Contract          Investor               Debtor
  │                          │                    │                      │
  │─── 1. create_escrow ────>│                    │                      │
  │    (Invoice Details)     │                    │                      │
  │                          │                    │                      │
  │                          │<── 2. fund_escrow ─│                      │
  │                          │    (Payment Tokens)│                      │
  │                          │                    │                      │
  │<── 3. Mint Tokens ───────┼───────────────────>│                      │
  │    (Invoice Shares)      │                    │                      │
  │                          │                    │                      │
  │                          │<───────────────────┼── 4. record_payment ─│
  │                          │                    │    (Invoice Payout)  │
  │                          │                    │                      │
  │<── 5. Payout Net ────────┼───────────────────>│                      │
  │    (Pro-rata split)      │                    │                      │
```

---

## Lifecycle Stages Breakdown

### Stage 1: Invoice Creation (`Created`)
- **Actor:** Seller / Business
- **Action:** Calls `create_escrow` with invoice metadata (face value, discount purchase price, due date, payment token).
- **State:** Escrow parameters validated and stored. Invoice tokens initialized.

### Stage 2: Investor Funding (`Funded`)
- **Actor:** Liquidity Provider / Investor
- **Action:** Calls `fund_escrow`. Transfers payment tokens into contract escrow vault.
- **State:** `invoice-token` contract mints pro-rata shares to investor. Escrow enters `Funded` state. Transfer locks activated.

### Stage 3: Settlement (`Settled`)
- **Actor:** Debtor (Invoice Payer)
- **Action:** Calls `record_payment` before or at due date.
- **State:** `payment-distributor` executes fee deduction and pro-rata payout fan-out to seller and investors. Escrow enters `Settled` state. Token locks released.

### Alternative Stage 4: Refund / Default (`Refunded`)
- **Actor:** Admin / Seller (if past due date without debtor payment)
- **Action:** Calls `refund`.
- **State:** Escrowed payment tokens returned to investors; invoice tokens burned. Escrow enters `Refunded` state.

---

## References

- State machine specification: [`docs/state-machine.md`](state-machine.md)
- Distributor math: [`docs/distributor_guide.md`](distributor_guide.md)
- Error codes: [`docs/error_catalog.md`](error_catalog.md)
