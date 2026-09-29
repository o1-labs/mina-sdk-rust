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
        let data = self
            .execute_query(queries::DAEMON_STATUS, None, "get_daemon_status")
            .await?;
        let status = &data["daemonStatus"];

        let sync_status: SyncStatus =
            serde_json::from_value(status.get("syncStatus").cloned().unwrap_or(Value::Null))
                .map_err(|_| Error::MissingField {
                    query_name: "get_daemon_status".into(),
                    field: "syncStatus".into(),
                })?;

        let peers = status.get("peers").and_then(|p| p.as_array()).map(|arr| {
            arr.iter()
                .map(|p| PeerInfo {
                    peer_id: p["peerId"].as_str().unwrap_or_default().to_string(),
                    host: p["host"].as_str().unwrap_or_default().to_string(),
                    port: p["libp2pPort"].as_i64().unwrap_or_default(),
                })
                .collect()
        });

        Ok(DaemonStatus {
            sync_status,
            blockchain_length: status["blockchainLength"].as_i64(),
            highest_block_length_received: status["highestBlockLengthReceived"].as_i64(),
            uptime_secs: status["uptimeSecs"].as_i64(),
            state_hash: status["stateHash"].as_str().map(String::from),
            commit_id: status["commitId"].as_str().map(String::from),
            peers,
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
            .ok_or_else(|| Error::MissingField {
                query_name: "get_network_id".into(),
                field: "networkID".into(),
            })
    }

    /// Get account data for a public key.
    pub async fn get_account(
        &self,
        public_key: &str,
        token_id: Option<&str>,
    ) -> Result<AccountData> {
        let (query, vars) = match token_id {
            Some(token) => (
                queries::GET_ACCOUNT_WITH_TOKEN,
                json!({ "publicKey": public_key, "token": token }),
            ),
            None => (queries::GET_ACCOUNT, json!({ "publicKey": public_key })),
        };

        let data = self.execute_query(query, Some(vars), "get_account").await?;

        let acc = data
            .get("account")
            .filter(|v| !v.is_null())
            .ok_or_else(|| Error::AccountNotFound(public_key.to_string()))?;

        let balance = &acc["balance"];
        let total = Currency::from_graphql(balance["total"].as_str().unwrap_or("0"))?;
        let liquid = balance["liquid"]
            .as_str()
            .map(Currency::from_graphql)
            .transpose()?;
        let locked = balance["locked"]
            .as_str()
            .map(Currency::from_graphql)
            .transpose()?;

        Ok(AccountData {
            public_key: acc["publicKey"].as_str().unwrap_or_default().to_string(),
            nonce: acc["nonce"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| acc["nonce"].as_u64())
                .unwrap_or(0),
            delegate: acc["delegate"].as_str().map(String::from),
            token_id: acc["tokenId"].as_str().map(String::from),
            balance: AccountBalance {
                total,
                liquid,
                locked,
            },
        })
    }

    /// Get blocks from the best chain.
    pub async fn get_best_chain(&self, max_length: Option<u32>) -> Result<Vec<BlockInfo>> {
        let vars = max_length.map(|n| json!({ "maxLength": n }));
        let data = self
            .execute_query(queries::BEST_CHAIN, vars, "get_best_chain")
            .await?;

        let chain = match data.get("bestChain").and_then(|c| c.as_array()) {
            Some(arr) => arr,
            None => return Ok(vec![]),
        };

        let blocks = chain
            .iter()
            .map(|block| {
                let consensus = &block["protocolState"]["consensusState"];
                let creator_pk = block
                    .get("creatorAccount")
                    .and_then(|c| c["publicKey"].as_str())
                    .unwrap_or("unknown")
                    .to_string();

                BlockInfo {
                    state_hash: block["stateHash"].as_str().unwrap_or_default().to_string(),
                    height: parse_u64(&consensus["blockHeight"]),
                    global_slot_since_hard_fork: parse_u64(&consensus["slot"]),
                    global_slot_since_genesis: parse_u64(&consensus["slotSinceGenesis"]),
                    creator_pk,
                    command_transaction_count: block["commandTransactionCount"]
                        .as_i64()
                        .unwrap_or(0),
                }
            })
            .collect();

        Ok(blocks)
    }

    /// Get the list of connected peers.
    pub async fn get_peers(&self) -> Result<Vec<PeerInfo>> {
        let data = self
            .execute_query(queries::GET_PEERS, None, "get_peers")
            .await?;
        let peers = data
            .get("getPeers")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|p| PeerInfo {
                        peer_id: p["peerId"].as_str().unwrap_or_default().to_string(),
                        host: p["host"].as_str().unwrap_or_default().to_string(),
                        port: p["libp2pPort"].as_i64().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(peers)
    }

    /// Get pending user commands from the transaction pool.
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

        let commands: Vec<PooledUserCommand> = data
            .get("pooledUserCommands")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        Ok(commands)
    }

    // -- Mutations --

    /// Send a payment transaction.
    ///
    /// Requires the sender's account to be unlocked on the node.
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

        let data = self
            .execute_query(
                queries::SEND_PAYMENT,
                Some(json!({ "input": input })),
                "send_payment",
            )
            .await?;

        let result = &data["sendPayment"]["payment"];
        Ok(SendPaymentResult {
            id: result["id"].as_str().unwrap_or_default().to_string(),
            hash: result["hash"].as_str().unwrap_or_default().to_string(),
            nonce: parse_u64(&result["nonce"]),
        })
    }

    /// Send a stake delegation transaction.
    ///
    /// Requires the sender's account to be unlocked on the node.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(client: &mina_sdk::MinaClient) -> mina_sdk::Result<()> {
    /// use mina_sdk::{Delegation, Currency};
    ///
    /// let result = client.send_delegation(
    ///     Delegation::sender("B62qsender...")
    ///         .to("B62qdelegate...")
    ///         .fee(Currency::from_mina("0.01")?),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
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

        let data = self
            .execute_query(
                queries::SEND_DELEGATION,
                Some(json!({ "input": input })),
                "send_delegation",
            )
            .await?;

        let result = &data["sendDelegation"]["delegation"];
        Ok(SendDelegationResult {
            id: result["id"].as_str().unwrap_or_default().to_string(),
            hash: result["hash"].as_str().unwrap_or_default().to_string(),
            nonce: parse_u64(&result["nonce"]),
        })
    }

    /// Set or unset the SNARK worker key.
    ///
    /// Pass `None` to disable the SNARK worker.
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
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_u64())
        .unwrap_or(0)
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
