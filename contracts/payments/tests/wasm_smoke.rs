//! Loads the *built* release wasm into the Soroban host, runs the main payment
//! paths through the VM and prints metered resources (these feed
//! docs/COSTS.md). Ignored by default because it needs the wasm first:
//!
//! ```text
//! cargo build --target wasm32v1-none --release
//! cargo test --test wasm_smoke -- --ignored --nocapture
//! ```
//!
//! Set `STELLAR_PAY_WASM` to test a wasm at another path.

use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env,
};
use stellar_pay_core::{PaymentsContractClient, SettleMode, Status};

fn wasm_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("STELLAR_PAY_WASM") {
        return p.into();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32v1-none/release/stellar_pay_core.wasm")
}

fn report(env: &Env, label: &str) {
    let r = env.cost_estimate().resources();
    let fee = env.cost_estimate().fee();
    println!(
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
}

#[test]
#[ignore]
fn release_wasm_runs_payment_paths() {
    let path = wasm_path();
    let wasm = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    println!("wasm: {} ({} bytes)", path.display(), wasm.len());

    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let operator = Address::generate(&env);
    let merchant = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = TokenClient::new(&env, &sac.address());
    let token_admin = StellarAssetClient::new(&env, &sac.address());

    let contract = env.register(wasm.as_slice(), ());
    let client = PaymentsContractClient::new(&env, &contract);
    client.initialize(&admin, &token.address, &100, &fee_recipient, &3_600);

    println!("| path | instructions | mem bytes | read entries | write entries | read bytes | write bytes | event bytes | est. fee (stroops) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    let reference = BytesN::from_array(&env, &[7; 32]);
    let id = |n: u8| BytesN::from_array(&env, &[n; 32]);
    let amount = 10_000_000_i128;

    client.create_payment(
        &operator,
        &merchant,
        &id(1),
        &amount,
        &reference,
        &SettleMode::Manual,
    );
    report(&env, "create_payment");
    let payer = Address::generate(&env);
    token_admin.mint(&payer, &(amount * 4));
    client.pay(&payer, &id(1));
    report(&env, "pay (manual, escrow)");
    client.settle(&id(1));
    report(&env, "settle");

    client.create_payment(
        &operator,
        &merchant,
        &id(2),
        &amount,
        &reference,
        &SettleMode::Auto,
    );
    client.pay(&payer, &id(2));
    report(&env, "pay (auto-settle)");

    client.create_payment(
        &operator,
        &merchant,
        &id(3),
        &amount,
        &reference,
        &SettleMode::Manual,
    );
    client.pay(&payer, &id(3));
    client.refund(&id(3));
    report(&env, "refund");

    client.create_payment(
        &operator,
        &merchant,
        &id(4),
        &amount,
        &reference,
        &SettleMode::Manual,
    );
    client.cancel(&id(4));
    report(&env, "cancel");

    assert_eq!(client.get_payment(&id(1)).status, Status::Settled);
    assert_eq!(client.get_payment(&id(2)).status, Status::Settled);
    assert_eq!(client.get_payment(&id(3)).status, Status::Refunded);
    assert_eq!(client.get_payment(&id(4)).status, Status::Cancelled);
    assert_eq!(token.balance(&merchant), 2 * 9_900_000);
    assert_eq!(token.balance(&fee_recipient), 2 * 100_000);
    assert_eq!(token.balance(&contract), 0);
}
