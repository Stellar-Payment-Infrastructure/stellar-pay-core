# Roadmap

## v0.1.0 (current, testnet)

Payment lifecycle contract with Auto/Manual settlement, capped fees fixed at creation, payer reclaim, pause that
never traps funds, two-step admin transfer, versioned events, and a deployed testnet instance. See the
[README](README.md#what-is-implemented).

## Open Wave issues

These are deliberately left for contributors. Each has a `TODO(wave: core-N)` marker at its plug-in point.

| Id | Points | Issue | Plug-in point |
|---|---|---|---|
| core-1 | 4 | **Multi-currency support:** admin-managed token allowlist (add/remove/list), per-token limits, allowlist checks in `create_payment` and `pay` | `is_token_allowed` in `contracts/payments/src/lib.rs` |
| core-2 | 3 | **Versioned event schema for webhook consumers:** JSON Schema per event, extra fields (reference, fee), decoder conformance vectors, backward-compatibility policy | `contracts/payments/src/events.rs`, `docs/EVENTS.md` |
| core-3 | 3 | **Payment expiry:** `expires_at` at creation, `pay` rejected after expiry, permissionless `expire()`, `Expired` status and event | `create_payment` and `pay` in `contracts/payments/src/lib.rs` |
| core-4 | 2 | **Resource-fee optimization:** fewer ledger reads/writes/bytes, with before/after numbers against `docs/COSTS.md` | `contracts/payments/src/storage.rs` |

The contract cannot send webhooks itself (contracts emit events; the API delivers webhooks), which is why core-2 is
scoped as an event-schema task.

## Accepted for v0.1.0, to revisit

From [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md):

- **M1:** bind `pay` to the expected merchant and amount, so the contract itself rejects id squatting (today the
  API checks this off-chain before building a pay transaction).
- **M5:** replace `initialize` with a deploy-time constructor; add a TTL keeper for idle deployments.
- **L2:** expire pending admin proposals and emit an event from `propose_admin`.
- **M2, M3:** merchant co-signed settle; pull-based fees so a recipient that cannot receive the token cannot
  block settlement.

These change the contract interface, so they are planned for a later minor version with a redeploy.

## Not planned for v1

Mainnet (requires a professional audit first), conditional payments (time/hash locks; `Condition` is the extension
point), an upgrade entry point, and cross-asset settlement.
