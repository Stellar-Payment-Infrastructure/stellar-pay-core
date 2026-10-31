extern crate std;

use std::format;
use std::string::String as StdString;
use std::vec;
use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{
    symbol_short,
    testutils::EnvTestConfig,
    testutils::{
        storage::Persistent as _, Address as _, AuthorizedFunction, AuthorizedInvocation,
        Events as _, Ledger, MockAuth, MockAuthInvoke,
    },
    token::{StellarAssetClient, TokenClient},
    xdr::{Limits, ScVal, WriteXdr},
    Address, BytesN, Env, IntoVal, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

use crate::{
    split_amount, CancelledEvent, CreatedEvent, DataKey, Error, FeeChangedEvent, PaidEvent,
    PausedEvent, PaymentsContract, PaymentsContractClient, RefundReason, RefundedEvent, SettleMode,
    SettledEvent, Status, MAX_FEE_BPS,
};

const FEE_BPS: u32 = 100;
const RECLAIM_DELAY: u64 = 3_600;
const START_TS: u64 = 1_700_000_000;

struct Setup {
    env: Env,
    client: PaymentsContractClient<'static>,
    contract: Address,
    admin: Address,
    fee_recipient: Address,
    operator: Address,
    merchant: Address,
    token: TokenClient<'static>,
    token_admin: StellarAssetClient<'static>,
}

impl Setup {
    fn new() -> Self {
        let env = test_env();
        env.mock_all_auths();
        Self::with_env(env)
    }

    fn with_env(env: Env) -> Self {
        env.ledger().set_timestamp(START_TS);
        let admin = Address::generate(&env);
        let fee_recipient = Address::generate(&env);
        let operator = Address::generate(&env);
        let merchant = Address::generate(&env);
        let issuer = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(issuer);
        let token = TokenClient::new(&env, &sac.address());
        let token_admin = StellarAssetClient::new(&env, &sac.address());
        let contract = env.register(PaymentsContract, ());
        let client = PaymentsContractClient::new(&env, &contract);
        Setup {
            env,
            client,
            contract,
            admin,
            fee_recipient,
            operator,
            merchant,
            token,
            token_admin,
        }
    }

    fn init(&self) {
        self.client.initialize(
            &self.admin,
            &self.token.address,
            &FEE_BPS,
            &self.fee_recipient,
            &RECLAIM_DELAY,
        );
    }

    fn id(&self, n: u8) -> BytesN<32> {
        BytesN::from_array(&self.env, &[n; 32])
    }

    fn reference(&self) -> BytesN<32> {
        BytesN::from_array(&self.env, &[0xAB; 32])
    }

    fn create(&self, n: u8, amount: i128, mode: SettleMode) -> BytesN<32> {
        let id = self.id(n);
        self.client.create_payment(
            &self.operator,
            &self.merchant,
            &id,
            &amount,
            &self.reference(),
            &mode,
        );
        id
    }

    fn funded_payer(&self, amount: i128) -> Address {
        let payer = Address::generate(&self.env);
        self.token_admin.mint(&payer, &amount);
        payer
    }

    /// Creates a manual payment and pays it; returns (id, payer).
    fn paid(&self, n: u8, amount: i128) -> (BytesN<32>, Address) {
        let id = self.create(n, amount, SettleMode::Manual);
        let payer = self.funded_payer(amount);
        self.client.pay(&payer, &id);
        (id, payer)
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token.balance(who)
    }

    fn advance(&self, secs: u64) {
        let ts = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(ts + secs);
    }

    /// Events published by the payments contract in the last invocation.
    fn contract_events(&self) -> StdVec<(Vec<Val>, Val)> {
        self.env
            .events()
            .all()
            .iter()
            .filter(|(addr, _, _)| *addr == self.contract)
            .map(|(_, topics, data)| (topics, data))
            .collect()
    }

    fn event_names(&self) -> StdVec<Symbol> {
        self.contract_events()
            .iter()
            .map(|(topics, _)| {
                let ns: Symbol = topics.get(0).unwrap().into_val(&self.env);
                assert_eq!(ns, symbol_short!("pay"));
                topics.get(1).unwrap().into_val(&self.env)
            })
            .collect()
    }

    fn mock_only(&self, who: &Address, fn_name: &'static str, args: Vec<Val>) {
        let invoke = MockAuthInvoke {
            contract: &self.contract,
            fn_name,
            args,
            sub_invokes: &[],
        };
        self.env.mock_auths(&[MockAuth {
            address: who,
            invoke: &invoke,
        }]);
    }
}

/// An Env that does not write test snapshot files on drop.
fn test_env() -> Env {
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

fn sym(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

fn is_auth_failure<T, E>(r: &Result<T, E>) -> bool {
    r.is_err()
}

// ---------------------------------------------------------------- initialize

#[test]
fn initialize_sets_config_and_emits_event() {
    let s = Setup::new();
    s.init();
    assert_eq!(s.event_names(), vec![sym(&s.env, "initialized")]);
    let config = s.client.get_config();
    assert_eq!(config.admin, s.admin);
    assert_eq!(config.token, s.token.address);
    assert_eq!(config.fee_bps, FEE_BPS);
    assert_eq!(config.fee_recipient, s.fee_recipient);
    assert_eq!(config.reclaim_delay, RECLAIM_DELAY);
    assert!(!config.paused);
    assert_eq!(config.version, 1);
}

#[test]
fn initialize_twice_fails() {
    let s = Setup::new();
    s.init();
    let r = s.client.try_initialize(
        &s.admin,
        &s.token.address,
        &FEE_BPS,
        &s.fee_recipient,
        &RECLAIM_DELAY,
    );
    assert_eq!(r, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn initialize_fee_too_high() {
    let s = Setup::new();
    let r = s.client.try_initialize(
        &s.admin,
        &s.token.address,
        &(MAX_FEE_BPS + 1),
        &s.fee_recipient,
        &RECLAIM_DELAY,
    );
    assert_eq!(r, Err(Ok(Error::FeeTooHigh)));
}

#[test]
fn initialize_rejects_contract_as_fee_recipient_or_token() {
    let s = Setup::new();
    let r = s.client.try_initialize(
        &s.admin,
        &s.token.address,
        &FEE_BPS,
        &s.contract,
        &RECLAIM_DELAY,
    );
    assert_eq!(r, Err(Ok(Error::InvalidAddress)));
    let r = s.client.try_initialize(
        &s.admin,
        &s.contract,
        &FEE_BPS,
        &s.fee_recipient,
        &RECLAIM_DELAY,
    );
    assert_eq!(r, Err(Ok(Error::InvalidAddress)));
}

#[test]
fn initialize_requires_admin_auth() {
    let s = Setup::with_env(test_env());
    let r = s.client.try_initialize(
        &s.admin,
        &s.token.address,
        &FEE_BPS,
        &s.fee_recipient,
        &RECLAIM_DELAY,
    );
    assert!(is_auth_failure(&r));
    assert_eq!(s.client.try_get_config(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn calls_before_initialize_fail() {
    let s = Setup::new();
    assert_eq!(s.client.try_get_config(), Err(Ok(Error::NotInitialized)));
    let r = s.client.try_create_payment(
        &s.operator,
        &s.merchant,
        &s.id(1),
        &100,
        &s.reference(),
        &SettleMode::Manual,
    );
    assert_eq!(r, Err(Ok(Error::NotInitialized)));
    assert_eq!(
        s.client.try_settle(&s.id(1)),
        Err(Ok(Error::NotInitialized))
    );
}

// ------------------------------------------------------------ create_payment

#[test]
fn create_payment_stores_created_payment() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 1_000, SettleMode::Manual);
    let events = s.contract_events();

    // Only the operator was asked to authorize, for exactly this call.
    assert_eq!(
        s.env.auths(),
        vec![(
            s.operator.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    s.contract.clone(),
                    Symbol::new(&s.env, "create_payment"),
                    (
                        s.operator.clone(),
                        s.merchant.clone(),
                        id.clone(),
                        1_000_i128,
                        s.reference(),
                        SettleMode::Manual,
                    )
                        .into_val(&s.env),
                )),
                sub_invocations: vec![],
            }
        )]
    );

    let p = s.client.get_payment(&id);
    assert_eq!(p.operator, s.operator);
    assert_eq!(p.merchant, s.merchant);
    assert_eq!(p.token, s.token.address);
    assert_eq!(p.amount, 1_000);
    assert_eq!(p.reference, s.reference());
    assert_eq!(p.settle_mode, SettleMode::Manual);
    assert_eq!(p.status, Status::Created);
    assert_eq!(p.fee_bps, FEE_BPS);
    assert_eq!(p.created_at, START_TS);
    assert!(p.payer.is_none() && p.fee_recipient.is_none());
    assert!(p.paid_at.is_none() && p.closed_at.is_none());

    assert_eq!(events.len(), 1);
    let ev: CreatedEvent = events[0].1.into_val(&s.env);
    assert_eq!(ev.v, 1);
    assert_eq!(ev.payment_id, id);
    assert_eq!(ev.amount, 1_000);
    assert_eq!(ev.merchant, s.merchant);
}

#[test]
fn create_payment_duplicate_id_fails() {
    let s = Setup::new();
    s.init();
    s.create(1, 1_000, SettleMode::Manual);
    let r = s.client.try_create_payment(
        &s.operator,
        &s.merchant,
        &s.id(1),
        &5,
        &s.reference(),
        &SettleMode::Auto,
    );
    assert_eq!(r, Err(Ok(Error::AlreadyExists)));
    assert_eq!(s.client.get_payment(&s.id(1)).amount, 1_000);
}

#[test]
fn create_payment_rejects_non_positive_amounts() {
    let s = Setup::new();
    s.init();
    for amount in [0_i128, -1, i128::MIN] {
        let r = s.client.try_create_payment(
            &s.operator,
            &s.merchant,
            &s.id(1),
            &amount,
            &s.reference(),
            &SettleMode::Manual,
        );
        assert_eq!(r, Err(Ok(Error::InvalidAmount)));
    }
}

#[test]
fn create_payment_rejects_contract_as_merchant() {
    let s = Setup::new();
    s.init();
    let r = s.client.try_create_payment(
        &s.operator,
        &s.contract,
        &s.id(1),
        &10,
        &s.reference(),
        &SettleMode::Manual,
    );
    assert_eq!(r, Err(Ok(Error::InvalidAddress)));
}

#[test]
fn create_payment_blocked_while_paused() {
    let s = Setup::new();
    s.init();
    s.client.set_paused(&true);
    let r = s.client.try_create_payment(
        &s.operator,
        &s.merchant,
        &s.id(1),
        &10,
        &s.reference(),
        &SettleMode::Manual,
    );
    assert_eq!(r, Err(Ok(Error::Paused)));
}

#[test]
fn create_payment_requires_operator_auth() {
    let s = Setup::new();
    s.init();
    let stranger = Address::generate(&s.env);
    let args: Vec<Val> = (
        s.operator.clone(),
        s.merchant.clone(),
        s.id(1),
        10_i128,
        s.reference(),
        SettleMode::Manual,
    )
        .into_val(&s.env);
    s.mock_only(&stranger, "create_payment", args);
    let r = s.client.try_create_payment(
        &s.operator,
        &s.merchant,
        &s.id(1),
        &10,
        &s.reference(),
        &SettleMode::Manual,
    );
    assert!(is_auth_failure(&r));
    s.env.mock_all_auths();
    assert_eq!(s.client.try_get_payment(&s.id(1)), Err(Ok(Error::NotFound)));
}

// ------------------------------------------------------------------------ pay

#[test]
fn pay_manual_escrows_funds_and_snapshots_fee() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 1_000, SettleMode::Manual);
    let payer = s.funded_payer(1_000);
    s.advance(10);
    s.client.pay(&payer, &id);
    let names = s.event_names();
    let events = s.contract_events();

    // The payer authorized `pay` and, nested inside it, the token transfer.
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, payer);
    assert_eq!(auths[0].1.sub_invocations.len(), 1);

    let p = s.client.get_payment(&id);
    assert_eq!(p.status, Status::Paid);
    assert_eq!(p.payer, Some(payer.clone()));
    assert_eq!(p.paid_at, Some(START_TS + 10));
    assert_eq!(p.fee_bps, FEE_BPS);
    assert_eq!(p.fee_recipient, Some(s.fee_recipient.clone()));
    assert_eq!(s.balance(&s.contract), 1_000);
    assert_eq!(s.balance(&payer), 0);

    assert_eq!(names, vec![symbol_short!("paid")]);
    let ev: PaidEvent = events[0].1.into_val(&s.env);
    assert_eq!(ev.payer, payer);
    assert_eq!(ev.amount, 1_000);
    assert_eq!(ev.paid_at, START_TS + 10);
}

