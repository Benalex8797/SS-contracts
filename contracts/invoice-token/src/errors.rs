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
    RoleNotGranted = 4,
    /// Amount is zero or negative.
    InvalidAmount = 5,
    /// Insufficient balance for transfer or burn.
    InsufficientBalance = 6,
    InsufficientBalanceForFee = 7,
    /// Insufficient allowance for transfer_from or burn_from.
    InsufficientAllowance = 8,
    /// Allowance has expired (expiration_ledger passed).
    AllowanceExpired = 9,
    /// Transfers are locked (pre-settlement restriction).
    TransferLocked = 10,
    /// Arithmetic overflow.
    Overflow = 11,
    /// Approval expiration_ledger must be >= current ledger unless setting amount to 0.
    InvalidExpiration = 12,
    /// Contract is paused and the requested operation is temporarily disabled.
    Paused = 13,
    /// Token decimals exceed the supported sub-asset precision.
    InvalidDecimals = 14,
    /// Token name or symbol metadata is empty (SEP-41 requires non-empty metadata).
    InvalidMetadata = 15,
    /// No allowance exists for (from, spender), so its expiration cannot be extended.
    AllowanceNotFound = 16,
    /// Batch length mismatch between addresses and amounts.
    BatchLengthMismatch = 17,
    /// Fee basis points exceed maximum.
    InvalidFeeBps = 18,
}
