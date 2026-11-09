# Changelog

All notable changes are documented here. Format follows Keep a Changelog.

## [Unreleased]

- Initial implementation (stellar-pay-core). Not yet released or deployed.
- Security review fixes: `reclaim_delay` bounded to 1 hour–90 days (`InvalidDelay`), fee terms fixed at `create_payment`, oversized amounts rejected at create.
