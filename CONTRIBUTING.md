# Contributing to stellar-pay-core

Thanks for helping. This contract moves money, so changes get careful review.

## Setup

- Rust stable with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli) if you deploy to testnet

Run the full check before opening a pull request (CI runs the same):

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --target wasm32-unknown-unknown --release
```

## Wave issues

Planned contributor issues are listed in [ROADMAP.md](ROADMAP.md) and labeled `wave` on GitHub. Ask to be assigned
on the issue before starting, keep to the issue's scope, and resolve its `TODO(wave: core-N)` marker. If the scope
turns out to need a breaking interface change, say so on the issue first.

## Code rules

- `#![no_std]`; no `unwrap()`/`expect()` in contract code; return a `#[contracterror]` variant instead.
- Amounts are `i128` in token base units with checked arithmetic. Never floats.
- Authorize the right `Address` with `require_auth()` before any state change.
- Write state before calling the token (checks, effects, interactions).
- Emit an event on every state change, and extend storage TTL on every persistent write.
- Error codes are stable: add new variants at the end, never renumber.
- No personal data on-chain: only ids, hashes, amounts, addresses and status.
- Every public function needs a happy-path test and at least one failure-path test. Prefer targeted auth mocks
  over `mock_all_auths()`, which can hide authorization bugs.

## Commits and pull requests

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat: add token allowlist`,
`fix: reject expired payments in pay`. Update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`, and update
[docs/SPEC.md](docs/SPEC.md) and [docs/EVENTS.md](docs/EVENTS.md) when the interface or events change.

Never commit secret keys or seed phrases. Use `stellar keys` identities, and testnet only.

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
