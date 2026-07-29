# Upgrade Migration Safety & Storage Compatibility Analysis

Security audit of contract upgrade safety, WASM byte-code migration, and instance/persistent storage layout preservation.

---

## Key Safety Invariants

1. **Storage Layout Immutability:** Upgraded contract code MUST NOT reorder or alter existing `StorageKey` enum discriminant values.
2. **Admin Authorization Guard:** Upgrades MUST be restricted strictly to `env.storage().instance().get(&Admin)` and invoke `admin.require_auth()`.
3. **Rollback Safety:** Previous WASM hashes are logged via contract events to enable admin rollback if a critical defect is identified.

---

## Migration Verification Checklist

- [x] Tested upgrade execution with a modified WASM hash on Testnet.
- [x] Verified that pre-existing escrows remain readable and settleable post-upgrade.
- [x] Asserted that non-admin upgrade calls throw `Unauthorized` (code 4).

---

## References

- Upgrade protocol: [`docs/upgrades.md`](../docs/upgrades.md)
- Threat model: [`docs/threat_model.md`](../docs/threat_model.md)
