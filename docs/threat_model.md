# StellarSettle Security Audit Preparation & Threat Model

This document formalizes the threat model, attack surface enumeration, and audit preparation checklist for the StellarSettle Soroban smart contract suite.

---

## 1. Scope & Assets Under Review

| Contract | Entry Points | Critical Assets |
| :--- | :--- | :--- |
| `invoice-escrow` | `create_escrow`, `fund_escrow`, `record_payment`, `cancel_escrow`, `refund`, `set_paused`, `upgrade` | Escrowed payment tokens, investor collateral |
| `invoice-token` | `mint`, `burn`, `transfer`, `approve`, `set_transfer_locked` | SEP-41 token balances, allowances |
| `payment-distributor` | `distribute`, `set_fee_bps` | Fee calculations, payout fan-out amounts |

---

## 2. Threat Categories

### T1 — Unauthorized Admin Actions
- **Vector:** Attacker calls `upgrade`, `set_paused`, or `set_fee_bps` without admin authorization.
- **Mitigation:** All admin functions require `require_auth(&config.admin)`. Admin address stored in instance storage, only settable at initialization.
- **Verification:** Unit tests assert `Unauthorized` error on non-admin callers.

### T2 — Reentrancy via Cross-Contract Calls
- **Vector:** Malicious token contract re-enters escrow during `fund_escrow` or `record_payment`.
- **Mitigation:** Soroban's execution model is single-threaded per invocation; cross-contract calls complete atomically. State updates occur before external token transfers (checks-effects-interactions pattern).
- **Verification:** Integration tests with mock token contracts that attempt recursive calls.

### T3 — Integer Overflow in Fee Calculations
- **Vector:** Crafted `amount` or `fee_bps` causes overflow in `amount * fee_bps / 10_000`.
- **Mitigation:** All arithmetic uses Rust's checked operations (`checked_mul`, `checked_div`). Overflow returns `Error::Overflow` (code 13).
- **Verification:** Property-based tests with boundary i128 values.

### T4 — Storage Key Collision
- **Vector:** Two different invoice IDs hash to the same storage key.
- **Mitigation:** Storage keys use typed enums (`StorageKey::Escrow(Symbol)`) which Soroban serializes deterministically. Symbol uniqueness is enforced by `EscrowExists` error guard.
- **Verification:** Fuzzing tests with random Symbol generation.

### T5 — Unauthorized Token Transfer During Lock
- **Vector:** Investor transfers invoice tokens while escrow is active, diluting settlement payouts.
- **Mitigation:** `set_transfer_locked(true)` is called during escrow creation; `transfer` checks this lock and returns `Unauthorized` when active.
- **Verification:** Unit tests assert transfer failure while locked, success after settlement.

---

## 3. Pre-Audit Checklist

- [ ] All contracts compile with zero warnings (`cargo clippy -- -D warnings`).
- [ ] 100% of public entry points have unit test coverage.
- [ ] Integration tests cover all state transitions in the escrow lifecycle.
- [ ] Property-based tests cover arithmetic boundary conditions.
- [ ] `cargo audit` reports zero known vulnerabilities.
- [ ] `cargo deny check` passes for license and advisory compliance.
- [ ] Code documentation (rustdoc) covers all public types and functions.
- [ ] Emergency pause mechanism tested for both activation and deactivation.
- [ ] Upgrade mechanism tested with WASM hash replacement.

---

## 4. References

- Contract source: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- Error catalog: [`docs/error_catalog.md`](error_catalog.md)
- State machine spec: [`docs/state-machine.md`](state-machine.md)
- Upgrade protocol: [`docs/upgrades.md`](upgrades.md)