#[test]
fn pay_unknown_payment_fails() {
    let s = Setup::new();
    s.init();
    let payer = s.funded_payer(10);
    assert_eq!(s.client.try_pay(&payer, &s.id(9)), Err(Ok(Error::NotFound)));
}

#[test]
fn pay_twice_fails_with_invalid_state() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 100);
    let other = s.funded_payer(100);
    assert_eq!(s.client.try_pay(&other, &id), Err(Ok(Error::InvalidState)));
    assert_eq!(s.balance(&other), 100);
    assert_eq!(s.balance(&s.contract), 100);
}

#[test]
fn pay_cancelled_payment_fails() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    s.client.cancel(&id);
    let payer = s.funded_payer(100);
    assert_eq!(s.client.try_pay(&payer, &id), Err(Ok(Error::InvalidState)));
}

#[test]
fn pay_with_insufficient_balance_fails_atomically() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 1_000, SettleMode::Manual);
    let payer = s.funded_payer(999);
    assert!(s.client.try_pay(&payer, &id).is_err());
    assert_eq!(s.client.get_payment(&id).status, Status::Created);
    assert_eq!(s.balance(&payer), 999);
    assert_eq!(s.balance(&s.contract), 0);
}

#[test]
fn pay_blocked_while_paused() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    s.client.set_paused(&true);
    let payer = s.funded_payer(100);
    assert_eq!(s.client.try_pay(&payer, &id), Err(Ok(Error::Paused)));
}

