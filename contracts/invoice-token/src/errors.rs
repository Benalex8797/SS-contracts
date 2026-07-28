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
    /// Token decimals exceed the supported sub-asset precision.
    InvalidDecimals = 12,
    /// Token name or symbol metadata is empty (SEP-41 requires non-empty metadata).
    InvalidMetadata = 13,
    /// No allowance exists for (from, spender), so its expiration cannot be extended.
    AllowanceNotFound = 14,
    /// Balance is sufficient for the transfer amount but not for the fee.
    InsufficientBalanceForFee = 15,
    /// mint_batch vectors have mismatched lengths.
    BatchLengthMismatch = 16,
    /// Fee basis points value is out of allowed range (0..=10_000).
    InvalidFeeBps = 17,
    /// Role has not been granted to anyone, so there is no role admin.
    RoleNotGranted = 18,
}
