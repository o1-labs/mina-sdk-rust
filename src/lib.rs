//! Rust SDK for interacting with [Mina Protocol](https://minaprotocol.com) nodes via GraphQL.
//!
//! Mina is a lightweight blockchain (about 22 KB) powered by recursive zero-knowledge proofs.
//! This crate provides a typed, async client for the Mina daemon's GraphQL API — covering
//! node status, account queries, block exploration, payments, stake delegation, and SNARK
//! worker configuration.
//!
//! # Features
//!
//! - **Async/await** — built on [tokio](https://tokio.rs) and [reqwest](https://docs.rs/reqwest)
//! - **Typed responses** — every query returns a strongly-typed Rust struct
//! - **Automatic retry** — configurable retry count and delay for transient failures
//! - **Currency safety** — [`Currency`] type with nanomina precision and overflow-safe arithmetic
//! - **Extensible** — [`MinaClient::execute_query`] runs arbitrary GraphQL through the same
//!   retry-aware client; [`queries`] module exposes all built-in query strings
//! - **Tracing** — all requests are instrumented with [`tracing`](https://docs.rs/tracing)
//!
//! # Quick Start
//!
//! ```no_run
//! # async fn example() -> mina_sdk::Result<()> {
//! use mina_sdk::{MinaClient, Payment, Currency};
//!
//! let client = MinaClient::new("http://127.0.0.1:3085/graphql");
//!
//! // Query node status
//! let status = client.get_sync_status().await?;
//! println!("Node is {status}");
//!
//! // Send a payment
//! let result = client.send_payment(
//!     Payment::sender("B62q..sender..")
//!         .to("B62q..receiver..")
//!         .amount(Currency::from_mina("1.5")?)
//!         .fee(Currency::from_mina("0.01")?)
//!         .memo("hello"),
//! ).await?;
//! println!("Payment hash: {}", result.hash);
//! # Ok(())
//! # }
//! ```
//!
//! # Custom Configuration
//!
//! ```no_run
//! use mina_sdk::{ClientConfig, MinaClient};
//! use std::time::Duration;
//!
//! let client = MinaClient::with_config(ClientConfig {
//!     graphql_uri: "http://my-node:3085/graphql".to_string(),
//!     retries: 5,
//!     retry_delay: Duration::from_secs(10),
//!     timeout: Duration::from_secs(60),
//! });
//! ```
//!
//! # Currency
//!
//! The [`Currency`] type represents MINA amounts stored internally as nanomina (1 MINA = 10^9
//! nanomina). It provides safe conversions and arithmetic:
//!
//! ```
//! use mina_sdk::Currency;
//!
//! let amount = Currency::from_mina("1.5").unwrap();
//! assert_eq!(amount.nanomina(), 1_500_000_000);
//! assert_eq!(amount.mina(), "1.500000000");
//!
//! let fee = Currency::from_mina("0.01").unwrap();
//! let total = amount + fee;
//! assert_eq!(total, Currency::from_mina("1.51").unwrap());
//!
//! // Overflow-safe variants
//! assert!(fee.checked_add(amount).is_some());
//! assert!(amount.checked_sub(fee).is_ok());
//! ```
//!
//! # Error Handling
//!
//! All fallible operations return [`Result<T>`](Result), which uses [`Error`]. Match on
//! variants for fine-grained control:
//!
//! ```no_run
//! # async fn example() -> mina_sdk::Result<()> {
//! use mina_sdk::{MinaClient, Error};
//!
//! let client = MinaClient::new("http://127.0.0.1:3085/graphql");
//! match client.get_account("B62q...", None).await {
//!     Ok(account) => println!("Balance: {}", account.balance.total),
//!     Err(Error::AccountNotFound(key)) => eprintln!("No such account: {key}"),
//!     Err(Error::Connection { attempts, .. }) => eprintln!("Node unreachable after {attempts} tries"),
//!     Err(e) => eprintln!("Error: {e}"),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # API Overview
//!
//! | Method | Description |
//! |--------|-------------|
//! | [`MinaClient::get_sync_status`] | Node sync state (Synced, Bootstrap, etc.) |
//! | [`MinaClient::get_daemon_status`] | Comprehensive daemon info |
//! | [`MinaClient::get_network_id`] | Network identifier string |
//! | [`MinaClient::get_account`] | Account balance, nonce, delegate |
//! | [`MinaClient::get_best_chain`] | Recent blocks from the best chain |
//! | [`MinaClient::get_peers`] | Connected peer list |
//! | [`MinaClient::get_pooled_user_commands`] | Pending transaction pool |
//! | [`MinaClient::send_payment`] | Send a MINA payment |
//! | [`MinaClient::send_delegation`] | Delegate stake to a validator |
//! | [`MinaClient::set_snark_worker`] | Enable/disable SNARK worker |
//! | [`MinaClient::set_snark_work_fee`] | Set SNARK work fee |
//! | [`MinaClient::execute_query`] | Run arbitrary GraphQL |
//!
//! # ITN server (feature `itn`)
//!
//! With the `itn` feature, [`itn::ItnClient`] talks to the daemon's second
//! GraphQL server (`--itn-graphql-port`, `ITN_FEATURES=1`), which load testing
//! tools use to schedule transactions, read internal logs, change connection
//! gating and stop the daemon. Its requests are signed with an ed25519
//! [`itn::ItnKey`]; see the [`itn`] module for the protocol.
//!
//! # Examples
//!
//! See the [`examples/`](https://github.com/MinaProtocol/mina-sdk-rust/tree/master/examples)
//! directory for runnable programs:
//!
//! - **basic_usage** — connect, query status, browse blocks and accounts
//! - **send_payment** — submit a payment transaction
//! - **stake_delegation** — delegate stake to a validator
//! - **currency_operations** — create, convert, and do arithmetic on amounts (no node needed)
//! - **custom_query** — run arbitrary GraphQL through `execute_query`
//! - **error_handling** — match on specific error variants
//! - **node_monitoring** — health checks, peer listing, block browsing

mod client;
mod currency;
pub mod error;
#[cfg(feature = "itn")]
#[cfg_attr(docsrs, doc(cfg(feature = "itn")))]
pub mod itn;
pub mod queries;
mod types;

pub use client::{ClientConfig, MinaClient, QueryBuilder};
pub use currency::Currency;
pub use error::{Error, Result};
pub use types::{
    AccountBalance, AccountData, AccountPermissions, AccountTiming, AddrsAndPorts, BlockInfo,
    BlockRef, BlockTransaction, CompletedWork, DaemonMetrics, DaemonStatus, Delegation, EpochData,
    FeeTransfer, GenesisConstants, Payment, PeerInfo, PooledUserCommand, SendDelegationResult,
    SendPaymentResult, SignatureInput, SubmittedCommand, SyncStatus, TrackedAccount,
    TransactionRef, TransactionStatus, ZkappCommandResult, ZkappFailure, ZkappFeePayer,
};
