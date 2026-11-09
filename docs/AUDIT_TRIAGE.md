# Dependency audit triage

Run: `cargo audit` on 2026-11-04. This is a dependency triage only. It is not the
adversarial contract review described in the playbook, which is still pending.

| Advisory | Crate | Status | Rationale |
|---|---|---|---|
| RUSTSEC-2026-0387 | crypto-bigint 0.5.5 | Accepted (temporary) | Transitive dependency of soroban-sdk host-side crates used by tests. Not compiled into the wasm contract. Revisit when soroban-sdk upgrades past it. |
| RUSTSEC-2024-0388 | derivative (unmaintained) | Accepted | Transitive via soroban-sdk. |
| RUSTSEC-2024-0436 | paste (unmaintained) | Accepted | Transitive via soroban-sdk. |
| RUSTSEC-2026-0097 | rand 0.8.5 (unsound with custom logger) | Accepted | Transitive; the contract installs no custom logger. |

Property tests (`proptest`) for fee math and the state machine exist in `contracts/payments/src/test.rs`.
