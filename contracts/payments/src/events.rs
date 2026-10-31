//! Contract events. The API indexer depends on these layouts; see
//! `docs/EVENTS.md`. Every event is published with topics
//! `("pay", <name>)` and a data struct whose first field is the schema
//! version `v`. Adding fields requires bumping `v` (see the compatibility
//! policy in EVENTS.md).
//!
//! TODO(wave: core-2): versioned JSON schema docs, extra fields and decoder
//! conformance vectors build on these helpers.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol};

use crate::types::{Config, Payment, RefundReason, SettleMode};

/// Current event schema version.
pub const EVENT_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedEvent {
    pub v: u32,
    pub payment_id: BytesN<32>,
    pub operator: Address,
    pub merchant: Address,
    pub token: Address,
    pub amount: i128,
    pub reference: BytesN<32>,
    pub settle_mode: SettleMode,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaidEvent {
    pub v: u32,
    pub payment_id: BytesN<32>,
    pub payer: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub paid_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettledEvent {
    pub v: u32,
    pub payment_id: BytesN<32>,
    pub merchant: Address,
    pub fee_recipient: Address,
    pub merchant_amount: i128,
    pub fee: i128,
    pub settled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundedEvent {
    pub v: u32,
    pub payment_id: BytesN<32>,
    pub payer: Address,
    pub amount: i128,
    pub reason: RefundReason,
    pub refunded_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelledEvent {
    pub v: u32,
    pub payment_id: BytesN<32>,
    pub cancelled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeChangedEvent {
    pub v: u32,
    pub fee_bps: u32,
    pub fee_recipient: Address,
    pub config_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PausedEvent {
    pub v: u32,
    pub paused: bool,
    pub config_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChangedEvent {
    pub v: u32,
    pub previous_admin: Address,
    pub admin: Address,
    pub config_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializedEvent {
    pub v: u32,
    pub admin: Address,
    pub token: Address,
    pub fee_bps: u32,
    pub fee_recipient: Address,
    pub reclaim_delay: u64,
}

fn topics(name: Symbol) -> (Symbol, Symbol) {
    (symbol_short!("pay"), name)
}

pub fn initialized(env: &Env, config: &Config) {
    env.events().publish(
        topics(Symbol::new(env, "initialized")),
        InitializedEvent {
            v: EVENT_VERSION,
            admin: config.admin.clone(),
            token: config.token.clone(),
            fee_bps: config.fee_bps,
            fee_recipient: config.fee_recipient.clone(),
            reclaim_delay: config.reclaim_delay,
        },
    );
}

pub fn created(env: &Env, payment_id: &BytesN<32>, payment: &Payment) {
    env.events().publish(
        topics(symbol_short!("created")),
        CreatedEvent {
            v: EVENT_VERSION,
            payment_id: payment_id.clone(),
            operator: payment.operator.clone(),
            merchant: payment.merchant.clone(),
            token: payment.token.clone(),
            amount: payment.amount,
            reference: payment.reference.clone(),
            settle_mode: payment.settle_mode,
            created_at: payment.created_at,
        },
    );
}

pub fn paid(env: &Env, payment_id: &BytesN<32>, payer: &Address, payment: &Payment, at: u64) {
    env.events().publish(
        topics(symbol_short!("paid")),
        PaidEvent {
            v: EVENT_VERSION,
            payment_id: payment_id.clone(),
            payer: payer.clone(),
            amount: payment.amount,
            fee_bps: payment.fee_bps,
            paid_at: at,
        },
    );
}

pub fn settled(
    env: &Env,
    payment_id: &BytesN<32>,
    payment: &Payment,
    fee_recipient: &Address,
    split: (i128, i128),
    at: u64,
) {
    let (merchant_amount, fee) = split;
    env.events().publish(
        topics(symbol_short!("settled")),
        SettledEvent {
            v: EVENT_VERSION,
            payment_id: payment_id.clone(),
            merchant: payment.merchant.clone(),
            fee_recipient: fee_recipient.clone(),
            merchant_amount,
            fee,
            settled_at: at,
        },
    );
}

pub fn refunded(
    env: &Env,
    payment_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    reason: RefundReason,
    at: u64,
) {
    env.events().publish(
        topics(symbol_short!("refunded")),
        RefundedEvent {
            v: EVENT_VERSION,
            payment_id: payment_id.clone(),
            payer: payer.clone(),
            amount,
            reason,
            refunded_at: at,
        },
    );
}

pub fn cancelled(env: &Env, payment_id: &BytesN<32>, at: u64) {
    env.events().publish(
        topics(symbol_short!("cancelled")),
        CancelledEvent {
            v: EVENT_VERSION,
            payment_id: payment_id.clone(),
            cancelled_at: at,
        },
    );
}

pub fn fee_changed(env: &Env, config: &Config) {
    env.events().publish(
        topics(Symbol::new(env, "fee_changed")),
        FeeChangedEvent {
            v: EVENT_VERSION,
            fee_bps: config.fee_bps,
            fee_recipient: config.fee_recipient.clone(),
            config_version: config.version,
        },
    );
}

pub fn paused(env: &Env, config: &Config) {
    env.events().publish(
        topics(symbol_short!("paused")),
        PausedEvent {
            v: EVENT_VERSION,
            paused: config.paused,
            config_version: config.version,
        },
    );
}

pub fn admin_changed(env: &Env, previous_admin: &Address, config: &Config) {
    env.events().publish(
        topics(Symbol::new(env, "admin_changed")),
        AdminChangedEvent {
            v: EVENT_VERSION,
            previous_admin: previous_admin.clone(),
            admin: config.admin.clone(),
            config_version: config.version,
        },
    );
}