#[test]
fn pay_requires_payer_auth() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    let payer = s.funded_payer(100);
    let stranger = Address::generate(&s.env);
    s.mock_only(
        &stranger,
        "pay",
        (payer.clone(), id.clone()).into_val(&s.env),
    );
    assert!(is_auth_failure(&s.client.try_pay(&payer, &id)));
    s.env.mock_all_auths();
    assert_eq!(s.client.get_payment(&id).status, Status::Created);
    assert_eq!(s.balance(&payer), 100);
}

#[test]
fn pay_without_any_auth_fails() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    let payer = s.funded_payer(100);
    s.env.set_auths(&[]);
    assert!(is_auth_failure(&s.client.try_pay(&payer, &id)));
}

// ---------------------------------------------------------------- auto settle

#[test]
fn pay_auto_settles_without_escrow() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 1_000, SettleMode::Auto);
    let payer = s.funded_payer(1_000);
    s.client.pay(&payer, &id);
    let names = s.event_names();
    let events = s.contract_events();

    let p = s.client.get_payment(&id);
    assert_eq!(p.status, Status::Settled);
    assert_eq!(p.closed_at, Some(START_TS));
    assert_eq!(s.balance(&s.merchant), 990);
    assert_eq!(s.balance(&s.fee_recipient), 10);
    assert_eq!(s.balance(&s.contract), 0);
    assert_eq!(names, vec![symbol_short!("paid"), symbol_short!("settled")]);
    let ev: SettledEvent = events[1].1.into_val(&s.env);
    assert_eq!((ev.merchant_amount, ev.fee), (990, 10));
    assert_eq!(ev.fee_recipient, s.fee_recipient);
}

#[test]
fn auto_settle_fee_rounding_table() {
    // (amount, fee_bps, expected_fee)
    let cases: [(i128, u32, i128); 9] = [
        (1, 100, 0),
        (99, 100, 0),
        (100, 100, 1),
        (199, 100, 1),
        (1, 0, 0),
        (1_000_000, 0, 0),
        (1, MAX_FEE_BPS, 0),
        (20, MAX_FEE_BPS, 1),
        (1_000_000_000_000, MAX_FEE_BPS, 50_000_000_000),
    ];
    for (i, (amount, fee_bps, expected_fee)) in cases.iter().enumerate() {
        let s = Setup::new();
        s.client.initialize(
            &s.admin,
            &s.token.address,
            fee_bps,
            &s.fee_recipient,
            &RECLAIM_DELAY,
        );
        let id = s.create(i as u8, *amount, SettleMode::Auto);
        let payer = s.funded_payer(*amount);
        s.client.pay(&payer, &id);
        assert_eq!(s.balance(&s.fee_recipient), *expected_fee, "case {i}");
        assert_eq!(s.balance(&s.merchant), amount - expected_fee, "case {i}");
        assert_eq!(s.balance(&s.contract), 0, "case {i}");
    }
}

// --------------------------------------------------------------------- settle

