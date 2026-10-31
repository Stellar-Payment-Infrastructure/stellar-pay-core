//! Stellar Pay Core: a Soroban payment escrow/settlement contract.
//!
//! State machine (one-way, terminal states have no exits):
//!
//! ```text
//! (none) --create_payment--> Created --cancel--> Cancelled
//!                               |
//!                              pay
//!                               v
//!                             Paid --refund / reclaim--> Refunded
//!                               |
//!                             settle (or inside `pay` when SettleMode::Auto)
//!                               v
//!                            Settled
//! ```
//!
//! Invariants (each has a test in `test.rs`):
//! 1. Funds only move to the payment's merchant, the fee recipient, or the payer.
//! 2. `settle` and `refund`/`reclaim` happen at most once per payment.
//! 3. The contract's token balance equals the sum of `amount` over `Paid` payments.
//! 4. `fee + merchant_amount == amount` (checked math, floor division on the fee).
//! 5. Pausing never traps funds: `settle`, `refund`, `reclaim` work while paused.
//! 6. The admin cannot move escrowed funds (powers: capped fee, pause, admin transfer).
//! 7. Fee bps and fee recipient are snapshotted at `pay`.
//! 8. The operator cannot redirect funds to an arbitrary address.
//!
//! See `docs/SPEC.md` for the full specification and threat model.

#![no_std]

mod error;
mod events;
mod fees;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env};

pub use error::Error;
pub use events::{
    AdminChangedEvent, CancelledEvent, CreatedEvent, FeeChangedEvent, InitializedEvent, PaidEvent,
    PausedEvent, RefundedEvent, SettledEvent, EVENT_VERSION,
};
pub use fees::{split_amount, BPS_DENOMINATOR, MAX_FEE_BPS};
pub use types::{Condition, Config, DataKey, Payment, RefundReason, SettleMode, Status};

/// Storage schema version written into new `Config` and `Payment` records.
pub const STORAGE_VERSION: u32 = 1;

/// Reads the config and keeps the contract instance alive.
fn load_config(env: &Env) -> Result<Config, Error> {
    let config = storage::read_config(env).ok_or(Error::NotInitialized)?;
    storage::extend_instance_ttl(env);
    Ok(config)
}

fn load_payment(env: &Env, payment_id: &BytesN<32>) -> Result<Payment, Error> {
    storage::read_payment(env, payment_id).ok_or(Error::NotFound)
}

/// Whether `token` may be used for payments.
///
/// TODO(wave: core-1): replace with an admin-managed allowlist (add/remove/list)
/// with per-token config. v1 accepts exactly the one configured token.
fn is_token_allowed(config: &Config, token: &Address) -> bool {
    *token == config.token
}

fn bump_config_version(config: &mut Config) -> Result<(), Error> {
    config.version = config.version.checked_add(1).ok_or(Error::Overflow)?;
    Ok(())
}

#[contract]
pub struct PaymentsContract;

