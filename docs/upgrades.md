# Contract Upgrade & Storage Migration Protocol

This specification outlines the upgrade safety mechanics, WASM code hash updates, and storage migration protocol for StellarSettle smart contracts.

## 🔄 Upgrade Architecture

StellarSettle contracts leverage Soroban's native `env.deployer().update_current_contract_wasm(new_wasm_hash)` entry point for seamless code upgrades while preserving contract address continuity and persistent storage state.

### Upgrade Requirements

1. **Admin Authorization**: Only the authorized admin address (`config.admin`) may initiate a contract upgrade.
2. **Pause Verification**: Contract MUST be placed in emergency paused state (`set_paused(true)`) prior to deploying new WASM code to prevent race conditions during migration.
3. **Storage TTL Maintenance**: Instance and persistent storage keys MUST be extended before and immediately after WASM replacement.

## 🛡️ Migration Checklist

- [ ] Compile new WASM artifact using release profile (`soroban contract build`).
- [ ] Install new WASM code hash on Stellar network (`soroban contract install --wasm <path>`).
- [ ] Call `set_paused(true)` on current contract instance.
- [ ] Invoke contract upgrade function with target `new_wasm_hash`.
- [ ] Run state verification smoke tests.
- [ ] Call `set_paused(false)` to resume normal operation.
