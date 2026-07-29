# SEP-41 Token Standard Compliance Mapping

This specification maps the `invoice-token` smart contract methods and events to the official Stellar SEP-41 Fungible Token Standard.

---

## Method Compliance Mapping

| SEP-41 Standard Method | `invoice-token` Implementation | Status | Notes |
| :--- | :--- | :--- | :--- |
| `allowance(from, spender)` | `allowance(from: Address, spender: Address) -> i128` | ✅ Compliant | Returns current approved transfer allowance |
| `approve(from, spender, amount, expiration_ledger)` | `approve(...) -> ()` | ✅ Compliant | Requires `from` authorization; enforces expiration ledger |
| `balance(id)` | `balance(id: Address) -> i128` | ✅ Compliant | Returns token balance for account |
| `transfer(from, to, amount)` | `transfer(from: Address, to: Address, amount: i128) -> ()` | ✅ Compliant | Enforces transfer lock during active escrow |
| `transfer_from(spender, from, to, amount)` | `transfer_from(...) -> ()` | ✅ Compliant | Deducts allowance and transfers tokens |
| `burn(from, amount)` | `burn(from: Address, amount: i128) -> ()` | ✅ Compliant | Decreases total supply and balance |
| `burn_from(spender, from, amount)` | `burn_from(...) -> ()` | ✅ Compliant | Burns tokens using approved allowance |
| `decimals()` | `decimals() -> u32` | ✅ Compliant | Fixed at 7 decimals (standard Stellar precision) |
| `name()` | `name() -> String` | ✅ Compliant | Set during token initialization |
| `symbol()` | `symbol() -> String` | ✅ Compliant | Unique ticker symbol |

---

## Token Extension Entrypoints

In addition to standard SEP-41, `invoice-token` exports custom extension entrypoints for escrow state synchronization:

```rust
// Lock/unlock transfers during active invoice financing window
pub fn set_transfer_locked(env: Env, locked: bool);
pub fn is_transfer_locked(env: Env) -> bool;
```

---

## References

- Token implementation: [`contracts/invoice-token/src/lib.rs`](../contracts/invoice-token/src/lib.rs)
- State machine interaction: [`docs/state-machine.md`](state-machine.md)
