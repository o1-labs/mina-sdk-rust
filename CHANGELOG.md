# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- The common API of the Rust, Go and JS SDKs: `spec/SPEC.md` and
  `spec/operations.graphql`. `tests/spec_tests.rs` checks that the query
  strings are the specification's documents and that the documents are valid
  against `schema/graphql_schema.json`.
- Methods of the common API that this SDK did not have: `get_daemon_metrics`,
  `get_genesis_block`, `get_block` (`BlockRef`), `get_pooled_zkapp_commands`,
  `get_transaction_status` (`TransactionRef`), `get_genesis_constants`,
  `get_tracked_accounts`, `get_snark_pool`, `get_fork_config`, `send_zkapp`
  and `unlock_account`.
- Signatures made outside the daemon: `Payment::signature` and
  `Delegation::signature` (`SignatureInput`).
- Result fields of the common API (the union of what the three SDKs
  returned): more `DaemonStatus`, `AccountData` (timing, permissions, zkApp
  state), `BlockInfo` (epoch data, ledger hashes, coinbase, fee transfers,
  user commands), `PooledUserCommand` and `SubmittedCommand` fields.
- Feature `itn`: `mina_sdk::itn::ItnClient` for the daemon's ITN GraphQL
  server (`--itn-graphql-port`), with ed25519 request signing (`ItnKey`),
  the `auth` handshake, sequence numbers and recovery from HTTP 412. It
  covers every field of `schema_itn`: `auth`, `slotsWon`, `internalLogs`,
  `flushInternalLogs`, `schedulePayments`, `scheduleZkappCommands`,
  `stopScheduledTransactions`, `updateGating`, `stopDaemon`,
  `zkAppCommandLimit`.
- `schema/itn_graphql_schema.json`, an introspection dump of the ITN schema,
  and an offline test of the ITN documents against it.
- Example `itn_internal_logs`.
- Error variants `ItnUnauthorized`, `ItnSequencing` and `InvalidItnKey`.
  Code that matches `Error` exhaustively must add them.

### Changed
- Every query is a named operation of the specification. Nullable variables
  are always sent, as null when omitted (`get_best_chain`, `get_account`).
- `get_account` uses one document with an optional `$token`;
  `queries::GET_ACCOUNT_WITH_TOKEN` is deprecated.
- `SendPaymentResult` and `SendDelegationResult` are aliases of the new
  `SubmittedCommand`. Code that builds these result types or `Payment` and
  `Delegation` with struct literals must add the new fields.
- The drift check sends a nullable variable without a sentinel as null, and
  has sentinels for `ID`, `UnlockInput` and `SendZkappInput`.

## [0.2.0-alpha.1] - 2026-04-18

### Added
- Constructor input validation: panics on zero retries or zero timeout
- HTTP status code check before parsing JSON response body
- Retry on HTTP 5xx errors (previously only retried on connection failures)
- Reverse multiply support: `3_u64 * currency` now works
- `ClientConfig::default()` test to assert public API contract
- CHANGELOG.md
- Badges in README (crates.io, docs.rs, CI, license, coverage)
- Integration test instructions in README
- Troubleshooting section in README
- Doc build with `-D warnings` in CI
- Coverage of `client_tests` in the coverage job (raised from ~18% to ~88%)

### Changed
- Slimmed `tokio` runtime features from `full` to `rt` + `time`
- Explicit type re-exports instead of glob `pub use types::*`
- Excluded non-essential files from crates.io package
- Switched coverage reporting from Codecov to Coveralls

### Fixed
- `GET_ACCOUNT` query uses correct `TokenId` type (was `UInt64`)
- Account query split into with/without token variants for schema compatibility
- Integration tests skip gracefully when env vars are empty strings

## [0.1.0] - 2025-11-20

### Added
- Initial release
- Async GraphQL client with configurable retry and timeout
- Query methods: sync_status, daemon_status, network_id, account, best_chain, peers, pooled_user_commands
- Mutation methods: send_payment, send_delegation, set_snark_worker, set_snark_work_fee
- `Currency` type with nanomina arithmetic, decimal parsing, and Display
- `execute_query()` for custom GraphQL queries
- `tracing` instrumentation
- Unit tests with wiremock
- Integration tests against live Mina daemon
- CI, integration, release, and schema-drift workflows
- Apache 2.0 license
