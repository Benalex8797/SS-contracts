# Security Audit: SEP-41 Allowance Expiration Boundary Checks

Evaluation of allowance expiration mechanics, ledger sequence boundary conditions, and authorization expiration in `invoice-token`.

---

## Expiration Boundary Rules

In SEP-41 token implementations on Soroban:

1. **`expiration_ledger` Validation:** An allowance is valid if and only if:
   $$\text{current\_ledger} \le \text{expiration\_ledger}$$
2. **Expired Allowance Behavior:** If $\text{current\_ledger} > \text{expiration\_ledger}$, `transfer_from` and `burn_from` MUST treat the allowance balance as `0` and return `Error::AllowanceExpired`.

---

## Verification Test Cases

- [x] Unit test: `approve` with `expiration_ledger = current + 100` succeeds for 100 ledgers.
- [x] Unit test: `transfer_from` on ledger `current + 101` fails with `AllowanceExpired`.
- [x] Unit test: Setting `amount = 0` clears allowance immediately.

---

## References

- SEP-41 mapping: [`docs/sep41_mapping.md`](../docs/sep41_mapping.md)
- Threat model: [`docs/threat_model.md`](../docs/threat_model.md)
