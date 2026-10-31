# stellar-pay-core — Specification (v0.1.0)

Soroban contract that tracks a payment through a strict one-way state machine
and moves a single SEP-41 token between payer, contract escrow, merchant and
fee recipient. Source: `contracts/payments/src/`.

## 1. State machine

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

Terminal states: `Cancelled`, `Refunded`, `Settled`. No function moves a
payment out of a terminal state. `Auto` payments go `Created → Settled` in one
`pay` call (a `paid` and a `settled` event are both emitted) and never sit in
escrow.

## 2. Roles

| Role | Who | Powers |
|---|---|---|
| admin | set at `initialize` | `set_fee` (capped at 500 bps), `set_paused`, two-step admin transfer. Cannot move escrowed funds. |
| operator | per payment (`create_payment` arg) | `create_payment`, `settle`, `cancel`, `refund` for its payments. Funds can only go to the merchant, the fee recipient or the payer, so a rogue operator can stall but not steal. In the hosted model the API's service key is the operator; self-hosters may use operator = merchant. |
| merchant | per payment | Receives `amount − fee` on settle. No powers. |
| payer | set by `pay` | Funds the payment; may `reclaim` after `reclaim_delay`. |
| fee recipient | config, snapshotted at `pay` | Receives `fee` on settle. |

## 3. Functions

| Function | Auth | Effect / errors |
|---|---|---|
| `initialize(admin, token, fee_bps, fee_recipient, reclaim_delay)` | admin | One-time. `AlreadyInitialized`, `FeeTooHigh`, `InvalidAddress` (token or fee recipient = this contract). |
| `get_config()` | none | Read-only. `NotInitialized`. |
| `create_payment(operator, merchant, payment_id, amount, reference, settle_mode)` | operator | → `Created`. `Paused`, `InvalidAmount` (≤ 0), `InvalidAddress` (merchant = contract), `AlreadyExists` (duplicate id). Token = configured token. |
| `pay(payer, payment_id)` | payer (covers nested token transfer) | Snapshots `fee_bps` and `fee_recipient`. Manual: payer → contract, `Paid`. Auto: payer → merchant (`amount − fee`) and payer → fee recipient (`fee`), `Settled`. `Paused`, `NotFound`, `InvalidState`, `TokenNotAllowed`, `InvalidAddress` (payer = contract); token errors (e.g. insufficient balance) revert the whole call. |
| `settle(payment_id)` | payment operator | `Paid → Settled`, contract → merchant and fee recipient. Works while paused. `NotFound`, `InvalidState`. |
| `cancel(payment_id)` | payment operator | `Created → Cancelled`. `InvalidState`. |
| `refund(payment_id)` | payment operator | `Paid → Refunded`, full `amount` to payer. Works while paused. |
| `reclaim(payment_id)` | payer | `Paid → Refunded` once `ledger.timestamp ≥ paid_at + reclaim_delay`. Works while paused. `TooEarly`, `Overflow` (deadline overflows u64). |
| `get_payment(payment_id)` | none | Read-only. `NotFound`. |
| `set_fee(fee_bps, fee_recipient)` | admin | `FeeTooHigh`, `InvalidAddress`. Affects only payments paid afterwards. |
| `set_paused(paused)` | admin | Blocks `create_payment` and `pay` only. |
| `propose_admin(new_admin)` | admin | Stores pending admin (a later call replaces it). |
| `accept_admin()` | pending admin | `NotFound` if nothing pending. |
| `bump_ttl(payment_id)` | none | Extends payment + instance TTL. `NotFound`. |

Every state-changing function reads config first (`NotInitialized` before
`initialize`) and extends the instance TTL.

## 4. Invariants

Each has at least one test in `src/test.rs`; 1–4 and 8 are also checked
after every step of a randomized, model-based operation sequence test
(`invariants_hold_under_random_operation_sequences`), and 4 by proptest.

1. Funds only move to the payment's merchant, the fee recipient, or the payer.
2. `settle` and `refund`/`reclaim` happen at most once per payment.
3. Contract token balance == Σ `amount` over payments in `Paid`.
4. `fee + merchant_amount == amount`; `fee = floor(amount × fee_bps / 10_000)`.
5. Pausing never traps funds (`settle`, `refund`, `reclaim` ignore pause).
6. Admin cannot move escrowed funds.
7. `fee_bps` and `fee_recipient` are snapshotted at `pay`.
8. Operator cannot redirect funds to an arbitrary address (no function takes
   a destination address at settle/refund time).

## 5. Data model

See `src/types.rs`. `Config` lives in instance storage; `Payment(id)` in
persistent storage. Records carry `version` (storage schema, currently 1).
`Condition` has only `None` (extension point for time/hash locks, not in v1).
Integer-backed enums (`Status`, `SettleMode`, `RefundReason`, `Condition`)
encode as `u32` on-chain.

**No PII on-chain:** only ids, a 32-byte opaque `reference` hash, amounts,
addresses, status and ledger timestamps.

## 6. Events

Topics `("pay", <name>)`, data struct with schema version `v`. Full layout and
JSON examples: `docs/EVENTS.md`; XDR vectors: `docs/test-vectors.json`.

## 7. Errors

`NotInitialized=1, AlreadyInitialized=2, NotFound=3, AlreadyExists=4,
Unauthorized=5, InvalidAmount=6, InvalidState=7, Paused=8, TokenNotAllowed=9,
FeeTooHigh=10, TooEarly=11, Overflow=12, InvalidAddress=13`. Codes are stable
and never renumbered. (`Unauthorized` is reserved; auth failures surface as
host auth errors from `require_auth`.)

## 8. Token assumptions

One token per deployment (Wave core-1 adds an allowlist; plug-in point
`is_token_allowed`). The token must have standard SEP-41 transfer semantics:
fee-on-transfer, rebasing or hook-bearing tokens would break invariant 3.
Stellar Asset Contracts (XLM and classic assets) satisfy this. Classic-asset
transfers fail if the recipient lacks a trustline; such a failure reverts the
whole call (no partial state).

## 9. Storage and TTL

Instance (config): extended to ~60 days when below ~30 days, on every call.
Payments: extended to ~30 days on every write and by `bump_ttl` when below
~7 days. Terminal payments may archive; the API database is the long-term
history. An archived entry must be restored before it can be read again.

## 10. Upgrade policy

The contract has no upgrade entry point. v1 policy: **redeploy on testnet**
and point the API at the new contract id. Records carry a `version` so a
future v2 can read v1 records (to be tested when a v2 exists).

## 11. Threat model

| Threat | Mitigation |
|---|---|
| Malicious operator | Can stall (never settle) but not steal: destinations are fixed at create (merchant) and pay (payer, fee recipient). Payer can `reclaim` after `reclaim_delay`. |
| Malicious payer | Must authorize `pay` and fund it; cannot pay twice (state check) or reclaim early (`TooEarly`). |
| Malicious admin | Fee capped at 5% and snapshotted at `pay`; pause cannot block exits; no function lets admin move escrow. |
| Malicious token | Out of scope by configuration: only a vetted SAC is configured. |
| Griefing (id squatting) | Payment ids are random 32-byte values chosen by the operator; creation needs operator auth. |
| Replay / duplicate processing | Unique ids (`AlreadyExists`), one-way state machine, `settle`/`refund` once. |
| Initialization front-running | `initialize` requires admin auth, but anyone can initialize an uninitialized contract with their own admin. Operational mitigation: deploy and initialize back-to-back and verify `get_config` before use; redeploy if it does not match. |
| Funds sent to the contract itself | `merchant`, `payer`, `fee_recipient` and `token` may not be the contract address. |
