//! Storage helpers. Every write extends the entry's TTL.
//!
//! TODO(wave: core-4): resource-fee optimization (fewer ledger reads/writes,
//! smaller entries) happens here; baseline numbers are in `docs/COSTS.md`.

use soroban_sdk::{Address, BytesN, Env};

use crate::types::{Config, DataKey, Payment};

/// Ledgers per day at a ~5s close time (17,280 = 86,400 / 5).
const DAY_IN_LEDGERS: u32 = 17_280;

/// Instance storage (config, pending admin and the contract code) is extended
/// to ~60 days whenever fewer than ~30 days remain. Every public entry point
/// touches the instance, so an active contract never archives.
const INSTANCE_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS * 30;
const INSTANCE_TTL_EXTEND_TO: u32 = DAY_IN_LEDGERS * 60;

/// Payment entries are extended to ~30 days on every write (and by
/// `bump_ttl`) when fewer than ~7 days remain. Terminal payments may archive
/// afterwards: the API database is the long-term history, and an archived
/// entry can be restored if ever needed.
pub const PAYMENT_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS * 7;
pub const PAYMENT_TTL_EXTEND_TO: u32 = DAY_IN_LEDGERS * 30;

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

pub fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

pub fn read_config(env: &Env) -> Option<Config> {
    env.storage().instance().get(&DataKey::Config)
}

pub fn write_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
    extend_instance_ttl(env);
}

pub fn has_payment(env: &Env, payment_id: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Payment(payment_id.clone()))
}

pub fn read_payment(env: &Env, payment_id: &BytesN<32>) -> Option<Payment> {
    env.storage()
        .persistent()
        .get(&DataKey::Payment(payment_id.clone()))
}

pub fn write_payment(env: &Env, payment_id: &BytesN<32>, payment: &Payment) {
    let key = DataKey::Payment(payment_id.clone());
    env.storage().persistent().set(&key, payment);
    env.storage()
        .persistent()
        .extend_ttl(&key, PAYMENT_TTL_THRESHOLD, PAYMENT_TTL_EXTEND_TO);
}

pub fn extend_payment_ttl(env: &Env, payment_id: &BytesN<32>) {
    env.storage().persistent().extend_ttl(
        &DataKey::Payment(payment_id.clone()),
        PAYMENT_TTL_THRESHOLD,
        PAYMENT_TTL_EXTEND_TO,
    );
}

pub fn read_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn write_pending_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
    extend_instance_ttl(env);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}
