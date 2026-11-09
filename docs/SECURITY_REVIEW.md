# Security review — stellar-pay-core

Date: 2026-11-07. Scope: `contracts/payments`.

**What this is:** an adversarial read of the contract by a separate AI agent working from the code only, then triage and
fixes by the maintainers. It is not a professional third-party audit. Get one before any mainnet use. The project is
testnet-only.

Dependency advisories are tracked separately in `AUDIT_TRIAGE.md`.

## Result

No way was found to move escrowed funds to anyone other than the merchant, the fee recipient or the payer. Every
mutating function authorizes the right address before writing state, token transfers happen after the state write,
and arithmetic is checked.

## Findings and disposition

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| H1 | High | `reclaim_delay` unbounded: huge values disable `reclaim`, zero lets a payer reclaim right after receiving goods | **Fixed.** `initialize` requires 1 hour to 90 days, else `InvalidDelay` (#14). Tests: `initialize_rejects_unreasonable_reclaim_delay`, `payer_can_always_reclaim_after_the_maximum_delay` |
| M4 | Medium | Admin could raise the fee (up to 5%) between `create` and `pay` | **Fixed.** Fee bps and recipient are snapshotted at `create_payment`. Test: `fee_terms_are_fixed_at_create_not_pay`. This changes the earlier documented design (snapshot at `pay`); SPEC updated |
| L1 | Low | Huge `amount` accepted at create, then `settle` overflows forever (refund/reclaim still work) | **Fixed.** `create_payment` rejects amounts whose maximum-fee split overflows. Test: `create_rejects_amounts_whose_fee_split_would_overflow` |
| M1 | Medium | `payment_id` is first come first served and `pay` signs only the id, so a squatter can create an id with their own merchant | **Deferred.** Needs a `pay` signature change that ripples through API, SDK and frontend. Mitigation until then: the API must use random ids and check `get_payment` (merchant, amount) before presenting a pay link |
| M2 | Medium | Operator alone controls `settle`/`refund` of Paid funds; merchant has no authority | **Accepted, by design.** The operator is a trusted role. Revisit with merchant-settle or co-sign |
| M3 | Medium | A fee recipient or merchant that cannot receive the token (revoked trustline) blocks `settle`; recipient is snapshotted | **Accepted.** `refund`/`reclaim` still release funds. Pull-based fees would fix it |
| M5 | Medium | `initialize` can be front-run if deploy and init are separate transactions; instance TTL can lapse when idle | **Open.** Initialize immediately after deploy and check `get_config`. A `__constructor` and a TTL keeper are future work |
| L2 | Low | Pending admin never expires, `propose_admin` emits no event | **Open** |
| L3 | Low | `Created` payments never expire (`TODO(wave: core-3)`) | **Open, planned** |
| L4 | Low | Floor-rounded fee is zero for dust amounts | **Accepted** (rounding favours the merchant, documented) |
| I1–I4 | Info | Stuck donations; assumes exact-transfer tokens (`core-1` allowlist must keep this); PII only if `reference`/`payment_id` are unkeyed hashes of low-entropy data; archived payment id reuse unverified | **Documented** |

Not yet done from the playbook's Phase 8: negative end-to-end on the contract side beyond the API's own negative e2e,
and a re-review of the fixes by a second reviewer.
