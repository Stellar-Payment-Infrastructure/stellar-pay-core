# Changelog

All notable changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - not yet tagged

First release, testnet only. Not professionally audited.

### Added

- Payment lifecycle contract: `create_payment`, `pay` (Auto or Manual settlement), `settle`, `cancel`, `refund`,
  and payer `reclaim` after `reclaim_delay`.
- Fees in basis points, capped at 500, floor rounding, fixed when the payment is created.
- Admin controls: `set_fee`, `set_paused` (blocks only `create_payment` and `pay`), two-step admin transfer.
- Storage TTL extension on writes and a permissionless `bump_ttl`.
- Versioned events for every state change, documented in `docs/EVENTS.md` with XDR test vectors.
- Testnet deployment record (`deployments/testnet.json`) and smoke test transcript (`docs/SMOKE_TEST.md`).

### Security

- Adversarial review fixes: `reclaim_delay` bounded to 1 hour–90 days (`InvalidDelay`), fee terms fixed at
  `create_payment`, amounts whose fee split could overflow rejected at create.
- Accepted for this version, with mitigations: M1 (payment-id squatting), M5 (initialization front-running),
  L2 (admin proposals). See `docs/SECURITY_REVIEW.md`.
