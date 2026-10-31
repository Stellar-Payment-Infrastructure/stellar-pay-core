use soroban_sdk::contracterror;

/// Contract error codes. The numeric values are part of the public interface:
/// the API maps `Error(Contract, #N)` to stable error codes, so never renumber.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `initialize` has not been called yet.
    NotInitialized = 1,
    /// `initialize` was already called.
    AlreadyInitialized = 2,
    /// No payment (or pending admin) exists for the given key.
    NotFound = 3,
    /// A payment with this id already exists.
    AlreadyExists = 4,
    /// The caller is not allowed to perform this action.
    Unauthorized = 5,
    /// Amount must be strictly positive.
    InvalidAmount = 6,
    /// The payment is not in a state that allows this transition.
    InvalidState = 7,
    /// The contract is paused (blocks `create_payment` and `pay` only).
    Paused = 8,
    /// The token is not accepted by this deployment.
    TokenNotAllowed = 9,
    /// `fee_bps` exceeds `MAX_FEE_BPS`.
    FeeTooHigh = 10,
    /// `reclaim` called before `paid_at + reclaim_delay`.
    TooEarly = 11,
    /// Checked arithmetic overflowed.
    Overflow = 12,
    /// An address argument is not allowed here (e.g. the contract itself).
    InvalidAddress = 13,
}
