# Soroban Storage TTL Extension Best Practices

This guide documents the storage strategy, TTL (Time-To-Live) extension policies, and rent mitigation patterns for StellarSettle smart contracts on Soroban.

---

## 1. Soroban Storage Categories

StellarSettle contracts utilize all three Soroban storage tiers:

| Storage Type | Use Case in StellarSettle | Default Lifetime | TTL Management |
| :--- | :--- | :--- | :--- |
| **Instance Storage** | Contract admin, fee BPS, global pause flag | Tied to contract instance | Extended on every admin invocation |
| **Persistent Storage** | Escrow records, token balances, allowances | Long-term (~6 months default) | Extended automatically during `fund_escrow` and `record_payment` |
| **Temporary Storage** | Nonce tracking, ephemeral signature verification | Short-term (~30 days) | Let expire naturally |

---

## 2. TTL Extension Constants

```rust
pub const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days in ledgers (5s/ledger)
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;

pub const PERSISTENT_BUMP_AMOUNT: u32 = 1_036_800; // ~60 days in ledgers
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 200_000;
```

---

## 3. Best Practices for Developers

1. **Auto-bump on Mutative Calls:** Every state-modifying call (`create_escrow`, `fund_escrow`, `record_payment`) MUST call `env.storage().persistent().extend_ttl(...)` on affected keys.
2. **Off-Chain Keeper Bumping:** Indexers and front-ends should monitor key TTLs using RPC and call `extend_ttl` if an active escrow approaches the threshold.
3. **Storage Cleanup:** Expired or settled escrows can have their persistent entries archived or cleared to reclaim storage rent.

---

## References

- Contract implementation: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- Gas costs: [`docs/benchmarks.md`](benchmarks.md)