#[test]
fn settle_pays_merchant_and_fee_recipient_exactly() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 12_345);
    s.advance(5);
    s.client.settle(&id);
    assert_eq!(s.env.auths()[0].0, s.operator);
    assert_eq!(s.event_names(), vec![symbol_short!("settled")]);
    let p = s.client.get_payment(&id);
    assert_eq!(p.status, Status::Settled);
    assert_eq!(p.closed_at, Some(START_TS + 5));
    assert_eq!(s.balance(&s.merchant), 12_222);
    assert_eq!(s.balance(&s.fee_recipient), 123);
    assert_eq!(s.balance(&s.contract), 0);
}

#[test]
fn settle_twice_fails() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 100);
    s.client.settle(&id);
    assert_eq!(s.client.try_settle(&id), Err(Ok(Error::InvalidState)));
    assert_eq!(s.balance(&s.merchant), 99);
}

#[test]
fn settle_requires_paid_state() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    assert_eq!(s.client.try_settle(&id), Err(Ok(Error::InvalidState)));
    assert_eq!(s.client.try_settle(&s.id(7)), Err(Ok(Error::NotFound)));
}

#[test]
fn settle_requires_operator_auth_not_admin_or_merchant() {
    let s = Setup::new();
    s.init();
    let (id, payer) = s.paid(1, 100);
    for who in [s.admin.clone(), s.merchant.clone(), payer.clone()] {
        s.mock_only(&who, "settle", (id.clone(),).into_val(&s.env));
        assert!(is_auth_failure(&s.client.try_settle(&id)));
    }
    s.env.mock_all_auths();
    assert_eq!(s.client.get_payment(&id).status, Status::Paid);
    assert_eq!(s.balance(&s.contract), 100);
}

// --------------------------------------------------------------------- cancel

#[test]
fn cancel_created_payment() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    s.advance(1);
    s.client.cancel(&id);
    assert_eq!(s.env.auths()[0].0, s.operator);
    let ev: CancelledEvent = s.contract_events()[0].1.into_val(&s.env);
    let p = s.client.get_payment(&id);
    assert_eq!(p.status, Status::Cancelled);
    assert_eq!(p.closed_at, Some(START_TS + 1));
    assert_eq!(ev.payment_id, id);
}

#[test]
fn cancel_rejects_paid_and_terminal_payments() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 100);
    assert_eq!(s.client.try_cancel(&id), Err(Ok(Error::InvalidState)));
    let id2 = s.create(2, 100, SettleMode::Manual);
    s.client.cancel(&id2);
    assert_eq!(s.client.try_cancel(&id2), Err(Ok(Error::InvalidState)));
}

#[test]
fn cancel_requires_operator_auth() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    s.mock_only(&s.merchant, "cancel", (id.clone(),).into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_cancel(&id)));
}

// --------------------------------------------------------------------- refund

#[test]
fn refund_returns_full_amount_to_payer() {
    let s = Setup::new();
    s.init();
    let (id, payer) = s.paid(1, 1_000);
    s.client.refund(&id);
    assert_eq!(s.env.auths()[0].0, s.operator);
    let ev: RefundedEvent = s.contract_events()[0].1.into_val(&s.env);
    assert_eq!(s.client.get_payment(&id).status, Status::Refunded);
    assert_eq!(s.balance(&payer), 1_000);
    assert_eq!(s.balance(&s.contract), 0);
    assert_eq!(s.balance(&s.merchant), 0);
    assert_eq!(ev.reason, RefundReason::Operator);
    assert_eq!(ev.amount, 1_000);
}

#[test]
fn refund_rejects_wrong_states() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    assert_eq!(s.client.try_refund(&id), Err(Ok(Error::InvalidState)));
    let (id2, _) = s.paid(2, 100);
    s.client.refund(&id2);
    assert_eq!(s.client.try_refund(&id2), Err(Ok(Error::InvalidState)));
    let (id3, _) = s.paid(3, 100);
    s.client.settle(&id3);
    assert_eq!(s.client.try_refund(&id3), Err(Ok(Error::InvalidState)));
    assert_eq!(s.client.try_settle(&id2), Err(Ok(Error::InvalidState)));
}

#[test]
fn refund_requires_operator_auth() {
    let s = Setup::new();
    s.init();
    let (id, payer) = s.paid(1, 100);
    for who in [s.admin.clone(), payer.clone()] {
        s.mock_only(&who, "refund", (id.clone(),).into_val(&s.env));
        assert!(is_auth_failure(&s.client.try_refund(&id)));
    }
}

// -------------------------------------------------------------------- reclaim

#[test]
fn reclaim_too_early_fails() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 100);
    s.advance(RECLAIM_DELAY - 1);
    assert_eq!(s.client.try_reclaim(&id), Err(Ok(Error::TooEarly)));
}

#[test]
fn reclaim_at_exact_deadline_succeeds() {
    let s = Setup::new();
    s.init();
    let (id, payer) = s.paid(1, 100);
    s.advance(RECLAIM_DELAY);
    s.client.reclaim(&id);
    assert_eq!(s.env.auths()[0].0, payer);
    let ev: RefundedEvent = s.contract_events()[0].1.into_val(&s.env);
    assert_eq!(s.client.get_payment(&id).status, Status::Refunded);
    assert_eq!(s.balance(&payer), 100);
    assert_eq!(ev.reason, RefundReason::Reclaim);
}

#[test]
fn reclaim_requires_payer_auth() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 100);
    s.advance(RECLAIM_DELAY);
    for who in [s.operator.clone(), s.admin.clone(), s.merchant.clone()] {
        s.mock_only(&who, "reclaim", (id.clone(),).into_val(&s.env));
        assert!(is_auth_failure(&s.client.try_reclaim(&id)));
    }
}

