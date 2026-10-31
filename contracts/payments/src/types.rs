use soroban_sdk::{contracttype, Address, BytesN};

/// Storage keys.
///
/// `Config` and `PendingAdmin` live in instance storage; `Payment` entries live
/// in persistent storage (never temporary: they hold money state).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Config,
    Payment(BytesN<32>),
    PendingAdmin,
}

/// Payment lifecycle state. Transitions are one-way:
/// `Created -> Paid -> Settled | Refunded`, `Created -> Cancelled`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Status {
    Created = 0,
    Paid = 1,
    Settled = 2,
    Refunded = 3,
    Cancelled = 4,
}

/// `Auto` settles inside `pay` (funds are never escrowed); `Manual` escrows
/// funds until the operator calls `settle` or `refund`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SettleMode {
    Auto = 0,
    Manual = 1,
}

/// Extension point for conditional payments (time-lock, hash-lock).
/// Only `None` exists in v1 so new variants can be added without breaking
/// the storage layout.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Condition {
    None = 0,
}

/// Why a payment moved to `Refunded`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RefundReason {
    /// The operator called `refund`.
    Operator = 0,
    /// The payer called `reclaim` after `reclaim_delay`.
    Reclaim = 1,
}

/// Deployment-wide configuration (instance storage).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    /// The single token accepted by this deployment (SEP-41 / SAC address).
    pub token: Address,
    pub fee_bps: u32,
    pub fee_recipient: Address,
    /// Seconds after `paid_at` before the payer may `reclaim`.
    pub reclaim_delay: u64,
    pub paused: bool,
    /// Incremented on every config change.
    pub version: u32,
}

/// A payment record (persistent storage). Contains no PII: only addresses,
/// amounts, a 32-byte opaque reference hash, status and ledger timestamps.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    /// Authorizes lifecycle actions (create, settle, cancel, refund).
    pub operator: Address,
    /// Payee: receives `amount - fee` on settle.
    pub merchant: Address,
    /// Set by `pay`.
    pub payer: Option<Address>,
    pub token: Address,
    pub amount: i128,
    /// Copied from config at create; re-snapshotted at `pay` and fixed after.
    pub fee_bps: u32,
    /// Snapshotted at `pay` so later config changes don't affect in-flight payments.
    pub fee_recipient: Option<Address>,
    /// Opaque merchant reference hash (never raw order data).
    pub reference: BytesN<32>,
    pub settle_mode: SettleMode,
    pub condition: Condition,
    pub status: Status,
    pub created_at: u64,
    pub paid_at: Option<u64>,
    pub closed_at: Option<u64>,
    /// Storage schema version of this record.
    pub version: u32,
}
