# Invoice Escrow Contract API Reference

Complete technical specification of all public entrypoints, error codes, and parameters for the `invoice-escrow` smart contract.

---

## Functions

### `initialize`
Initializes the contract instance with an admin address.

```rust
pub fn initialize(env: Env, admin: Address);
```
- **Auth:** Requires no auth (can only be invoked once).
- **Errors:** `AlreadyInitialized` (code 1) if invoked multiple times.

### `create_escrow`
Creates a new invoice escrow record.

```rust
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
);
```
- **Auth:** Requires `seller` authorization.
- **Errors:** `EscrowExists` (code 2), `InvalidAmount` (code 3).

### `fund_escrow`
Funds an active escrow with payment tokens and mints invoice tokens to investor.

```rust
pub fn fund_escrow(env: Env, invoice_id: Symbol, investor: Address, amount: i128);
```
- **Auth:** Requires `investor` authorization.
- **Errors:** `EscrowNotFound` (code 4), `EscrowAlreadyFunded` (code 5).

---

## References

- Contract implementation: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- State machine spec: [`docs/state-machine.md`](state-machine.md)
