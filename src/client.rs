use std::time::Duration;

use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::error::{Error, GraphqlErrorEntry, Result};
use crate::types::*;
use crate::{queries, Currency};

/// Configuration for the Mina daemon client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The daemon's GraphQL endpoint URL.
    pub graphql_uri: String,
    /// Number of retry attempts for failed requests.
    pub retries: u32,
    /// Duration to wait between retries.
    pub retry_delay: Duration,
    /// HTTP request timeout.
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            graphql_uri: "http://127.0.0.1:3085/graphql".to_string(),
            retries: 3,
            retry_delay: Duration::from_secs(5),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Client for interacting with a Mina daemon via its GraphQL API.
///
/// # Examples
/// ```no_run
/// # async fn example() -> mina_sdk::Result<()> {
/// use mina_sdk::MinaClient;
///
/// let client = MinaClient::new("http://127.0.0.1:3085/graphql");
/// let status = client.get_sync_status().await?;
/// println!("Sync status: {status}");
/// # Ok(())
/// # }
/// ```
pub struct MinaClient {
    config: ClientConfig,
    http: reqwest::Client,
}

impl Default for MinaClient {
    /// Create a client connected to the default local daemon
    /// (`http://127.0.0.1:3085/graphql`) with default retry/timeout settings.
    fn default() -> Self {
        Self::with_config(ClientConfig::default())
    }
}

impl MinaClient {
    /// Create a new client with default settings.
    pub fn new(graphql_uri: &str) -> Self {
        Self::with_config(ClientConfig {
            graphql_uri: graphql_uri.to_string(),
            ..Default::default()
        })
    }

    /// Create a client targeting `http://{host}:{port}/graphql`.
    ///
    /// ```
    /// use mina_sdk::MinaClient;
    /// let client = MinaClient::from_host_and_port("127.0.0.1", 3085);
    /// ```
    pub fn from_host_and_port(host: &str, port: u16) -> Self {
        Self::new(&format!("http://{host}:{port}/graphql"))
    }

    /// Create a new client with custom configuration.
    ///
    /// # Panics
    ///
    /// Panics if `retries` is 0, `retry_delay` is negative, or `timeout` is zero.
    pub fn with_config(config: ClientConfig) -> Self {
        assert!(config.retries >= 1, "retries must be at least 1");
        assert!(
            !config.timeout.is_zero(),
            "timeout must be greater than zero"
        );
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("failed to build HTTP client");
        Self { config, http }
    }

    /// Start building a custom GraphQL query with named options.
    ///
    /// Preferred entry point for arbitrary queries — variables and name are
    /// opt-in via the builder, so callers don't have to spell out `None`:
    ///
    /// ```no_run
    /// # async fn example() -> mina_sdk::Result<()> {
    /// use mina_sdk::MinaClient;
    /// use serde_json::json;
    ///
    /// let client = MinaClient::default();
    ///
    /// let data = client.query("query { version }").send().await?;
    /// println!("{}", data["version"]);
    ///
    /// let data = client
    ///     .query("query ($len: Int) { bestChain(maxLength: $len) { stateHash } }")
    ///     .variables(json!({ "len": 3 }))
    ///     .name("best_chain_custom")
    ///     .send()
    ///     .await?;
    /// # let _ = data;
    /// # Ok(())
    /// # }
    /// ```
    pub fn query<'a>(&'a self, query: &'a str) -> QueryBuilder<'a> {
        QueryBuilder {
            client: self,
            query,
            variables: None,
            name: None,
        }
    }