#[contractimpl]
impl PaymentsContract {
    /// One-time setup. Requires `admin` auth. Fails with `AlreadyInitialized`
    /// if called twice, `FeeTooHigh` if `fee_bps > MAX_FEE_BPS`, and
    /// `InvalidAddress` if `token` or `fee_recipient` is this contract.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        fee_bps: u32,
        fee_recipient: Address,
        reclaim_delay: u64,
    ) -> Result<(), Error> {
        if storage::has_config(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::FeeTooHigh);
        }
        let this = env.current_contract_address();
        if token == this || fee_recipient == this {
            return Err(Error::InvalidAddress);
        }

        let config = Config {
            admin,
            token,
            fee_bps,
            fee_recipient,
            reclaim_delay,
            paused: false,
            version: STORAGE_VERSION,
        };
        storage::write_config(&env, &config);
        events::initialized(&env, &config);
        Ok(())
    }

    /// Returns the deployment configuration. Read-only.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::read_config(&env).ok_or(Error::NotInitialized)
    }

    /// Creates a payment in `Created`. Requires `operator` auth.
    ///
    /// `payment_id` is a caller-chosen unique 32-byte id (duplicate ids fail
    /// with `AlreadyExists`, which makes creation idempotent per id).
    /// `amount` is in token base units and must be > 0. `reference` is an
    /// opaque 32-byte hash (never raw order data). Not allowed while paused.
    pub fn create_payment(
        env: Env,
        operator: Address,
        merchant: Address,
        payment_id: BytesN<32>,
        amount: i128,
        reference: BytesN<32>,
        settle_mode: SettleMode,
    ) -> Result<(), Error> {
        operator.require_auth();
        let config = load_config(&env)?;
        if config.paused {
            return Err(Error::Paused);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if merchant == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        if storage::has_payment(&env, &payment_id) {
            return Err(Error::AlreadyExists);
        }
        // TODO(wave: core-3): accept `expires_at` here and store it on the payment.

        let payment = Payment {
            operator,
            merchant,
            payer: None,
            token: config.token.clone(),
            amount,
            fee_bps: config.fee_bps,
            fee_recipient: None,
            reference,
            settle_mode,
            condition: Condition::None,
            status: Status::Created,
            created_at: env.ledger().timestamp(),
            paid_at: None,
            closed_at: None,
            version: STORAGE_VERSION,
        };
        storage::write_payment(&env, &payment_id, &payment);
        events::created(&env, &payment_id, &payment);
        Ok(())
    }

    /// Pays a `Created` payment. Requires `payer` auth (which must cover the
    /// nested token transfer). Snapshots the current fee bps and fee
    /// recipient into the payment.
    ///
    /// `Manual`: moves `amount` from the payer into contract escrow (`Paid`).
    /// `Auto`: transfers `amount - fee` to the merchant and `fee` to the fee
    /// recipient directly from the payer (`Settled`); funds are never held.
    /// Not allowed while paused.
    pub fn pay(env: Env, payer: Address, payment_id: BytesN<32>) -> Result<(), Error> {
        payer.require_auth();
        let config = load_config(&env)?;
        if config.paused {
            return Err(Error::Paused);
        }
        let mut payment = load_payment(&env, &payment_id)?;
        if payment.status != Status::Created {
            return Err(Error::InvalidState);
        }
        // TODO(wave: core-3): reject with an `Expired` error after `expires_at`.
        if !is_token_allowed(&config, &payment.token) {
            return Err(Error::TokenNotAllowed);
        }
        let this = env.current_contract_address();
        if payer == this {
            return Err(Error::InvalidAddress);
        }

        let now = env.ledger().timestamp();
        payment.payer = Some(payer.clone());
        payment.fee_bps = config.fee_bps;
        payment.fee_recipient = Some(config.fee_recipient.clone());
        payment.paid_at = Some(now);

        let token_client = token::Client::new(&env, &payment.token);
        match payment.settle_mode {
            SettleMode::Manual => {
                // Effects before interactions.
                payment.status = Status::Paid;
                storage::write_payment(&env, &payment_id, &payment);
                token_client.transfer(&payer, &this, &payment.amount);
                events::paid(&env, &payment_id, &payer, &payment, now);
            }
            SettleMode::Auto => {
                let (merchant_amount, fee) = split_amount(payment.amount, payment.fee_bps)?;
                payment.status = Status::Settled;
                payment.closed_at = Some(now);
                storage::write_payment(&env, &payment_id, &payment);
                token_client.transfer(&payer, &payment.merchant, &merchant_amount);
                if fee > 0 {
                    token_client.transfer(&payer, &config.fee_recipient, &fee);
                }
                events::paid(&env, &payment_id, &payer, &payment, now);
                events::settled(
                    &env,
                    &payment_id,
                    &payment,
                    &config.fee_recipient,
                    (merchant_amount, fee),
                    now,
                );
            }
        }
        Ok(())
    }

    /// Settles a `Paid` payment: `amount - fee` to the merchant and `fee` to
    /// the fee recipient snapshotted at `pay`. Requires the payment's
    /// operator auth. Allowed while paused.
    pub fn settle(env: Env, payment_id: BytesN<32>) -> Result<(), Error> {
        load_config(&env)?;
        let mut payment = load_payment(&env, &payment_id)?;
        payment.operator.require_auth();
        if payment.status != Status::Paid {
            return Err(Error::InvalidState);
        }
        let fee_recipient = payment.fee_recipient.clone().ok_or(Error::InvalidState)?;
        let (merchant_amount, fee) = split_amount(payment.amount, payment.fee_bps)?;
        let now = env.ledger().timestamp();

        payment.status = Status::Settled;
        payment.closed_at = Some(now);
        storage::write_payment(&env, &payment_id, &payment);

        let this = env.current_contract_address();
        let token_client = token::Client::new(&env, &payment.token);
        token_client.transfer(&this, &payment.merchant, &merchant_amount);
        if fee > 0 {
            token_client.transfer(&this, &fee_recipient, &fee);
        }
        events::settled(
            &env,
            &payment_id,
            &payment,
            &fee_recipient,
            (merchant_amount, fee),
            now,
        );
        Ok(())
    }

    /// Cancels a `Created` payment. Requires the payment's operator auth.
    pub fn cancel(env: Env, payment_id: BytesN<32>) -> Result<(), Error> {
        load_config(&env)?;
        let mut payment = load_payment(&env, &payment_id)?;
        payment.operator.require_auth();
        if payment.status != Status::Created {
            return Err(Error::InvalidState);
        }
        let now = env.ledger().timestamp();
        payment.status = Status::Cancelled;
        payment.closed_at = Some(now);
        storage::write_payment(&env, &payment_id, &payment);
        events::cancelled(&env, &payment_id, now);
        Ok(())
    }

    /// Refunds a `Paid` payment: returns the full `amount` to the payer.
    /// Requires the payment's operator auth. Allowed while paused.
    pub fn refund(env: Env, payment_id: BytesN<32>) -> Result<(), Error> {
        load_config(&env)?;
        let mut payment = load_payment(&env, &payment_id)?;
        payment.operator.require_auth();
        if payment.status != Status::Paid {
            return Err(Error::InvalidState);
        }
        Self::refund_to_payer(&env, &payment_id, &mut payment, RefundReason::Operator)
    }

    /// Payer safety valve: refunds a `Paid` payment to its payer once
    /// `paid_at + reclaim_delay` has passed (ledger time). Requires the
    /// payer's auth. Allowed while paused.
    pub fn reclaim(env: Env, payment_id: BytesN<32>) -> Result<(), Error> {
        let config = load_config(&env)?;
        let mut payment = load_payment(&env, &payment_id)?;
        if payment.status != Status::Paid {
            return Err(Error::InvalidState);
        }
        let payer = payment.payer.clone().ok_or(Error::InvalidState)?;
        payer.require_auth();
        let paid_at = payment.paid_at.ok_or(Error::InvalidState)?;
        let unlock_at = paid_at
            .checked_add(config.reclaim_delay)
            .ok_or(Error::Overflow)?;
        if env.ledger().timestamp() < unlock_at {
            return Err(Error::TooEarly);
        }
        Self::refund_to_payer(&env, &payment_id, &mut payment, RefundReason::Reclaim)
    }

    /// Returns a payment. Read-only.
    pub fn get_payment(env: Env, payment_id: BytesN<32>) -> Result<Payment, Error> {
        load_payment(&env, &payment_id)
    }

    /// Updates the fee (capped at `MAX_FEE_BPS`) and fee recipient. Requires
    /// admin auth. Does not affect payments that are already paid.
    pub fn set_fee(env: Env, fee_bps: u32, fee_recipient: Address) -> Result<(), Error> {
        let mut config = load_config(&env)?;
        config.admin.require_auth();
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::FeeTooHigh);
        }
        if fee_recipient == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        config.fee_bps = fee_bps;
        config.fee_recipient = fee_recipient;
        bump_config_version(&mut config)?;
        storage::write_config(&env, &config);
        events::fee_changed(&env, &config);
        Ok(())
    }

    /// Pauses or unpauses the contract. Requires admin auth. Pausing blocks
    /// only `create_payment` and `pay`; funds can always leave escrow.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut config = load_config(&env)?;
        config.admin.require_auth();
        config.paused = paused;
        bump_config_version(&mut config)?;
        storage::write_config(&env, &config);
        events::paused(&env, &config);
        Ok(())
    }

    /// Step 1 of the admin transfer: records `new_admin` as pending.
    /// Requires current admin auth. A later call replaces the pending admin.
    pub fn propose_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let config = load_config(&env)?;
        config.admin.require_auth();
        storage::write_pending_admin(&env, &new_admin);
        Ok(())
    }

    /// Step 2 of the admin transfer: the pending admin accepts. Requires the
    /// pending admin's auth. Fails with `NotFound` if nothing is pending.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let mut config = load_config(&env)?;
        let pending = storage::read_pending_admin(&env).ok_or(Error::NotFound)?;
        pending.require_auth();
        let previous_admin = config.admin.clone();
        config.admin = pending;
        bump_config_version(&mut config)?;
        storage::write_config(&env, &config);
        storage::clear_pending_admin(&env);
        events::admin_changed(&env, &previous_admin, &config);
        Ok(())
    }

    /// Extends the TTL of a payment entry (and the contract instance).
    /// Permissionless: anyone may pay the rent.
    pub fn bump_ttl(env: Env, payment_id: BytesN<32>) -> Result<(), Error> {
        if !storage::has_payment(&env, &payment_id) {
            return Err(Error::NotFound);
        }
        storage::extend_payment_ttl(&env, &payment_id);
        storage::extend_instance_ttl(&env);
        Ok(())
    }
}

impl PaymentsContract {
    fn refund_to_payer(
        env: &Env,
        payment_id: &BytesN<32>,
        payment: &mut Payment,
        reason: RefundReason,
    ) -> Result<(), Error> {
        let payer = payment.payer.clone().ok_or(Error::InvalidState)?;
        let now = env.ledger().timestamp();
        payment.status = Status::Refunded;
        payment.closed_at = Some(now);
        storage::write_payment(env, payment_id, payment);

        let token_client = token::Client::new(env, &payment.token);
        token_client.transfer(&env.current_contract_address(), &payer, &payment.amount);
        events::refunded(env, payment_id, &payer, payment.amount, reason, now);
        Ok(())
    }
}
