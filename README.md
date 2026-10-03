# Mina Rust SDK

[![CI](https://github.com/MinaProtocol/mina-sdk-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/MinaProtocol/mina-sdk-rust/actions/workflows/ci.yml)
[![Integration Tests](https://github.com/MinaProtocol/mina-sdk-rust/actions/workflows/integration.yml/badge.svg)](https://github.com/MinaProtocol/mina-sdk-rust/actions/workflows/integration.yml)
[![Coverage](https://coveralls.io/repos/github/MinaProtocol/mina-sdk-rust/badge.svg?branch=master)](https://coveralls.io/github/MinaProtocol/mina-sdk-rust?branch=master)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Rust SDK for interacting with [Mina Protocol](https://minaprotocol.com) nodes via GraphQL.

## Features

- **Async GraphQL client** — query node status, accounts, blocks; send payments and delegations
- Typed response structs with `Currency` arithmetic
- Automatic retry with configurable backoff
- Public `execute_query()` for custom GraphQL queries
- `tracing` instrumentation

## Requirements

- Rust 1.70+ (edition 2021)
- A running [Mina daemon](https://docs.minaprotocol.com/node-operators/getting-started) with GraphQL enabled

## Installation

```toml
[dependencies]
mina-sdk = "0.1"
```

## Quick Start

```rust
use mina_sdk::{MinaClient, Payment, Currency};

#[tokio::main]
async fn main() -> mina_sdk::Result<()> {
    let client = MinaClient::new("http://127.0.0.1:3085/graphql");

    // Check sync status
    let status = client.get_sync_status().await?;
    println!("Sync status: {status}");

    // Query an account
    let account = client.get_account("B62q...", None).await?;
    println!("Balance: {} MINA", account.balance.total);

    // Send a payment
    let result = client.send_payment(
        Payment::sender("B62qsender...")
            .to("B62qreceiver...")
            .amount(Currency::from_mina("1.5")?)
            .fee(Currency::from_mina("0.01")?)
            .memo("hello from SDK"),
    ).await?;
    println!("Tx hash: {}", result.hash);

    Ok(())
}
```

## Configuration

```rust
use mina_sdk::{MinaClient, ClientConfig};
use std::time::Duration;

let client = MinaClient::with_config(ClientConfig {
    graphql_uri: "http://127.0.0.1:3085/graphql".to_string(),
    retries: 3,
    retry_delay: Duration::from_secs(5),
    timeout: Duration::from_secs(30),
});
```

## API Reference

Full API documentation is available on [docs.rs](https://docs.rs/mina-sdk).

The Mina SDKs have the same API, defined in
[mina-sdk-spec](https://github.com/o1-labs/mina-sdk-spec). `spec/` is a copy
of it at the tag in `spec/VERSION`. A test checks that this SDK's queries,
including the ITN queries, are the specification's documents, and CI checks
that `spec/` is the tag's copy.

### Queries

| Method | Description |
|--------|-------------|
| `get_sync_status()` | Node sync status (Synced, Bootstrap, etc.) |
| `get_daemon_status()` | Daemon status: chain length, peers, addresses, block production keys |
| `get_daemon_metrics()` | Transaction and snark pool metrics, block production delay |
| `get_network_id()` | Network identifier |
| `get_account(public_key, token_id)` | Balance, nonce, delegate, timing, permissions, zkApp state |
| `get_best_chain(max_length)` | Recent blocks from the best chain |
| `get_genesis_block()` | The genesis block |
| `get_block(BlockRef)` | One block, by state hash or height |
| `get_peers()` | Connected peers |
| `get_pooled_user_commands(public_key)` | Pending payments and delegations |
| `get_pooled_zkapp_commands(public_key)` | Pending zkApp commands |
| `get_transaction_status(TransactionRef)` | `Pending`, `Included` or `Unknown` |
| `get_genesis_constants()` | Genesis timestamp, coinbase, account creation fee |
| `get_tracked_accounts()` | Accounts in the daemon's keystore |
| `get_snark_pool()` | Completed snark work |
| `get_fork_config()` | The daemon's fork configuration (JSON) |
| `execute_query(query, variables, name)` | Run a custom GraphQL query |

### Mutations

| Method | Description |
|--------|-------------|
| `send_payment(Payment)` | Send a payment; `Payment::signature` for one made outside the daemon |
| `send_delegation(Delegation)` | Delegate stake; `Delegation::signature` likewise |
| `send_zkapp(command)` | Send a signed zkApp command (JSON) |
| `unlock_account(public_key, password)` | Unlock a keystore account |
| `set_snark_worker(public_key)` | Set/unset SNARK worker |
| `set_snark_work_fee(fee)` | Set SNARK work fee |

### Currency

```rust
use mina_sdk::Currency;

let a = Currency::from_mina("10")?;              // 10 MINA
let b = Currency::from_mina("1.5")?;             // 1.5 MINA
let c = Currency::from_nanomina(1_000_000_000);   // 1 MINA
let d = Currency::from_graphql("1500000000")?;     // from GraphQL response

println!("{}", (a + b));        // 11.500000000
println!("{}", a.nanomina());   // 10000000000
assert!(a > b);
```

### ITN server (feature `itn`)

A daemon started with `ITN_FEATURES=1`, `--itn-graphql-port` and `--itn-keys`
serves a second GraphQL API, which load testing tools use. `ItnClient` signs
each request with an ed25519 `ItnKey` whose public half must be in
`--itn-keys`, and it handles the daemon's sequence numbers (a new `auth` after
a daemon restart, HTTP 412).

```toml
mina-sdk = { version = "0.2", features = ["itn"] }
```

```rust
use mina_sdk::itn::{ItnClient, ItnKey, PaymentsDetails};

let key = ItnKey::from_base64(&std::fs::read_to_string("itn_sk")?)?;
println!("start the daemon with --itn-keys {}", key.public_key_base64());

let itn = ItnClient::new("http://127.0.0.1:3086/graphql", key);
let logs = itn.internal_logs(0).await?;
let handle = itn.schedule_payments(&PaymentsDetails { /* ... */ }).await?;
itn.stop_scheduled_transactions(&handle).await?;
```

| Method | GraphQL |
|--------|---------|
| `auth()` | `auth` (server UUID, sequence number, peer ID, block producer) |
| `slots_won()` | `slotsWon` |
| `internal_logs(start)` / `flush_internal_logs(end)` | `internalLogs` / `flushInternalLogs` |
| `schedule_payments(&PaymentsDetails)` | `schedulePayments` |
| `schedule_zkapp_commands(&ZkappCommandsDetails)` | `scheduleZkappCommands` |
| `stop_scheduled_transactions(handle)` | `stopScheduledTransactions` |
| `update_gating(&GatingUpdate)` | `updateGating` |
| `stop_daemon(delay, clean)` | `stopDaemon` |
| `set_zkapp_command_limit(limit)` | `zkAppCommandLimit` |
| `execute_query(query, vars, name)` | any document, sequenced and signed |

These need a daemon with MinaProtocol/mina#19616; older daemons answer them
with a GraphQL error:

| Method | GraphQL |
|--------|---------|
| `commit_id()` | `auth { commitId }`: the daemon's git commit |
| `scheduled_transactions()` | `scheduledTransactions`: handles of the running schedulers |
| `schedule_payments_with_handle(&PaymentsDetails, handle)` | `schedulePayments` with a caller-chosen handle |
| `schedule_zkapp_commands_with_handle(&ZkappCommandsDetails, handle)` | `scheduleZkappCommands` with a caller-chosen handle |
| `create_accounts(&CreateAccountsDetails, handle)` | `createAccounts`: keys at once, funding in the background under the handle |

A handle is a UUID that the caller chooses and records before the call. A
call with the handle of a running scheduler starts nothing and returns that
handle, so these calls may be repeated after a transport error.

A sequenced request is never repeated after a transport error, because the
daemon may already have run it. The documents in `mina_sdk::itn::queries`
are those of `spec/itn-operations.graphql`, which mina-sdk-spec validates
against the daemon's ITN schema.

## Examples

Runnable programs live in [`examples/`](examples/):

| Example | Command | Needs a node |
|---------|---------|--------------|
| `basic_usage` | `cargo run --example basic_usage` | yes |
| `send_payment` | `cargo run --example send_payment` | yes |
| `stake_delegation` | `cargo run --example stake_delegation` | yes |
| `node_monitoring` | `cargo run --example node_monitoring` | yes |
| `custom_query` | `cargo run --example custom_query` | yes |
| `error_handling` | `cargo run --example error_handling` | yes |
| `currency_operations` | `cargo run --example currency_operations` | no |
| `itn_internal_logs` | `cargo run --example itn_internal_logs --features itn` | yes, with ITN |

## Development

```bash
git clone https://github.com/MinaProtocol/mina-sdk-rust.git
cd mina-sdk-rust
cargo test
cargo clippy
```

### Integration tests

Integration tests run against a live Mina node and are skipped by default.
To run them locally with a [lightnet](https://docs.minaprotocol.com/zkapps/writing-a-zkapp/introduction-to-zkapps/testing-zkapps-lightnet) Docker container:

```bash
docker run --rm -d -p 8080:8080 -p 8181:8181 -p 3085:3085 \
  -e NETWORK_TYPE=single-node -e PROOF_LEVEL=none \
  o1labs/mina-local-network:compatible-latest-lightnet

# Wait for the network to sync, then:
MINA_GRAPHQL_URI=http://127.0.0.1:8080/graphql \
  cargo test --test integration_tests -- --test-threads=1
```

The ITN integration tests need a daemon with `ITN_FEATURES=1`,
`--itn-graphql-port 3086`, `--itn-keys <public key>` and, for non-empty
internal logs, `--internal-tracing`:

```bash
MINA_ITN_URI=http://127.0.0.1:3086/graphql MINA_ITN_KEY=<base64 seed> \
  cargo test --features itn --test itn_integration_tests -- --test-threads=1
```

## Troubleshooting

**Connection refused** — Make sure the Mina daemon is running and the GraphQL endpoint is accessible. The default URI is `http://127.0.0.1:3085/graphql`.

**Account not found** — The account may not exist on the network, or the public key format is incorrect. Mina public keys start with `B62q`.

**Schema drift** — If queries fail with unexpected GraphQL errors, the daemon version may have changed its schema. Check the documents against your node with mina-sdk-spec: `python3 scripts/check.py --endpoint http://your-node:3085/graphql` (in a clone of [mina-sdk-spec](https://github.com/o1-labs/mina-sdk-spec)).

**Timeout errors** — Increase the timeout and retry settings via `ClientConfig`. Some queries (like `get_best_chain`) can be slow on nodes that are still syncing.

## Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

## License

[Apache License 2.0](LICENSE)
