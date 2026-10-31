# Resource costs (baseline)

Baseline for Wave core-4 (resource-fee optimization). Stellar charges
**resource fees** (CPU instructions, memory, ledger entry reads/writes, bytes,
events, rent), not gas.

## How these numbers were produced

`tests/wasm_smoke.rs` loads the **release wasm** (`wasm32v1-none`, 38,134
bytes, soroban-sdk 22.0.11, rustc 1.99) into the Soroban host and reads
`env.cost_estimate().resources()` / `.fee()` after each call:

```bash
cargo build --target wasm32v1-none --release
cargo test --test wasm_smoke -- --ignored --nocapture
```

Caveats (from the soroban-sdk docs): the host metering does not model every
production cost (e.g. some XDR round trips, transaction size), and the fee
estimate uses the SDK's pubnet fee snapshot of 2024-12-11. Use RPC
simulation for exact numbers before submitting. Amount = 10,000,000 base
units, fee 100 bps, token = a Stellar Asset Contract.

| path | instructions | mem bytes | read entries | write entries | read bytes | write bytes | event bytes | est. fee (stroops) |
|---|---|---|---|---|---|---|---|---|
| create_payment | 4,400,977 | 2,013,883 | 3 | 2 | 38,668 | 724 | 496 | 3,313,831 |
| pay (manual, escrow) | 4,597,644 | 2,039,957 | 5 | 4 | 40,104 | 1,252 | 548 | 2,373,807 |
| settle | 4,754,351 | 2,063,985 | 5 | 5 | 40,184 | 1,484 | 872 | 2,846,120 |
| pay (auto-settle) | 4,793,410 | 2,063,769 | 4 | 5 | 40,460 | 1,484 | 1,184 | 1,759,318 |
| refund | 4,602,868 | 2,043,006 | 4 | 4 | 40,316 | 1,260 | 556 | 1,502,484 |
| cancel | 4,444,679 | 2,020,336 | 3 | 2 | 39,320 | 732 | 208 | 1,452,016 |

`create_payment`'s estimate includes rent for a brand-new persistent entry
(~30 days), which dominates its fee.

## Observations for optimization (core-4)

- Read bytes are dominated by the contract code entry (~38 KB wasm). Smaller
  wasm directly lowers every call.
- The `Payment` record stores `token` even though v1 has one token (it is kept
  for core-1 multi-token support).
- `Auto` mode skips escrow: two direct transfers instead of three.
- Zero-amount fee transfers are skipped.
