# Blockchain Indexer Event Log Specification

This specification documents the exact event topics and payload schemas emitted by StellarSettle contracts (`invoice-escrow`, `invoice-token`, `payment-distributor`) for off-chain indexer consumption.

## Invoice Escrow Contract Events

### `escrow_created`
- **Topics**: `(Symbol("escrow_created"), invoice_id: Symbol)`
- **Data Payload**: `(seller: Address, debtor: Address, face_value: i128, purchase_price: i128, due_date: u64, payment_token: Address, invoice_token: Address, commitment: BytesN<32>)`
- **Description**: Emitted when a new escrow contract is initialized by a seller.

### `escrow_funded`
- **Topics**: `(Symbol("escrow_funded"), invoice_id: Symbol)`
- **Data Payload**: `(buyer: Address, funded_amount: i128, total_funded: i128, purchase_price: i128)`
- **Description**: Emitted when an investor deposits funds toward an invoice purchase.

### `payment_settled`
- **Topics**: `(Symbol("payment_settled"), invoice_id: Symbol)`
- **Data Payload**: `(payment_amount: i128, platform_fee: i128, investor_payout: i128)`
- **Description**: Emitted when a debtor executes payment settlement.

### `escrow_refunded`
- **Topics**: `(Symbol("escrow_refunded"), invoice_id: Symbol)`
- **Data Payload**: `(refunded_amount: i128)`
- **Description**: Emitted when an unpaid invoice triggers an investor collateral refund after due date.