    /// Execute a raw GraphQL query and return the `data` field of the response.
    ///
    /// This low-level method stays public so downstream crates
    /// (e.g. mina-perf-testing) can reuse the client's retry logic. For new
    /// code prefer [`MinaClient::query`], which exposes a builder.
    pub async fn execute_query(
        &self,
        query: &str,
        variables: Option<Value>,
        query_name: &str,
    ) -> Result<Value> {
        let mut payload = json!({ "query": query });
        if let Some(vars) = variables {
            payload["variables"] = vars;
        }

        let mut last_err: Option<reqwest::Error> = None;

        for attempt in 1..=self.config.retries {
            debug!(
                query_name,
                attempt,
                max = self.config.retries,
                "GraphQL request"
            );

            match self
                .http
                .post(&self.config.graphql_uri)
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        warn!(query_name, attempt, %status, "HTTP error");
                        last_err = Some(resp.error_for_status().unwrap_err());
                        if attempt < self.config.retries {
                            tokio::time::sleep(self.config.retry_delay).await;
                        }
                        continue;
                    }
                    match resp.json::<Value>().await {
                        Ok(body) => return graphql_data(body, query_name),
                        Err(e) => {
                            warn!(query_name, attempt, error = %e, "failed to parse response");
                            last_err = Some(e);
                        }
                    }
                }
                Err(e) => {
                    warn!(query_name, attempt, error = %e, "connection error");
                    last_err = Some(e);
                }
            }

            if attempt < self.config.retries {
                tokio::time::sleep(self.config.retry_delay).await;
            }
        }

        Err(Error::Connection {
            query_name: query_name.to_string(),
            attempts: self.config.retries,
            source: last_err.expect("at least one attempt must have been made"),
        })
    }

    /// Get the GraphQL endpoint URI.
    pub fn graphql_uri(&self) -> &str {
        &self.config.graphql_uri
    }

    // -- Queries --

    /// Get the node's sync status.
    pub async fn get_sync_status(&self) -> Result<SyncStatus> {
        let data = self
            .execute_query(queries::SYNC_STATUS, None, "get_sync_status")
            .await?;
        let s = data["syncStatus"]
            .as_str()
            .ok_or_else(|| Error::MissingField {
                query_name: "get_sync_status".into(),
                field: "syncStatus".into(),
            })?;
        serde_json::from_value(Value::String(s.to_string())).map_err(|_| Error::MissingField {
            query_name: "get_sync_status".into(),
            field: "syncStatus".into(),
        })
    }

    /// Get comprehensive daemon status.
    pub async fn get_daemon_status(&self) -> Result<DaemonStatus> {
        const NAME: &str = "get_daemon_status";
        let data = self
            .execute_query(queries::DAEMON_STATUS, None, NAME)
            .await?;
        let status = &data["daemonStatus"];

        let sync_status: SyncStatus =
            serde_json::from_value(status.get("syncStatus").cloned().unwrap_or(Value::Null))
                .map_err(|_| missing(NAME, "syncStatus"))?;

        let peers = status
            .get("peers")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().map(parse_peer).collect());
        let addrs = &status["addrsAndPorts"];

        Ok(DaemonStatus {
            sync_status,
            blockchain_length: status["blockchainLength"].as_i64(),
            highest_block_length_received: status["highestBlockLengthReceived"].as_i64(),
            highest_unvalidated_block_length_received: status
                ["highestUnvalidatedBlockLengthReceived"]
                .as_i64(),
            uptime_secs: status["uptimeSecs"].as_i64(),
            state_hash: opt_str(&status["stateHash"]),
            commit_id: opt_str(&status["commitId"]),
            num_accounts: status["numAccounts"].as_i64(),
            ledger_merkle_root: opt_str(&status["ledgerMerkleRoot"]),
            chain_id: opt_str(&status["chainId"]),
            catchup_status: status["catchupStatus"].as_array().map(|a| strings(a)),
            block_production_keys: status["blockProductionKeys"]
                .as_array()
                .map(|a| strings(a))
                .unwrap_or_default(),
            coinbase_receiver: opt_str(&status["coinbaseReceiver"]),
            peers,
            addrs_and_ports: addrs.is_object().then(|| AddrsAndPorts {
                external_ip: str_or_empty(&addrs["externalIp"]),
                bind_ip: str_or_empty(&addrs["bindIp"]),
                client_port: addrs["clientPort"].as_i64().unwrap_or_default(),
                libp2p_port: addrs["libp2pPort"].as_i64().unwrap_or_default(),
            }),
        })
    }

    /// Get the daemon's metrics: transaction and snark pools, block
    /// production delay.
    pub async fn get_daemon_metrics(&self) -> Result<DaemonMetrics> {
        const NAME: &str = "get_daemon_metrics";
        let data = self
            .execute_query(queries::DAEMON_METRICS, None, NAME)
            .await?;
        let m = &data["daemonStatus"]["metrics"];
        if !m.is_object() {
            return Err(missing(NAME, "daemonStatus.metrics"));
        }
        let n = |field: &str| parse_i64(&m[field]);
        Ok(DaemonMetrics {
            block_production_delay: m["blockProductionDelay"]
                .as_array()
                .map(|a| a.iter().map(parse_i64).collect())
                .unwrap_or_default(),
            transaction_pool_diff_received: n("transactionPoolDiffReceived"),
            transaction_pool_diff_broadcasted: n("transactionPoolDiffBroadcasted"),
            transactions_added_to_pool: n("transactionsAddedToPool"),
            transaction_pool_size: n("transactionPoolSize"),
            snark_pool_diff_received: n("snarkPoolDiffReceived"),
            snark_pool_diff_broadcasted: n("snarkPoolDiffBroadcasted"),
            pending_snark_work: n("pendingSnarkWork"),
            snark_pool_size: n("snarkPoolSize"),
        })
    }

    /// Get the network identifier.
    pub async fn get_network_id(&self) -> Result<String> {
        let data = self
            .execute_query(queries::NETWORK_ID, None, "get_network_id")
            .await?;
        data["networkID"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| missing("get_network_id", "networkID"))
    }

    /// Get account data for a public key; `token_id` is `None` for the
    /// default MINA token.
    pub async fn get_account(
        &self,
        public_key: &str,
        token_id: Option<&str>,
    ) -> Result<AccountData> {
        let vars = json!({ "publicKey": public_key, "token": token_id });
        let data = self
            .execute_query(queries::GET_ACCOUNT, Some(vars), "get_account")
            .await?;

        let acc = data
            .get("account")
            .filter(|v| !v.is_null())
            .ok_or_else(|| Error::AccountNotFound(public_key.to_string()))?;

        let balance = &acc["balance"];
        let timing = &acc["timing"];
        let permissions = &acc["permissions"];

        Ok(AccountData {
            public_key: str_or_empty(&acc["publicKey"]),
            nonce: parse_u64(&acc["nonce"]),
            delegate: opt_str(&acc["delegate"]),
            token_id: opt_str(&acc["tokenId"]),
            balance: AccountBalance {
                total: Currency::from_graphql(balance["total"].as_str().unwrap_or("0"))?,
                liquid: opt_currency(&balance["liquid"])?,
                locked: opt_currency(&balance["locked"])?,
                block_height: opt_u64(&balance["blockHeight"]),
            },
            token_symbol: opt_str(&acc["tokenSymbol"]),
            voting_for: opt_str(&acc["votingFor"]),
            receipt_chain_hash: opt_str(&acc["receiptChainHash"]),
            timing: if timing.is_object()
                && !timing.as_object().unwrap().values().all(Value::is_null)
            {
                Some(AccountTiming {
                    initial_minimum_balance: opt_currency(&timing["initialMinimumBalance"])?,
                    cliff_time: opt_u64(&timing["cliffTime"]),
                    cliff_amount: opt_currency(&timing["cliffAmount"])?,
                    vesting_period: opt_u64(&timing["vestingPeriod"]),
                    vesting_increment: opt_currency(&timing["vestingIncrement"])?,
                })
            } else {
                None
            },
            permissions: permissions.is_object().then(|| {
                let p = |f: &str| opt_str(&permissions[f]);
                let vk = &permissions["setVerificationKey"];
                AccountPermissions {
                    edit_state: p("editState"),
                    send: p("send"),
                    receive: p("receive"),
                    access: p("access"),
                    set_delegate: p("setDelegate"),
                    set_permissions: p("setPermissions"),
                    set_verification_key: vk
                        .is_object()
                        .then(|| (str_or_empty(&vk["auth"]), str_or_empty(&vk["txnVersion"]))),
                    set_zkapp_uri: p("setZkappUri"),
                    edit_action_state: p("editActionState"),
                    set_token_symbol: p("setTokenSymbol"),
                    increment_nonce: p("incrementNonce"),
                    set_voting_for: p("setVotingFor"),
                    set_timing: p("setTiming"),
                }
            }),
            zkapp_state: acc["zkappState"].as_array().map(|a| strings(a)),
            proved_state: acc["provedState"].as_bool(),
            zkapp_uri: opt_str(&acc["zkappUri"]),
        })
    }

    /// Get blocks from the best chain.
    pub async fn get_best_chain(&self, max_length: Option<u32>) -> Result<Vec<BlockInfo>> {
        let vars = json!({ "maxLength": max_length });
        let data = self
            .execute_query(queries::BEST_CHAIN, Some(vars), "get_best_chain")
            .await?;
        match data.get("bestChain").and_then(|c| c.as_array()) {
            Some(arr) => arr.iter().map(parse_block).collect(),
            None => Ok(vec![]),
        }
    }

    /// Get the network's genesis block.
    pub async fn get_genesis_block(&self) -> Result<BlockInfo> {
        let data = self
            .execute_query(queries::GENESIS_BLOCK, None, "get_genesis_block")
            .await?;
        let block = &data["genesisBlock"];
        if !block.is_object() {
            return Err(missing("get_genesis_block", "genesisBlock"));
        }
        parse_block(block)
    }

    /// Get one block, by state hash or by height.
    pub async fn get_block(&self, block: BlockRef) -> Result<BlockInfo> {
        let vars = match &block {
            BlockRef::StateHash(hash) => json!({ "stateHash": hash, "height": null }),
            BlockRef::Height(height) => json!({ "stateHash": null, "height": height }),
        };
        let data = self
            .execute_query(queries::BLOCK, Some(vars), "get_block")
            .await?;
        let found = &data["block"];
        if !found.is_object() {
            return Err(missing("get_block", "block"));
        }
        parse_block(found)
    }

    /// Get the list of connected peers.
    pub async fn get_peers(&self) -> Result<Vec<PeerInfo>> {
        let data = self
            .execute_query(queries::GET_PEERS, None, "get_peers")
            .await?;
        Ok(data
            .get("getPeers")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().map(parse_peer).collect())
            .unwrap_or_default())
    }

    /// Get pending user commands from the transaction pool; `None` for every
    /// sender.
    pub async fn get_pooled_user_commands(
        &self,
        public_key: Option<&str>,
    ) -> Result<Vec<PooledUserCommand>> {
        let vars = json!({ "publicKey": public_key });
        let data = self
            .execute_query(
                queries::POOLED_USER_COMMANDS,
                Some(vars),
                "get_pooled_user_commands",
            )
            .await?;
        Ok(data
            .get("pooledUserCommands")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|c| PooledUserCommand {
                        id: str_or_empty(&c["id"]),
                        hash: str_or_empty(&c["hash"]),
                        kind: str_or_empty(&c["kind"]),
                        nonce: scalar_string(&c["nonce"]),
                        amount: scalar_string(&c["amount"]),
                        fee: scalar_string(&c["fee"]),
                        from: str_or_empty(&c["from"]),
                        to: str_or_empty(&c["to"]),
                        source: str_or_empty(&c["source"]["publicKey"]),
                        receiver: str_or_empty(&c["receiver"]["publicKey"]),
                        memo: str_or_empty(&c["memo"]),
                        failure_reason: opt_str(&c["failureReason"]),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get pending zkApp commands from the transaction pool; `None` for every
    /// fee payer.
    pub async fn get_pooled_zkapp_commands(
        &self,
        public_key: Option<&str>,
    ) -> Result<Vec<ZkappCommandResult>> {
        let vars = json!({ "publicKey": public_key });
        let data = self
            .execute_query(
                queries::POOLED_ZKAPP_COMMANDS,
                Some(vars),
                "get_pooled_zkapp_commands",
            )
            .await?;
        data.get("pooledZkappCommands")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().map(parse_zkapp_result).collect())
            .unwrap_or(Ok(vec![]))
    }

    /// Get the status of a payment, delegation or zkApp command.
    pub async fn get_transaction_status(
        &self,
        transaction: TransactionRef,
    ) -> Result<TransactionStatus> {
        const NAME: &str = "get_transaction_status";
        let vars = match &transaction {
            TransactionRef::Payment(id) => json!({ "payment": id, "zkappTransaction": null }),
            TransactionRef::Zkapp(id) => json!({ "payment": null, "zkappTransaction": id }),
        };
        let data = self
            .execute_query(queries::TRANSACTION_STATUS, Some(vars), NAME)
            .await?;
        serde_json::from_value(data["transactionStatus"].clone())
            .map_err(|_| missing(NAME, "transactionStatus"))
    }

    /// Get the network's genesis constants.
    pub async fn get_genesis_constants(&self) -> Result<GenesisConstants> {
        const NAME: &str = "get_genesis_constants";
        let data = self
            .execute_query(queries::GENESIS_CONSTANTS, None, NAME)
            .await?;
        let c = &data["genesisConstants"];
        Ok(GenesisConstants {
            genesis_timestamp: c["genesisTimestamp"]
                .as_str()
                .ok_or_else(|| missing(NAME, "genesisConstants.genesisTimestamp"))?
                .to_string(),
            coinbase: currency(&c["coinbase"])?,
            account_creation_fee: currency(&c["accountCreationFee"])?,
        })
    }

    /// Get the accounts the daemon tracks (its wallet keys).
    pub async fn get_tracked_accounts(&self) -> Result<Vec<TrackedAccount>> {
        let data = self
            .execute_query(queries::TRACKED_ACCOUNTS, None, "get_tracked_accounts")
            .await?;
        data.get("trackedAccounts")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|a| {
                        Ok(TrackedAccount {
                            public_key: str_or_empty(&a["publicKey"]),
                            balance: currency(&a["balance"]["total"])?,
                        })
                    })
                    .collect()
            })
            .unwrap_or(Ok(vec![]))
    }

    /// Get the completed snark work in the snark pool.
    pub async fn get_snark_pool(&self) -> Result<Vec<CompletedWork>> {
        let data = self
            .execute_query(queries::SNARK_POOL, None, "get_snark_pool")
            .await?;
        data.get("snarkPool")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|w| {
                        Ok(CompletedWork {
                            prover: str_or_empty(&w["prover"]),
                            fee: currency(&w["fee"])?,
                            work_ids: w["workIds"]
                                .as_array()
                                .map(|ids| ids.iter().map(parse_i64).collect())
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or(Ok(vec![]))
    }

    /// Get the daemon's fork configuration, a JSON value.
    pub async fn get_fork_config(&self) -> Result<Value> {
        let data = self
            .execute_query(queries::FORK_CONFIG, None, "get_fork_config")
            .await?;
        data.get("fork_config")
            .cloned()
            .ok_or_else(|| missing("get_fork_config", "fork_config"))
    }

    // -- Mutations --

    /// Send a payment transaction.
    ///
    /// Without [`Payment::signature`], the daemon signs with the sender's key
    /// from its keystore, which must be unlocked ([`MinaClient::unlock_account`]).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(client: &mina_sdk::MinaClient) -> mina_sdk::Result<()> {
    /// use mina_sdk::{Payment, Currency};
    ///
    /// let result = client.send_payment(
    ///     Payment::sender("B62qsender...")
    ///         .to("B62qreceiver...")
    ///         .amount(Currency::from_mina("1.5")?)
    ///         .fee(Currency::from_mina("0.01")?)
    ///         .memo("coffee"),
    /// ).await?;
    /// println!("Tx hash: {}", result.hash);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_payment(&self, payment: Payment) -> Result<SendPaymentResult> {
        let mut input = json!({
            "from": payment.sender,
            "to": payment.receiver,
            "amount": payment.amount.to_nanomina_str(),
            "fee": payment.fee.to_nanomina_str(),
        });
        if let Some(m) = &payment.memo {
            input["memo"] = Value::String(m.clone());
        }
        if let Some(n) = payment.nonce {
            input["nonce"] = Value::String(n.to_string());
        }
        let vars = json!({ "input": input, "signature": signature_json(&payment.signature) });
        let data = self
            .execute_query(queries::SEND_PAYMENT, Some(vars), "send_payment")
            .await?;
        parse_submitted(&data["sendPayment"]["payment"])
    }

    /// Send a stake delegation transaction.
    ///
    /// Without [`Delegation::signature`], the daemon signs with the sender's
    /// key from its keystore, which must be unlocked.
    pub async fn send_delegation(&self, delegation: Delegation) -> Result<SendDelegationResult> {
        let mut input = json!({
            "from": delegation.sender,
            "to": delegation.delegate_to,
            "fee": delegation.fee.to_nanomina_str(),
        });
        if let Some(m) = &delegation.memo {
            input["memo"] = Value::String(m.clone());
        }
        if let Some(n) = delegation.nonce {
            input["nonce"] = Value::String(n.to_string());
        }
        let vars = json!({ "input": input, "signature": signature_json(&delegation.signature) });
        let data = self
            .execute_query(queries::SEND_DELEGATION, Some(vars), "send_delegation")
            .await?;
        parse_submitted(&data["sendDelegation"]["delegation"])
    }

    /// Send a signed zkApp command, as JSON in the daemon's
    /// `ZkappCommandInput` form (for example from o1js `toJSON()`).
    pub async fn send_zkapp(&self, zkapp_command: Value) -> Result<ZkappCommandResult> {
        let vars = json!({ "input": { "zkappCommand": zkapp_command } });
        let data = self
            .execute_query(queries::SEND_ZKAPP, Some(vars), "send_zkapp")
            .await?;
        parse_zkapp_result(&data["sendZkapp"]["zkapp"])
    }

    /// Unlock an account in the daemon's keystore, so that the daemon can
    /// sign payments and delegations from it. Returns its public key.
    pub async fn unlock_account(&self, public_key: &str, password: &str) -> Result<String> {
        let vars = json!({ "input": { "publicKey": public_key, "password": password } });
        let data = self
            .execute_query(queries::UNLOCK_ACCOUNT, Some(vars), "unlock_account")
            .await?;
        data["unlockAccount"]["publicKey"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| missing("unlock_account", "unlockAccount.publicKey"))
    }

    /// Set or unset the SNARK worker key. Returns the previous worker.
    pub async fn set_snark_worker(&self, public_key: Option<&str>) -> Result<Option<String>> {
        let vars = json!({ "input": public_key });
        let data = self
            .execute_query(queries::SET_SNARK_WORKER, Some(vars), "set_snark_worker")
            .await?;
        Ok(data["setSnarkWorker"]["lastSnarkWorker"]
            .as_str()
            .map(String::from))
    }

    /// Set the fee for SNARK work.
    pub async fn set_snark_work_fee(&self, fee: Currency) -> Result<String> {
        let vars = json!({ "fee": fee.to_nanomina_str() });
        let data = self
            .execute_query(
                queries::SET_SNARK_WORK_FEE,
                Some(vars),
                "set_snark_work_fee",
            )
            .await?;
        Ok(data["setSnarkWorkFee"]["lastFee"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }
}

/// Parse a JSON value that may be a string or number into u64.
fn parse_u64(v: &Value) -> u64 {
    opt_u64(v).unwrap_or(0)
}

// The daemon sends most integers (UInt32, UInt64, Length, Slot) as strings;
// these helpers accept a string or a number.

fn opt_u64(v: &Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_u64())
}

fn parse_i64(v: &Value) -> i64 {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_i64())
        .unwrap_or(0)
}

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

fn str_or_empty(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// A scalar as a string: the daemon sends some numbers as strings, some not.
fn scalar_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn strings(a: &[Value]) -> Vec<String> {
    a.iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

fn currency(v: &Value) -> Result<Currency> {
    Currency::from_graphql(&scalar_string(v))
}

fn opt_currency(v: &Value) -> Result<Option<Currency>> {
    match v {
        Value::Null => Ok(None),
        _ => currency(v).map(Some),
    }
}

fn missing(query_name: &str, field: &str) -> Error {
    Error::MissingField {
        query_name: query_name.into(),
        field: field.into(),
    }
}

fn parse_peer(p: &Value) -> PeerInfo {
    PeerInfo {
        peer_id: str_or_empty(&p["peerId"]),
        host: str_or_empty(&p["host"]),
        port: p["libp2pPort"].as_i64().unwrap_or_default(),
    }
}

fn parse_epoch(e: &Value) -> EpochData {
    EpochData {
        length: opt_u64(&e["epochLength"]),
        seed: str_or_empty(&e["seed"]),
        ledger_hash: str_or_empty(&e["ledger"]["hash"]),
    }
}

/// Parse a block of the common block selection (`BestChain`, `GenesisBlock`,
/// `Block`).
fn parse_block(block: &Value) -> Result<BlockInfo> {
    let protocol = &block["protocolState"];
    let consensus = &protocol["consensusState"];
    let chain = &protocol["blockchainState"];
    let txs = &block["transactions"];
    Ok(BlockInfo {
        state_hash: str_or_empty(&block["stateHash"]),
        height: parse_u64(&consensus["blockHeight"]),
        global_slot_since_hard_fork: parse_u64(&consensus["slot"]),
        global_slot_since_genesis: parse_u64(&consensus["slotSinceGenesis"]),
        creator_pk: block["creatorAccount"]["publicKey"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        command_transaction_count: parse_i64(&block["commandTransactionCount"]),
        previous_state_hash: str_or_empty(&protocol["previousStateHash"]),
        epoch: parse_u64(&consensus["epoch"]),
        block_creator: str_or_empty(&consensus["blockCreator"]),
        coinbase_receiver: opt_str(&consensus["coinbaseReceiever"]),
        staking_epoch: parse_epoch(&consensus["stakingEpochData"]),
        next_epoch: parse_epoch(&consensus["nextEpochData"]),
        date: scalar_string(&chain["date"]),
        utc_date: scalar_string(&chain["utcDate"]),
        snarked_ledger_hash: str_or_empty(&chain["snarkedLedgerHash"]),
        staged_ledger_hash: str_or_empty(&chain["stagedLedgerHash"]),
        coinbase: opt_currency(&txs["coinbase"])?.unwrap_or(Currency::from_nanomina(0)),
        coinbase_receiver_account: opt_str(&txs["coinbaseReceiverAccount"]["publicKey"]),
        fee_transfers: txs["feeTransfer"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|f| {
                        Ok(FeeTransfer {
                            recipient: str_or_empty(&f["recipient"]),
                            fee: currency(&f["fee"])?,
                            transfer_type: str_or_empty(&f["type"]),
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default(),
        user_commands: txs["userCommands"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|c| {
                        Ok(BlockTransaction {
                            id: str_or_empty(&c["id"]),
                            hash: str_or_empty(&c["hash"]),
                            kind: str_or_empty(&c["kind"]),
                            nonce: parse_u64(&c["nonce"]),
                            source: str_or_empty(&c["source"]["publicKey"]),
                            receiver: str_or_empty(&c["receiver"]["publicKey"]),
                            amount: opt_currency(&c["amount"])?
                                .unwrap_or(Currency::from_nanomina(0)),
                            fee: opt_currency(&c["fee"])?.unwrap_or(Currency::from_nanomina(0)),
                            memo: str_or_empty(&c["memo"]),
                            failure_reason: opt_str(&c["failureReason"]),
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default(),
    })
}

fn parse_submitted(c: &Value) -> Result<SubmittedCommand> {
    Ok(SubmittedCommand {
        id: str_or_empty(&c["id"]),
        hash: str_or_empty(&c["hash"]),
        nonce: parse_u64(&c["nonce"]),
        kind: str_or_empty(&c["kind"]),
        source: str_or_empty(&c["source"]["publicKey"]),
        receiver: str_or_empty(&c["receiver"]["publicKey"]),
        amount: opt_currency(&c["amount"])?,
        fee: opt_currency(&c["fee"])?,
        memo: str_or_empty(&c["memo"]),
    })
}

fn parse_zkapp_result(z: &Value) -> Result<ZkappCommandResult> {
    let command = &z["zkappCommand"];
    let payer = &command["feePayer"]["body"];
    Ok(ZkappCommandResult {
        id: str_or_empty(&z["id"]),
        hash: str_or_empty(&z["hash"]),
        memo: str_or_empty(&command["memo"]),
        fee_payer: ZkappFeePayer {
            public_key: str_or_empty(&payer["publicKey"]),
            fee: opt_currency(&payer["fee"])?.unwrap_or(Currency::from_nanomina(0)),
            nonce: parse_u64(&payer["nonce"]),
            valid_until: opt_u64(&payer["validUntil"]),
        },
        failure_reason: z["failureReason"].as_array().map(|a| {
            a.iter()
                .map(|f| ZkappFailure {
                    index: opt_u64(&f["index"]),
                    failures: f["failures"]
                        .as_array()
                        .map(|x| strings(x))
                        .unwrap_or_default(),
                })
                .collect()
        }),
    })
}

fn signature_json(signature: &Option<SignatureInput>) -> Value {
    match signature {
        Some(s) => json!({ "field": s.field, "scalar": s.scalar }),
        None => Value::Null,
    }
}

/// Builder returned by [`MinaClient::query`] for running arbitrary GraphQL.
///
/// Set optional `variables` and a log-friendly `name`, then `send()` to run
/// the query through the client's retry logic.
#[must_use = "call .send() to execute the query"]
pub struct QueryBuilder<'a> {
    client: &'a MinaClient,
    query: &'a str,
    variables: Option<Value>,
    name: Option<&'a str>,
}

impl<'a> QueryBuilder<'a> {
    /// Attach GraphQL `$variables` to the query.
    pub fn variables(mut self, variables: Value) -> Self {
        self.variables = Some(variables);
        self
    }

    /// Set a name for this query — used only in tracing/log messages.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Execute the query. Returns the GraphQL `data` field as a `serde_json::Value`.
    pub async fn send(self) -> Result<Value> {
        self.client
            .execute_query(self.query, self.variables, self.name.unwrap_or("custom"))
            .await
    }
}

/// Return the `data` field of a GraphQL response body, or [`Error::Graphql`]
/// when the body carries an `errors` array. Shared by every client in the crate.
pub(crate) fn graphql_data(body: Value, query_name: &str) -> Result<Value> {
    if let Some(errors) = body.get("errors").and_then(|e| e.as_array()) {
        let entries: Vec<GraphqlErrorEntry> = errors
            .iter()
            .map(|e| GraphqlErrorEntry {
                message: e
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            })
            .collect();
        let messages = entries
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Error::Graphql {
            query_name: query_name.to_string(),
            messages,
            errors: entries,
        });
    }
    Ok(body
        .get("data")
        .cloned()
        .unwrap_or(Value::Object(Default::default())))
}