#[test]
fn reclaim_rejects_wrong_states() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    s.advance(RECLAIM_DELAY);
    assert_eq!(s.client.try_reclaim(&id), Err(Ok(Error::InvalidState)));
    let (id2, _) = s.paid(2, 100);
    s.client.settle(&id2);
    s.advance(RECLAIM_DELAY);
    assert_eq!(s.client.try_reclaim(&id2), Err(Ok(Error::InvalidState)));
}

#[test]
fn reclaim_deadline_overflow_is_an_error_not_a_panic() {
    let s = Setup::new();
    s.client.initialize(
        &s.admin,
        &s.token.address,
        &FEE_BPS,
        &s.fee_recipient,
        &u64::MAX,
    );
    let (id, _) = s.paid(1, 100);
    assert_eq!(s.client.try_reclaim(&id), Err(Ok(Error::Overflow)));
}

// ---------------------------------------------------------------------- pause

#[test]
fn pausing_never_traps_funds() {
    let s = Setup::new();
    s.init();
    let (a, _) = s.paid(1, 100);
    let (b, payer_b) = s.paid(2, 100);
    let (c, payer_c) = s.paid(3, 100);
    s.client.set_paused(&true);
    s.client.settle(&a);
    s.client.refund(&b);
    s.advance(RECLAIM_DELAY);
    s.client.reclaim(&c);
    assert_eq!(s.balance(&s.contract), 0);
    assert_eq!(s.balance(&s.merchant), 99);
    assert_eq!(s.balance(&payer_b), 100);
    assert_eq!(s.balance(&payer_c), 100);
    let id = s.create_unpaused_after(4);
    assert_eq!(s.client.get_payment(&id).status, Status::Created);
}

impl Setup {
    fn create_unpaused_after(&self, n: u8) -> BytesN<32> {
        self.client.set_paused(&false);
        self.create(n, 10, SettleMode::Manual)
    }
}

#[test]
fn set_paused_updates_config_and_emits_event() {
    let s = Setup::new();
    s.init();
    s.client.set_paused(&true);
    assert_eq!(s.env.auths()[0].0, s.admin);
    let ev: PausedEvent = s.contract_events()[0].1.into_val(&s.env);
    let c = s.client.get_config();
    assert!(c.paused);
    assert_eq!(c.version, 2);
    assert!(ev.paused);
    assert_eq!(ev.config_version, 2);
}

// ---------------------------------------------------------------------- admin

#[test]
fn fee_change_does_not_affect_paid_payments() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 10_000);
    let new_recipient = Address::generate(&s.env);
    s.client.set_fee(&MAX_FEE_BPS, &new_recipient);
    s.client.settle(&id);
    assert_eq!(s.balance(&s.fee_recipient), 100);
    assert_eq!(s.balance(&new_recipient), 0);
    assert_eq!(s.balance(&s.merchant), 9_900);
}

#[test]
fn fee_is_snapshotted_at_pay_not_create() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 10_000, SettleMode::Manual);
    s.client.set_fee(&200, &s.fee_recipient);
    let payer = s.funded_payer(10_000);
    s.client.pay(&payer, &id);
    assert_eq!(s.client.get_payment(&id).fee_bps, 200);
    s.client.settle(&id);
    assert_eq!(s.balance(&s.fee_recipient), 200);
}

#[test]
fn set_fee_validates_and_emits_event() {
    let s = Setup::new();
    s.init();
    let r = s.client.try_set_fee(&(MAX_FEE_BPS + 1), &s.fee_recipient);
    assert_eq!(r, Err(Ok(Error::FeeTooHigh)));
    let r = s.client.try_set_fee(&10, &s.contract);
    assert_eq!(r, Err(Ok(Error::InvalidAddress)));
    let new_recipient = Address::generate(&s.env);
    s.client.set_fee(&250, &new_recipient);
    assert_eq!(s.env.auths()[0].0, s.admin);
    let ev: FeeChangedEvent = s.contract_events()[0].1.into_val(&s.env);
    let c = s.client.get_config();
    assert_eq!(
        (c.fee_bps, c.fee_recipient.clone(), c.version),
        (250, new_recipient.clone(), 2)
    );
    assert_eq!((ev.fee_bps, ev.fee_recipient), (250, new_recipient));
}

#[test]
fn admin_functions_require_admin_auth() {
    let s = Setup::new();
    s.init();
    let stranger = Address::generate(&s.env);
    s.mock_only(
        &s.operator,
        "set_fee",
        (10_u32, stranger.clone()).into_val(&s.env),
    );
    assert!(is_auth_failure(&s.client.try_set_fee(&10, &stranger)));
    s.mock_only(&s.operator, "set_paused", (true,).into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_set_paused(&true)));
    s.mock_only(
        &stranger,
        "propose_admin",
        (stranger.clone(),).into_val(&s.env),
    );
    assert!(is_auth_failure(&s.client.try_propose_admin(&stranger)));
    s.env.mock_all_auths();
    let c = s.client.get_config();
    assert_eq!(
        (c.fee_bps, c.paused, c.admin),
        (FEE_BPS, false, s.admin.clone())
    );
}

#[test]
fn two_step_admin_transfer() {
    let s = Setup::new();
    s.init();
    assert_eq!(s.client.try_accept_admin(), Err(Ok(Error::NotFound)));

    let new_admin = Address::generate(&s.env);
    s.client.propose_admin(&new_admin);
    assert_eq!(s.client.get_config().admin, s.admin);

    // Only the pending admin can accept.
    s.mock_only(&s.admin, "accept_admin", ().into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_accept_admin()));

    s.env.mock_all_auths();
    s.client.accept_admin();
    assert_eq!(s.env.auths()[0].0, new_admin);
    assert_eq!(s.event_names(), vec![sym(&s.env, "admin_changed")]);
    let c = s.client.get_config();
    assert_eq!(c.admin, new_admin);
    assert_eq!(c.version, 2);
    assert_eq!(s.client.try_accept_admin(), Err(Ok(Error::NotFound)));

    // The old admin lost its powers.
    s.mock_only(&s.admin, "set_paused", (true,).into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_set_paused(&true)));
}

