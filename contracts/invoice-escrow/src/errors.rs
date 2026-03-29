//! Error types for the invoice escrow contract.
use soroban_sdk::contracterror;

/// Errors that can occur during contract execution.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized (e.g. not admin).
    Unauthorized = 3,
    /// Amount is zero or negative.
    InvalidAmount = 4,
    /// Platform fee basis points exceed 10000 (100%).
    InvalidFeeBps = 5,
    /// No escrow exists for the given invoice.
    EscrowNotFound = 6,
    /// Escrow already exists for this invoice (duplicate create).
    EscrowExists = 7,
    /// Escrow has already been funded.
    EscrowFunded = 8,
    /// Escrow has not been funded yet.
    EscrowNotFunded = 9,
    /// Payment has already been settled or escrow refunded.
    AlreadySettled = 10,
    /// Refund not allowed (e.g. not past due date or wrong status).
    RefundNotAllowed = 11,
    /// Token transfer failed (e.g. insufficient balance).
    TransferFailed = 12,
    /// Arithmetic overflow or invalid operation.
    Overflow = 13,
    /// Escrow has been cancelled by the seller.
    EscrowCancelled = 14,
    /// Contract is paused and the requested operation is temporarily disabled.
    Paused = 15,
    /// Payer is not the authorized debtor for this invoice.
    InvalidPayer = 15,
    /// Due date is invalid (e.g., in the past or zero).
    InvalidDueDate = 16,
}
