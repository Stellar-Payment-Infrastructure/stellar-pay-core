# AGENTS.md — stellar-pay-core

## Package: stellar-pay-core

Soroban contract for payment escrow/settlement with a strict state machine.

### Verify Command

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --target wasm32-unknown-unknown --release
```

### Key Files

- `contracts/payments/src/lib.rs` — main contract implementation
- `contracts/payments/src/types.rs` — data types (Config, Payment, Status, SettleMode)
- `contracts/payments/src/storage.rs` — storage helpers with TTL management
- `contracts/payments/src/events.rs` — event definitions and publishing
- `contracts/payments/src/error.rs` — contract errors
- `contracts/payments/src/test.rs` — unit tests (19 tests)

### State Machine

```
Created --pay--> Paid --settle--> Settled
              |           |
              |           +--refund--> Refunded
              |           |
              +--cancel--> Cancelled
                            |
                         reclaim
                            |
                         Refunded
```

### Rules

- No `unwrap()`/`expect()` in contract code
- Checked arithmetic everywhere (`checked_add`, `checked_mul`, `checked_div`)
- State before transfers (checks-effects-interactions)
- TTL extension on every persistent write
- Events for every state change
- Token operations via `soroban_sdk::token::Client` (SEP-41)
- Fee snapshotted at `pay` time
- Pausing only blocks `create_payment` and `pay`

### Test Requirements

- Every public function: happy path + failure paths
- Invariant tests: balance == sum(Paid), fee+merchant=amount, funds to allowed parties only
- Auth enforcement tests (no `mock_all_auths`)
- Property tests for fee math