#[test]
fn admin_cannot_move_escrowed_funds() {
    let s = Setup::new();
    s.init();
    let (id, _) = s.paid(1, 1_000);
    // Every admin power, exercised while funds are escrowed.
    s.client.set_fee(&MAX_FEE_BPS, &s.admin);
    s.client.set_paused(&true);
    s.client.propose_admin(&s.admin);
    s.client.accept_admin();
    assert_eq!(s.balance(&s.contract), 1_000);
    assert_eq!(s.balance(&s.admin), 0);
    // Lifecycle actions need the operator or payer, never the admin.
    s.mock_only(&s.admin, "settle", (id.clone(),).into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_settle(&id)));
    s.mock_only(&s.admin, "refund", (id.clone(),).into_val(&s.env));
    assert!(is_auth_failure(&s.client.try_refund(&id)));
    s.env.mock_all_auths();
    s.client.settle(&id);
    // The fee recipient snapshotted at pay is paid, not the admin.
    assert_eq!(s.balance(&s.fee_recipient), 10);
    assert_eq!(s.balance(&s.admin), 0);
}

// ------------------------------------------------------------------- bump_ttl

#[test]
fn bump_ttl_extends_payment_entry() {
    let s = Setup::new();
    s.init();
    let id = s.create(1, 100, SettleMode::Manual);
    let key = DataKey::Payment(id.clone());
    let ttl_at = |s: &Setup| {
        s.env
            .as_contract(&s.contract, || s.env.storage().persistent().get_ttl(&key))
    };
    let initial = ttl_at(&s);
    assert_eq!(initial, crate::storage::PAYMENT_TTL_EXTEND_TO);

    // Move close to expiry, below the extension threshold.
    let seq = s.env.ledger().sequence();
    s.env
        .ledger()
        .set_sequence_number(seq + initial - crate::storage::PAYMENT_TTL_THRESHOLD + 10);
    let low = ttl_at(&s);
    assert!(low < crate::storage::PAYMENT_TTL_THRESHOLD);
    s.client.bump_ttl(&id);
    assert_eq!(ttl_at(&s), crate::storage::PAYMENT_TTL_EXTEND_TO);
    assert_eq!(s.client.try_bump_ttl(&s.id(9)), Err(Ok(Error::NotFound)));
}

// ----------------------------------------------------------------- invariants

