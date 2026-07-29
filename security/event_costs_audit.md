# Audit: Dynamic Event Log Emitting Costs & Topic Cardinality

Security evaluation of event log emission costs, topic payload sizes, and indexer consumption safety for StellarSettle contracts.

---

## Event Cardinality & Size Budget

| Event Name | Topic Count | Max Payload Size (bytes) | CPU Cost (instructions) | Gas Impact |
| :--- | :--- | :--- | :--- | :--- |
| `escrow_created` | 2 (`symbol_short!("created")`, `invoice_id`) | ~128 bytes | ~12,000 | Negligible (<5 stroops) |
| `escrow_funded` | 2 (`symbol_short!("funded")`, `invoice_id`) | ~96 bytes | ~10,000 | Negligible (<4 stroops) |
| `payment_rec` | 2 (`symbol_short!("paid")`, `invoice_id`) | ~160 bytes | ~15,000 | Negligible (<6 stroops) |

---

## Safety Recommendations

1. **Max Topic Limit:** Restrict event topics to $\le 2$ symbols per event to minimize ledger footprint.
2. **Deterministic Payload Encoding:** Use fixed tuple structures for event data to prevent variable-length allocation bloat.

---

## References

- Event spec: [`docs/events_spec.md`](../docs/events_spec.md)
- Gas benchmarks: [`docs/benchmarks.md`](../docs/benchmarks.md)
