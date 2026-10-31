# Events (schema v1)

Every event is published by the payments contract with two topics and one
data value:

- `topics[0]` = `Symbol("pay")`
- `topics[1]` = `Symbol(<name>)`
- `data` = a struct encoded as an `ScMap` (keys are field names, sorted), with
  `v: u32` = schema version.

Amounts are `i128` base units (JSON: decimal strings). Timestamps are ledger
seconds (`u64`). `payment_id` and `reference` are `BytesN<32>` (JSON: lowercase
hex). Addresses are strkeys (`G…` accounts or `C…` contracts).

Machine-checked XDR examples of every payment event are in
[`test-vectors.json`](./test-vectors.json); a test fails if that file drifts
from what the contract emits, and the API indexer's decoder tests consume the
same file.

## Payment lifecycle events

| name | data fields |
|---|---|
| `created` | `v, payment_id, operator, merchant, token, amount, reference, settle_mode (u32: 0 = auto, 1 = manual), created_at` |
| `paid` | `v, payment_id, payer, amount, fee_bps, paid_at` |
| `settled` | `v, payment_id, merchant, fee_recipient, merchant_amount, fee, settled_at` |
| `refunded` | `v, payment_id, payer, amount, reason (u32: 0 = operator refund, 1 = payer reclaim), refunded_at` |
| `cancelled` | `v, payment_id, cancelled_at` |

`Auto` payments emit `paid` then `settled` in the same transaction.

## Admin events

| name | data fields |
|---|---|
| `initialized` | `v, admin, token, fee_bps, fee_recipient, reclaim_delay` |
| `fee_changed` | `v, fee_bps, fee_recipient, config_version` |
| `paused` | `v, paused, config_version` |
| `admin_changed` | `v, previous_admin, admin, config_version` |

## JSON examples (decoded)

```json
{ "name": "created", "v": 1, "payment_id": "1111…1111", "operator": "C…", "merchant": "C…",
  "token": "C…", "amount": "1000000", "reference": "abab…abab", "settle_mode": "manual",
  "created_at": 1700000000 }
{ "name": "paid", "v": 1, "payment_id": "1111…1111", "payer": "C…", "amount": "1000000",
  "fee_bps": 100, "paid_at": 1700000030 }
{ "name": "settled", "v": 1, "payment_id": "1111…1111", "merchant": "C…", "fee_recipient": "C…",
  "merchant_amount": "990000", "fee": "10000", "settled_at": 1700000060 }
{ "name": "refunded", "v": 1, "payment_id": "2222…2222", "payer": "C…", "amount": "500",
  "reason": "operator", "refunded_at": 1700000060 }
{ "name": "cancelled", "v": 1, "payment_id": "4444…4444", "cancelled_at": 1700003660 }
```

(Full values are in `test-vectors.json`.)

## Compatibility policy

- Event names and topic layout are frozen for v1.
- Adding, removing or retyping a data field bumps `v`. Consumers must ignore
  events whose `v` they do not know (the API indexer logs and skips them).
- Field order in the encoding is by key name, so consumers must decode by key,
  never by position.
- TODO(wave: core-2): JSON Schema per event, extra fields (e.g. reference on
  `paid`), and a published backward-compatibility test suite.