/// xorshift64*; deterministic so failures are reproducible.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Model-based random test: applies random operation sequences and checks
/// after every step that every balance matches an independent model in which
/// funds can only flow payer -> contract -> {merchant, fee recipient, payer}.
/// Covers invariants 1, 2, 3, 4 and 8.
#[test]
fn invariants_hold_under_random_operation_sequences() {
    for seed in 1..=6_u64 {
        let s = Setup::new();
        s.init();
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let payers: StdVec<Address> = (0..4).map(|_| s.funded_payer(1_000_000)).collect();
        let mut model_balance: StdVec<i128> = vec![1_000_000; payers.len()];
        let mut merchant = 0_i128;
        let mut fees = 0_i128;
        // per payment: (amount, mode, status, payer index, fee_bps at pay)
        let mut model: StdVec<(i128, SettleMode, Status, usize, u32)> = StdVec::new();
        let mut fee_bps = FEE_BPS;

        for _step in 0..120 {
            let op = rng.below(8);
            let n = model.len();
            match op {
                0 | 1 if n < 60 => {
                    let amount = 1 + rng.below(50_000) as i128;
                    let mode = if rng.below(3) == 0 {
                        SettleMode::Auto
                    } else {
                        SettleMode::Manual
                    };
                    s.create(n as u8, amount, mode);
                    model.push((amount, mode, Status::Created, usize::MAX, 0));
                }
                2 | 3 if n > 0 => {
                    let i = rng.below(n as u64) as usize;
                    let p = rng.below(payers.len() as u64) as usize;
                    let r = s.client.try_pay(&payers[p], &s.id(i as u8));
                    let entry = &mut model[i];
                    if entry.2 == Status::Created && model_balance[p] >= entry.0 {
                        assert!(r.is_ok(), "seed {seed}: pay should succeed");
                        model_balance[p] -= entry.0;
                        entry.3 = p;
                        entry.4 = fee_bps;
                        if entry.1 == SettleMode::Auto {
                            let (m, f) = split_amount(entry.0, fee_bps).unwrap();
                            merchant += m;
                            fees += f;
                            entry.2 = Status::Settled;
                        } else {
                            entry.2 = Status::Paid;
                        }
                    } else {
                        assert!(r.is_err(), "seed {seed}: pay should fail");
                    }
                }
                4 if n > 0 => {
                    let i = rng.below(n as u64) as usize;
                    let r = s.client.try_settle(&s.id(i as u8));
                    let entry = &mut model[i];
                    if entry.2 == Status::Paid {
                        assert!(r.is_ok());
                        let (m, f) = split_amount(entry.0, entry.4).unwrap();
                        merchant += m;
                        fees += f;
                        entry.2 = Status::Settled;
                    } else {
                        assert_eq!(r, Err(Ok(Error::InvalidState)));
                    }
                }
                5 if n > 0 => {
                    let i = rng.below(n as u64) as usize;
                    let r = s.client.try_refund(&s.id(i as u8));
                    let entry = &mut model[i];
                    if entry.2 == Status::Paid {
                        assert!(r.is_ok());
                        model_balance[entry.3] += entry.0;
                        entry.2 = Status::Refunded;
                    } else {
                        assert_eq!(r, Err(Ok(Error::InvalidState)));
                    }
                }
                6 if n > 0 => {
                    let i = rng.below(n as u64) as usize;
                    let r = s.client.try_cancel(&s.id(i as u8));
                    let entry = &mut model[i];
                    if entry.2 == Status::Created {
                        assert!(r.is_ok());
                        entry.2 = Status::Cancelled;
                    } else {
                        assert_eq!(r, Err(Ok(Error::InvalidState)));
                    }
                }
                _ => {
                    fee_bps = rng.below(u64::from(MAX_FEE_BPS) + 1) as u32;
                    s.client.set_fee(&fee_bps, &s.fee_recipient);
                }
            }

            // Invariant 3: escrow balance == sum of Paid amounts.
            let escrowed: i128 = model
                .iter()
                .filter(|e| e.2 == Status::Paid)
                .map(|e| e.0)
                .sum();
            assert_eq!(s.balance(&s.contract), escrowed, "seed {seed}");
            // Invariants 1/4/8: every balance matches the model exactly.
            assert_eq!(s.balance(&s.merchant), merchant, "seed {seed}");
            assert_eq!(s.balance(&s.fee_recipient), fees, "seed {seed}");
            for (p, payer) in payers.iter().enumerate() {
                assert_eq!(s.balance(payer), model_balance[p], "seed {seed}");
            }
            assert_eq!(s.balance(&s.operator), 0);
            assert_eq!(s.balance(&s.admin), 0);
            // Conservation of supply.
            let total: i128 = model_balance.iter().sum::<i128>() + escrowed + merchant + fees;
            assert_eq!(total, 1_000_000 * payers.len() as i128);
            // On-chain status matches the model (one-way state machine).
            for (i, e) in model.iter().enumerate() {
                assert_eq!(s.client.get_payment(&s.id(i as u8)).status, e.2);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Invariant 4: fee + merchant_amount == amount, fee is the floor of
    /// amount * bps / 10_000 and never exceeds the cap.
    #[test]
    fn split_amount_conserves_value(amount in 1_i128..=i128::MAX / 1_000, fee_bps in 0_u32..=MAX_FEE_BPS) {
        let (merchant, fee) = split_amount(amount, fee_bps).unwrap();
        prop_assert_eq!(merchant + fee, amount);
        prop_assert!(fee >= 0 && merchant > 0);
        prop_assert_eq!(fee, amount * i128::from(fee_bps) / 10_000);
        prop_assert!(fee * 10_000 <= amount * i128::from(MAX_FEE_BPS));
    }

    #[test]
    fn split_amount_is_monotonic_in_amount(amount in 1_i128..1_000_000_000_000, fee_bps in 0_u32..=MAX_FEE_BPS) {
        let (_, fee_a) = split_amount(amount, fee_bps).unwrap();
        let (_, fee_b) = split_amount(amount + 1, fee_bps).unwrap();
        prop_assert!(fee_b >= fee_a);
    }
}

#[test]
fn split_amount_rejects_bad_inputs() {
    assert_eq!(split_amount(0, 100), Err(Error::InvalidAmount));
    assert_eq!(split_amount(-5, 100), Err(Error::InvalidAmount));
    assert_eq!(split_amount(100, MAX_FEE_BPS + 1), Err(Error::FeeTooHigh));
    assert_eq!(split_amount(i128::MAX, MAX_FEE_BPS), Err(Error::Overflow));
}

// ---------------------------------------------------------------- test vectors

fn hex(bytes: &[u8]) -> StdString {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn strkey(addr: &Address) -> StdString {
    let s = addr.to_string();
    let mut buf = vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut buf);
    StdString::from_utf8(buf).unwrap()
}

fn xdr_b64(env: &Env, v: &Val) -> StdString {
    ScVal::try_from_val(env, v)
        .unwrap()
        .to_xdr_base64(Limits::none())
        .unwrap()
}

/// Renders one event as a JSON object: raw XDR plus the decoded fields the
/// API indexer is expected to produce.
fn vector_json(env: &Env, topics: &Vec<Val>, data: &Val, expected: &str) -> StdString {
    let name: Symbol = topics.get(1).unwrap().into_val(env);
    let name_val: Val = name.into_val(env);
    let name_str = match ScVal::try_from_val(env, &name_val).unwrap() {
        ScVal::Symbol(sym) => StdString::from_utf8(sym.to_vec()).unwrap(),
        _ => unreachable!(),
    };
    let topic_xdrs: StdVec<StdString> = topics
        .iter()
        .map(|t| format!("\"{}\"", xdr_b64(env, &t)))
        .collect();
    format!(
        "    {{\n      \"name\": \"{}\",\n      \"topics_xdr\": [{}],\n      \"data_xdr\": \"{}\",\n      \"expected\": {}\n    }}",
        name_str,
        topic_xdrs.join(", "),
        xdr_b64(env, data),
        expected
    )
}

fn build_test_vectors() -> StdString {
    let s = Setup::new();
    s.init();
    let env = &s.env;
    let mut out: StdVec<StdString> = StdVec::new();
    let mut push = |s: &Setup, expected: &dyn Fn(&Val) -> StdString| {
        for (topics, data) in s.contract_events() {
            out.push(vector_json(&s.env, &topics, &data, &expected(&data)));
        }
    };

    // created + paid (manual), then settled
    let id = s.create(0x11, 1_000_000, SettleMode::Manual);
    push(&s, &|d| {
        let e: CreatedEvent = d.try_into_val(env).unwrap();
        format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"operator\": \"{}\", \"merchant\": \"{}\", \"token\": \"{}\", \"amount\": \"{}\", \"reference\": \"{}\", \"settle_mode\": \"manual\", \"created_at\": {} }}",
            e.v, hex(&e.payment_id.to_array()), strkey(&e.operator), strkey(&e.merchant), strkey(&e.token), e.amount, hex(&e.reference.to_array()), e.created_at
        )
    });
    let payer = s.funded_payer(1_000_000);
    s.advance(30);
    s.client.pay(&payer, &id);
    push(&s, &|d| {
        let e: PaidEvent = d.try_into_val(env).unwrap();
        format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"payer\": \"{}\", \"amount\": \"{}\", \"fee_bps\": {}, \"paid_at\": {} }}",
            e.v, hex(&e.payment_id.to_array()), strkey(&e.payer), e.amount, e.fee_bps, e.paid_at
        )
    });
    s.advance(30);
    s.client.settle(&id);
    let settled_json = |d: &Val| {
        let e: SettledEvent = d.try_into_val(env).unwrap();
        format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"merchant\": \"{}\", \"fee_recipient\": \"{}\", \"merchant_amount\": \"{}\", \"fee\": \"{}\", \"settled_at\": {} }}",
            e.v, hex(&e.payment_id.to_array()), strkey(&e.merchant), strkey(&e.fee_recipient), e.merchant_amount, e.fee, e.settled_at
        )
    };
    push(&s, &settled_json);

    // refunded (operator) and refunded (reclaim)
    let refunded_json = |d: &Val| {
        let e: RefundedEvent = d.try_into_val(env).unwrap();
        let reason = match e.reason {
            RefundReason::Operator => "operator",
            RefundReason::Reclaim => "reclaim",
        };
        format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"payer\": \"{}\", \"amount\": \"{}\", \"reason\": \"{}\", \"refunded_at\": {} }}",
            e.v, hex(&e.payment_id.to_array()), strkey(&e.payer), e.amount, reason, e.refunded_at
        )
    };
    let (id2, _) = s.paid(0x22, 500);
    s.client.refund(&id2);
    push(&s, &refunded_json);
    let (id3, _) = s.paid(0x33, 700);
    s.advance(RECLAIM_DELAY);
    s.client.reclaim(&id3);
    push(&s, &refunded_json);

    // cancelled
    let id4 = s.create(0x44, 900, SettleMode::Manual);
    s.client.cancel(&id4);
    push(&s, &|d| {
        let e: CancelledEvent = d.try_into_val(env).unwrap();
        format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"cancelled_at\": {} }}",
            e.v,
            hex(&e.payment_id.to_array()),
            e.cancelled_at
        )
    });

    // auto: paid + settled in one call
    let id5 = s.create(0x55, 10_000, SettleMode::Auto);
    let payer5 = s.funded_payer(10_000);
    s.client.pay(&payer5, &id5);
    let events = s.contract_events();
    let e: PaidEvent = events[0].1.try_into_val(env).unwrap();
    out.push(vector_json(
        env,
        &events[0].0,
        &events[0].1,
        &format!(
            "{{ \"v\": {}, \"payment_id\": \"{}\", \"payer\": \"{}\", \"amount\": \"{}\", \"fee_bps\": {}, \"paid_at\": {} }}",
            e.v, hex(&e.payment_id.to_array()), strkey(&e.payer), e.amount, e.fee_bps, e.paid_at
        ),
    ));
    out.push(vector_json(
        env,
        &events[1].0,
        &events[1].1,
        &settled_json(&events[1].1),
    ));

    format!(
        "{{\n  \"description\": \"Contract event vectors generated by stellar-pay-core tests (cargo test). Regenerate with UPDATE_VECTORS=1 cargo test test_vectors_are_up_to_date.\",\n  \"event_version\": 1,\n  \"contract_id\": \"{}\",\n  \"events\": [\n{}\n  ]\n}}\n",
        strkey(&s.contract),
        out.join(",\n")
    )
}

