//! Error types for the invoice token contract (SEP-41).

use soroban_sdk::contracterror;

/// Errors that can occur during token operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized (e.g. not admin or minter).
    Unauthorized = 3,
    /// Amount is zero or negative.
    InvalidAmount = 4,
    /// Insufficient balance for transfer or burn.
    InsufficientBalance = 5,
    /// Insufficient allowance for transfer_from or burn_from.
    InsufficientAllowance = 6,
    /// Allowance has expired (expiration_ledger passed).
    AllowanceExpired = 7,
    /// Transfers are locked (pre-settlement restriction).
    TransferLocked = 8,
    /// Arithmetic overflow.
    Overflow = 9,
    /// Approval expiration_ledger must be >= current ledger unless setting amount to 0.
    InvalidExpiration = 10,
    /// Contract is paused and the requested operation is temporarily disabled.
    Paused = 11,
}
