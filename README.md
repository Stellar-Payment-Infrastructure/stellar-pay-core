# stellar-pay-core

[![CI](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-core/actions/workflows/ci.yml)

**Status: testnet only · v0.1.0 · not professionally audited.** Do not use with real funds.

A Soroban smart contract that tracks a merchant payment through a strict one-way state machine and settles it
atomically in a single SEP-41 token. It is the on-chain part of the Stellar Payment Infrastructure suite:

| Repo | Role |
|---|---|
| **stellar-pay-core** (this repo) | Soroban contract: create, pay, settle, refund, cancel, reclaim |
| [stellar-pay-api](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-api) | REST API, event indexer, signed webhooks, OpenAPI, TypeScript SDK |
| [stellar-pay-frontend](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-frontend) | React checkout components and a demo storefront |
| [stellar-pay-analytics](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-analytics) | Aggregator service and dashboard |

## How it works

```
            create_payment
 (none) ───────────────────▶ Created ──cancel──▶ Cancelled
                                │
                                │ pay
                                ▼
                              Paid ──refund / reclaim──▶ Refunded
                                │
                                │ settle   (or inside pay when settle_mode = Auto)
                                ▼
                             Settled
```

- An **operator** (in the hosted model, the API's service key) creates a payment for a **merchant** with a random
  32-byte id, an amount in token base units and an opaque 32-byte reference hash.
- The **payer** signs `pay`. In `Auto` mode the funds go straight to the merchant and fee recipient in the same
  transaction and are never held. In `Manual` mode they wait in the contract until the operator settles or refunds.
- Funds can only ever move to the payment's merchant, the fee recipient, or back to the payer. A rogue operator
  can stall a payment but not redirect it, and the payer can `reclaim` escrowed funds after `reclaim_delay`.
- Every state change emits a versioned event; the API indexes them to update its database and send webhooks.

## What is implemented

- Payment lifecycle: `create_payment`, `pay` (Auto or Manual settle), `settle`, `cancel`, `refund`, payer `reclaim`.
- Fees in basis points, capped at 500 (5%), with floor rounding. Fee terms are fixed when the payment is created.
- Admin: `set_fee`, `set_paused` (pause blocks only `create_payment` and `pay`, never exits), two-step admin
  transfer (`propose_admin` / `accept_admin`).
- Storage TTL extension on every write, plus a permissionless `bump_ttl`.
- Versioned events for every state change ([docs/EVENTS.md](docs/EVENTS.md), XDR vectors in
  [docs/test-vectors.json](docs/test-vectors.json)).
- 54 tests: each function with its error paths, auth tests that authorize only the expected address (no blanket
  `mock_all_auths`), property tests for the fee math, and a model-based random-sequence test that checks the
  invariants after every step.

## Not implemented yet

- **One token per deployment.** Payments carry a `token` field, but only the configured token is accepted.
  A token allowlist is planned ([Wave issue core-1](ROADMAP.md)).
- **No invoice expiry.** `Created` payments never expire ([core-3](ROADMAP.md)). `reclaim` covers paid funds.
- **No conditional payments.** `Condition` has only a `None` variant, kept as an extension point.
- **No upgrade entry point.** The upgrade policy is "redeploy on testnet" ([SPEC §10](docs/SPEC.md#10-upgrade-policy)).
- **No mainnet deployment**, and no professional audit.

## Testnet deployment

| | |
|---|---|
| Contract | [`CDB5PL5RITGAI6FBYTICPZMQIFROZEMXYMSKFGVK5NOUEX2XLYQ3IS4I`](https://stellar.expert/explorer/testnet/contract/CDB5PL5RITGAI6FBYTICPZMQIFROZEMXYMSKFGVK5NOUEX2XLYQ3IS4I) |
| Token | Native XLM through its Stellar Asset Contract, `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC` (7 decimals) |
| Fee | 100 bps (1%) |
| Reclaim delay | 3600 s (short on purpose, to allow manual testing) |
| Wasm SHA-256 | `3de372a2fe0eff01b8d149c024f3c90366ee0058fd47e4b7490d49447623aa41` |

Full record: [deployments/testnet.json](deployments/testnet.json). Smoke test transcript (create → pay → settle,
cancel, refund, and the audit fixes checked live): [docs/SMOKE_TEST.md](docs/SMOKE_TEST.md).

The build is reproducible: `stellar contract build --optimize` (Stellar CLI 30.0.1, rustc 1.99.0) on this source
produces the same SHA-256 as the wasm fetched from testnet with `stellar contract fetch`.
Testnet is reset periodically; after a reset the contract must be redeployed and this table updated.

## Interface

| Function | Auth | Effect |
|---|---|---|
| `initialize(admin, token, fee_bps, fee_recipient, reclaim_delay)` | admin | One-time setup. `reclaim_delay` must be 1 hour to 90 days |
| `create_payment(operator, merchant, payment_id, amount, reference, settle_mode)` | operator | → `Created` |
| `pay(payer, payment_id)` | payer | → `Paid` (Manual) or `Settled` (Auto) |
| `settle(payment_id)` | payment operator | `Paid → Settled` |
| `cancel(payment_id)` | payment operator | `Created → Cancelled` |
| `refund(payment_id)` | payment operator | `Paid → Refunded`, full amount to the payer |
| `reclaim(payment_id)` | payer | `Paid → Refunded` after `paid_at + reclaim_delay` |
| `get_payment(payment_id)`, `get_config()` | none | Read-only |
| `set_fee`, `set_paused`, `propose_admin`, `accept_admin` | admin / pending admin | Configuration |
| `bump_ttl(payment_id)` | none | Extends storage TTL |

The full specification, invariants, error codes and threat model are in [docs/SPEC.md](docs/SPEC.md).
Resource-cost baseline: [docs/COSTS.md](docs/COSTS.md).

## Build and test

Requirements: Rust stable with the `wasm32-unknown-unknown` target, and the
[Stellar CLI](https://developers.stellar.org/docs/tools/cli) for deployment.

```bash
rustup target add wasm32-unknown-unknown
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --target wasm32-unknown-unknown --release
```

## Deploy your own (testnet)

The deployed build was produced with `stellar contract build --optimize` (Stellar CLI 30.0.1). Initialize right
after deploying and check `get_config`: until `initialize` runs, anyone could initialize the contract with their own
admin (finding M5, see [Security](#security)).

```bash
stellar keys generate sp-admin --network testnet --fund
stellar contract build --optimize
stellar contract deploy --wasm target/wasm32v1-none/release/stellar_pay_core.wasm \
  --source-account sp-admin --network testnet --alias stellar-pay-core
stellar contract invoke --id stellar-pay-core --source-account sp-admin --network testnet -- initialize \
  --admin <G... admin> --token <C... token SAC> --fee_bps 100 --fee_recipient <G...> --reclaim_delay 3600
stellar contract invoke --id stellar-pay-core --source-account sp-admin --network testnet -- get_config
```

## Security

The contract had an adversarial review (an independent AI agent working from the code, then maintainer triage), not
a professional audit. Findings and their dispositions are in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md);
dependency advisories in [docs/AUDIT_TRIAGE.md](docs/AUDIT_TRIAGE.md). Three medium/low findings are accepted for
v0.1.0, with mitigations:

- **M1, payment id squatting:** `pay` signs only the id, so someone who creates an id first under their own merchant
  could receive a payment meant for another. The API generates ids from 32 random bytes and refuses to build a pay
  transaction unless the on-chain merchant, amount and operator match its record. Integrators that call the
  contract directly must do the same check.
- **M5, initialization front-running:** initialize in the same session as the deploy and verify `get_config`.
- **L2, admin proposals:** a pending admin proposal never expires and `propose_admin` emits no event.

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Compatibility

| stellar-pay-core | stellar-pay-api | stellar-pay-sdk | stellar-pay-frontend | stellar-pay-analytics |
|---|---|---|---|---|
| 0.1.x | 0.1.x | 0.1.x | 0.1.x | 0.1.x |

## How this compares

Other Stellar payment projects we looked at (November 2026, from their public docs) solve different problems:
[Trustless Work](https://docs.trustlesswork.com/trustless-work) is escrow-as-a-service for milestone-based stablecoin
escrow with approvals and disputes; [StellarPay](https://cdn.jsdelivr.net/wp/stellarpay/trunk/readme.md) is a
WordPress/WooCommerce plugin for classic Stellar payments; [link2pay](https://link2pay.github.io/docs-link2pay) offers
a payments REST API and SDK. This project focuses on merchant checkout: an on-chain payment state machine
(instead of matching classic payments by memo), auto-settle or escrow per payment with a payer reclaim safety valve,
and an open-source stack around it (API with indexer and signed webhooks, React components, analytics).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the open Wave issues in [ROADMAP.md](ROADMAP.md).

## License

[MIT](LICENSE)