#[test]
fn test_vectors_are_deterministic() {
    assert_eq!(build_test_vectors(), build_test_vectors());
}

#[test]
fn test_vectors_are_up_to_date() {
    let generated = build_test_vectors();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/test-vectors.json");
    if std::env::var("UPDATE_VECTORS").is_ok() {
        std::fs::write(path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        committed, generated,
        "docs/test-vectors.json is stale; run UPDATE_VECTORS=1 cargo test test_vectors_are_up_to_date"
    );
}

// ---------------------------------------------------------------------- costs

/// Prints metered resources for the key paths. Run with:
/// `cargo test cost_report -- --ignored --nocapture`. Numbers feed docs/COSTS.md.
#[test]
#[ignore]
fn cost_report() {
    let s = Setup::new();
    s.init();
    let report = |label: &str| {
        let r = s.env.cost_estimate().resources();
        let fee = s.env.cost_estimate().fee();
        std::println!(
            "| {label} | {} | {} | {} | {} | {} | {} | {} | {} |",
            r.instructions,
            r.mem_bytes,
            r.read_entries,
            r.write_entries,
            r.read_bytes,
            r.write_bytes,
            r.contract_events_size_bytes,
            fee.total
        );
    };
    std::println!("| path | instructions | mem bytes | read entries | write entries | read bytes | write bytes | event bytes | est. fee (stroops) |");
    let id = s.create(1, 1_000_000, SettleMode::Manual);
    report("create_payment");
    let payer = s.funded_payer(1_000_000);
    s.client.pay(&payer, &id);
    report("pay (manual, escrow)");
    s.client.settle(&id);
    report("settle");
    let id2 = s.create(2, 1_000_000, SettleMode::Auto);
    let payer2 = s.funded_payer(1_000_000);
    s.client.pay(&payer2, &id2);
    report("pay (auto-settle)");
    let (id3, _) = s.paid(3, 1_000_000);
    s.client.refund(&id3);
    report("refund");
    let id4 = s.create(4, 1_000_000, SettleMode::Manual);
    s.client.cancel(&id4);
    report("cancel");
}
