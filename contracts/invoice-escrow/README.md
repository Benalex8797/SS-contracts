# Invoice Escrow Smart Contract (`invoice-escrow`)

The `invoice-escrow` contract forms the core financial escrow engine of the StellarSettle protocol. It manages invoice tokenization, seller collateral locking, investor funding, payment collection, and settlement.

---

## Key Features

- **Lifecycle Management:** Full state machine (`Created` → `Funded` → `Settled` / `Refunded` / `Cancelled`).
- **Emergency Pause:** Admin-controlled circuit breaker (`set_paused`).
- **Upgradeable:** Safe WASM hash replacement protocol (`upgrade`).
- **Reentrancy Safe:** Strict checks-effects-interactions execution.

---

## Quick Usage Example (Soroban CLI)

```bash
# Query Escrow Details
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- get_escrow \
  --invoice_id "INV-2026-001"
```

---

## Contract API Summary

| Function | Access | Description |
| :--- | :--- | :--- |
| `initialize(admin)` | Admin (once) | Initializes admin address in instance storage |
| `create_escrow(...)` | Seller | Creates new escrow record with invoice metadata |
| `fund_escrow(invoice_id, investor, amount)` | Investor | Funds escrow, locks payment tokens, mints invoice tokens |
| `record_payment(invoice_id, amount)` | Debtor / Anyone | Receives debtor payment, triggers pro-rata distribution |
| `cancel_escrow(invoice_id)` | Seller | Cancels unfunded escrow |
| `refund(invoice_id)` | Admin / Seller | Refunds investors if debtor defaults after due date |
| `set_paused(paused)` | Admin | Toggles contract emergency pause state |

---

## References

- Contract source: [`src/lib.rs`](src/lib.rs)
- State machine spec: [`../../docs/state-machine.md`](../../docs/state-machine.md)
- Gas benchmarks: [`../../docs/benchmarks.md`](../../docs/benchmarks.md)
