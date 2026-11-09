# Security policy

## Status

stellar-pay-core is **testnet-only** and has **not had a professional audit**. Do not use it with real funds.

| Version | Supported |
|---|---|
| 0.1.x | Yes (testnet) |

## Reporting a vulnerability

Please report privately through GitHub:
[Report a vulnerability](https://github.com/Stellar-Payment-Infrastructure/stellar-pay-core/security/advisories/new).
Do not open a public issue. Include the affected function, steps or a failing test, and the impact. We aim to
acknowledge reports within 3 business days.

Especially interesting: any way to move escrowed funds to an address other than the payment's merchant, fee
recipient or payer; to lock funds so that neither `settle`, `refund` nor `reclaim` can release them; or to bypass
authorization.

## Known issues

Accepted findings from the adversarial review (M1 payment-id squatting, M5 initialization front-running, L2 admin
proposals, and others) are listed with their mitigations in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md).
Dependency advisories are triaged in [docs/AUDIT_TRIAGE.md](docs/AUDIT_TRIAGE.md); CI runs `cargo audit` on every
push and weekly.
