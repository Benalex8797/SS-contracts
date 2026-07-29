# Payment Distributor Split Math & Fee Schedules

This document formalizes the pro-rata payout mathematics, platform fee deduction mechanics, and rounding protection rules implemented in the `payment-distributor` contract.

---

## 1. Distribution Formulae

When an escrow payment is received, the `distribute` entry point calculates payouts as follows:

### Platform Fee Deduction
$$\text{Fee} = \lfloor \frac{\text{Payment Amount} \times \text{fee\_bps}}{10,000} \rfloor$$

- **`fee_bps`**: Fee in basis points ($1\text{ bps} = 0.01\%$, $100\text{ bps} = 1\%$). Max fee cap is $1,000\text{ bps}$ ($10\%$).
- **Precision:** Truncated integer division using checked arithmetic (`checked_mul`, `checked_div`).

### Net Settlement Amount
$$\text{Net Amount} = \text{Payment Amount} - \text{Fee}$$

### Pro-Rata Investor Payouts
For each investor $i$ holding $S_i$ shares out of total supply $S_{\text{total}}$:

$$\text{Payout}_i = \lfloor \frac{\text{Net Amount} \times S_i}{S_{\text{total}}} \rfloor$$

---

## 2. Dust Handling & Remainder Distribution

Due to integer division truncation, the sum of individual investor payouts may leave a small sub-unit remainder ("dust"):

$$\text{Remainder} = \text{Net Amount} - \sum_{i=1}^{N} \text{Payout}_i$$

- **Rule:** Any non-zero remainder is credited to the seller/originator address to ensure total payout equality:
  $$\sum \text{Payouts} + \text{Fee} + \text{Remainder} = \text{Payment Amount}$$

---

## 3. Example Calculations

| Parameter | Value |
| :--- | :--- |
| **Gross Payment** | 100,000.00 USDC (`100_000_000_000` stroops) |
| **Platform Fee BPS** | 50 BPS (0.50%) |
| **Investor A Shares** | 600 / 1,000 (60%) |
| **Investor B Shares** | 400 / 1,000 (40%) |

- **Fee Calculation:**
  $$\text{Fee} = \frac{100,000 \times 50}{10,000} = 500.00\text{ USDC}$$
- **Net Payout:**
  $$\text{Net} = 100,000 - 500 = 99,500.00\text{ USDC}$$
- **Investor A:** $99,500 \times 0.60 = 59,700.00\text{ USDC}$
- **Investor B:** $99,500 \times 0.40 = 39,800.00\text{ USDC}$

---

## 4. Source References

- Distributor entrypoint: [`contracts/payment-distributor/src/lib.rs`](../contracts/payment-distributor/src/lib.rs)
- Escrow state machine: [`docs/state-machine.md`](state-machine.md)
- Gas benchmarks: [`docs/benchmarks.md`](benchmarks.md)
