# Security Audit: Immutable Storage Key Protections

Security analysis of immutable storage key protections, admin overwrite guards, and storage key collision prevention in `invoice-escrow`.

---

## 1. Storage Layout & Key Typing

StellarSettle contracts enforce key immutability via typed enums:

```rust
#[contracttype]
pub enum StorageKey {
    Admin,
    FeeBps,
    Escrow(Symbol),
    TokenBalance(Address),
}
```

---

## 2. Invariant Security Enforcement

- **Guard 1 (No Admin Overwrites):** `create_escrow` checks `env.storage().persistent().has(&StorageKey::Escrow(id))` and throws `Error::EscrowExists` (code 2) if key is already occupied.
- **Guard 2 (Admin Scope Boundary):** Admin functions (`set_paused`, `upgrade`) ONLY access `StorageKey::Admin` in instance storage; they CANNOT overwrite or delete persistent escrow storage records.

---

## References

- Pre-audit review: [`security/pre_audit_review.md`](pre_audit_review.md)
- Threat model: [`docs/threat_model.md`](../docs/threat_model.md)
