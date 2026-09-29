//! Client for the daemon's ITN GraphQL server (feature `itn`).
//!
//! A daemon started with `ITN_FEATURES=1`, `--itn-graphql-port <port>` and
//! `--itn-keys <base64 ed25519 public keys>` serves a second GraphQL API
//! (`Mina_graphql.schema_itn`). Load testing tools use it to schedule
//! payments and zkApp commands, read internal logs, change connection
//! gating and stop the daemon. Every request must be signed with an
//! ed25519 key whose public half is in `--itn-keys`.
//!
//! # Authentication protocol
//!
//! The daemon reads the `Authorization` header:
//!
//! - `Signature <pk> <sig>`: the signature covers the request body. The
//!   daemon accepts this form for the `auth` query only; every other
//!   operation answers "Missing sequence information".
//! - `Signature <pk> <sig> ; Sequencing <uuid> <n>`: the signature covers the
//!   big-endian `u16` `n`, then the server UUID, then the body. `n` must be
//!   exactly the daemon's sequence number for this public key, which starts
//!   at 0, goes up by one (wrapping) after each accepted sequenced request,
//!   and is shared by all clients that sign with the same key.
//!
//! `pk` and `sig` are standard base64. A bad signature or an unknown key
//! gives HTTP 401. A wrong UUID (the daemon restarted) or sequence number
//! gives HTTP 412.
//!
//! [`ItnClient`] runs `auth` to learn the UUID and the sequence number,
//! sends sequenced requests one at a time, and on a 412 runs `auth` again and
//! repeats the request once.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() -> mina_sdk::Result<()> {
//! use mina_sdk::itn::{ItnClient, ItnKey};
//!
//! let key = ItnKey::from_base64("<base64 seed>")?;
//! let client = ItnClient::new("http://127.0.0.1:3086/graphql", key);
//!
//! let auth = client.auth().await?;
//! println!("node {:?}, block producer: {}", auth.peer_id, auth.is_block_producer);
//!
//! let logs = client.internal_logs(0).await?;
//! if let Some(last) = logs.last() {
//!     client.flush_internal_logs(last.id).await?;
//! }
//! # Ok(())
//! # }
//! ```

mod key;
pub mod queries;
mod types;

pub use key::ItnKey;
pub use types::{
    GatingUpdate, ItnAuth, ItnLog, NetworkPeer, PaymentsDetails, ZkappCommandsDetails,
};

use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::sync::{Mutex, MutexGuard};
use tracing::{debug, warn};

use crate::client::graphql_data;
use crate::error::{Error, Result};
use crate::ClientConfig;

/// Server UUID and the next sequence number of this client's key.
struct Session {
    uuid: String,
    seq: u16,
}

/// Client for a daemon's ITN GraphQL server. See the [module docs](self).
///
/// One client holds one session per daemon, so share it (for example in an
/// `Arc`) instead of creating a client per request.
pub struct ItnClient {
    config: ClientConfig,
    http: reqwest::Client,
    key: ItnKey,
    session: Mutex<Option<Session>>,
}

impl ItnClient {
    /// Create a client for the ITN endpoint `graphql_uri`
    /// (for example `http://127.0.0.1:3086/graphql`) with default settings.
    pub fn new(graphql_uri: &str, key: ItnKey) -> Self {
        Self::with_config(
            ClientConfig {
                graphql_uri: graphql_uri.to_string(),
                ..Default::default()
            },
            key,
        )
    }

    /// Create a client for `http://{host}:{port}/graphql`.
    pub fn from_host_and_port(host: &str, port: u16, key: ItnKey) -> Self {
        Self::new(&format!("http://{host}:{port}/graphql"), key)
    }

    /// Create a client with custom configuration.
    ///
    /// `retries` and `retry_delay` apply to the `auth` handshake only. A
    /// sequenced request is never repeated after a transport error, because
    /// the daemon may have run it (for example started a payment schedule).
    ///
    /// # Panics
    ///
    /// Panics if `retries` is 0 or `timeout` is zero.
    pub fn with_config(config: ClientConfig, key: ItnKey) -> Self {
        assert!(config.retries >= 1, "retries must be at least 1");
        assert!(
            !config.timeout.is_zero(),
            "timeout must be greater than zero"
        );
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            http,
            key,
            session: Mutex::new(None),
        }
    }

    /// The ITN endpoint URI.
    pub fn graphql_uri(&self) -> &str {
        &self.config.graphql_uri
    }

    /// The base64 public key of this client's key, as it must appear in
    /// the daemon's `--itn-keys`.
    pub fn public_key_base64(&self) -> String {
        self.key.public_key_base64()
    }

    // -- Transport --

    async fn post(&self, body: &[u8], authorization: String) -> reqwest::Result<reqwest::Response> {
        self.http
            .post(&self.config.graphql_uri)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::AUTHORIZATION, authorization)
            .body(body.to_vec())
            .send()
            .await
    }

    /// Run `auth` with an unsequenced signature and store the session.
    async fn handshake(&self, session: &mut MutexGuard<'_, Option<Session>>) -> Result<ItnAuth> {
        const NAME: &str = "itn_auth";
        let body = serde_json::to_vec(&json!({ "query": queries::AUTH }))
            .expect("serializing a JSON value cannot fail");
        let authorization = format!(
            "Signature {} {}",
            self.key.public_key_base64(),
            self.key.sign_base64(&body)
        );

        let mut last_err: Option<reqwest::Error> = None;
        for attempt in 1..=self.config.retries {
            debug!(attempt, max = self.config.retries, "ITN auth request");
            match self.post(&body, authorization.clone()).await {
                Ok(resp) if resp.status() == StatusCode::UNAUTHORIZED => {
                    return Err(Error::ItnUnauthorized {
                        query_name: NAME.into(),
                    });
                }
                Ok(resp) if !resp.status().is_success() => {
                    warn!(attempt, status = %resp.status(), "ITN auth: HTTP error");
                    last_err = Some(resp.error_for_status().unwrap_err());
                }
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(body) => {
                        let data = graphql_data(body, NAME)?;
                        let auth = parse_auth(&data["auth"])?;
                        **session = Some(Session {
                            uuid: auth.server_uuid.clone(),
                            seq: auth.signer_sequence_number,
                        });
                        return Ok(auth);
                    }
                    Err(e) => {
                        warn!(attempt, error = %e, "ITN auth: failed to parse response");
                        last_err = Some(e);
                    }
                },
                Err(e) => {
                    warn!(attempt, error = %e, "ITN auth: connection error");
                    last_err = Some(e);
                }
            }
            if attempt < self.config.retries {
                tokio::time::sleep(self.config.retry_delay).await;
            }
        }
        Err(Error::Connection {
            query_name: NAME.into(),
            attempts: self.config.retries,
            source: last_err.expect("at least one attempt must have been made"),
        })
    }

    /// Send one sequenced request and return its `data` field.
    ///
    /// Requests of one client are sent one at a time, because the daemon
    /// accepts only its exact next sequence number.
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
        let body = serde_json::to_vec(&payload).expect("serializing a JSON value cannot fail");

        let mut session = self.session.lock().await;
        // A second pass only follows a 412: the daemon restarted, or another
        // client with the same key used our sequence number.
        for pass in 1..=2 {
            if session.is_none() {
                self.handshake(&mut session).await?;
            }
            let (uuid, seq) = {
                let s = session.as_ref().expect("handshake stores a session");
                (s.uuid.clone(), s.seq)
            };
            let mut msg = Vec::with_capacity(2 + uuid.len() + body.len());
            msg.extend_from_slice(&seq.to_be_bytes());
            msg.extend_from_slice(uuid.as_bytes());
            msg.extend_from_slice(&body);
            let authorization = format!(
                "Signature {} {} ; Sequencing {uuid} {seq}",
                self.key.public_key_base64(),
                self.key.sign_base64(&msg)
            );

            debug!(query_name, pass, seq, "ITN GraphQL request");
            let resp = match self.post(&body, authorization).await {
                Ok(resp) => resp,
                Err(e) => {
                    // Unknown whether the daemon counted it: start over next time.
                    *session = None;
                    return Err(Error::Connection {
                        query_name: query_name.into(),
                        attempts: 1,
                        source: e,
                    });
                }
            };
            match resp.status() {
                StatusCode::PRECONDITION_FAILED => {
                    warn!(
                        query_name,
                        pass, "ITN: stale session (412), running auth again"
                    );
                    *session = None;
                    continue;
                }
                StatusCode::UNAUTHORIZED => {
                    return Err(Error::ItnUnauthorized {
                        query_name: query_name.into(),
                    });
                }
                status if !status.is_success() => {
                    *session = None;
                    return Err(Error::Connection {
                        query_name: query_name.into(),
                        attempts: 1,
                        source: resp.error_for_status().unwrap_err(),
                    });
                }
                _ => {}
            }
            // The signature was accepted, so the daemon has moved to the
            // next number, whatever the GraphQL result is.
            if let Some(s) = session.as_mut() {
                s.seq = s.seq.wrapping_add(1);
            }
            let body = resp.json::<Value>().await.map_err(|e| Error::Connection {
                query_name: query_name.into(),
                attempts: 1,
                source: e,
            })?;
            return graphql_data(body, query_name);
        }
        Err(Error::ItnSequencing {
            query_name: query_name.into(),
        })
    }

    // -- Operations --

    /// Run the `auth` handshake and return the node's answer. Other methods
    /// call it when needed, so calling it first is optional.
    pub async fn auth(&self) -> Result<ItnAuth> {
        let mut session = self.session.lock().await;
        self.handshake(&mut session).await
    }

    /// Global slots the node's block producer keys won in the current epoch.
    pub async fn slots_won(&self) -> Result<Vec<u64>> {
        let data = self
            .execute_query(queries::SLOTS_WON, None, "itn_slots_won")
            .await?;
        array(&data, "slotsWon", "itn_slots_won")?
            .iter()
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| missing("itn_slots_won", "slotsWon"))
            })
            .collect()
    }

    /// Internal logs with an ID of at least `start_log_id`.
    pub async fn internal_logs(&self, start_log_id: i64) -> Result<Vec<ItnLog>> {
        const NAME: &str = "itn_internal_logs";
        let data = self
            .execute_query(
                queries::INTERNAL_LOGS,
                Some(json!({ "startLogId": start_log_id })),
                NAME,
            )
            .await?;
        array(&data, "internalLogs", NAME)?
            .iter()
            .map(|l| parse_log(l).ok_or_else(|| missing(NAME, "internalLogs")))
            .collect()
    }

    /// Drop internal logs up to and including `end_log_id`. Returns the
    /// daemon's answer, the number of logs deleted.
    pub async fn flush_internal_logs(&self, end_log_id: i64) -> Result<String> {
        self.string_mutation(
            queries::FLUSH_INTERNAL_LOGS,
            json!({ "endLogId": end_log_id }),
            "flushInternalLogs",
            "itn_flush_internal_logs",
        )
        .await
    }

    /// Start sending payments. Returns the handle for
    /// [`ItnClient::stop_scheduled_transactions`].
    pub async fn schedule_payments(&self, details: &PaymentsDetails) -> Result<String> {
        self.string_mutation(
            queries::SCHEDULE_PAYMENTS,
            json!({ "input": details.to_json() }),
            "schedulePayments",
            "itn_schedule_payments",
        )
        .await
    }

    /// Start sending zkApp commands. Returns the handle for
    /// [`ItnClient::stop_scheduled_transactions`].
    pub async fn schedule_zkapp_commands(&self, details: &ZkappCommandsDetails) -> Result<String> {
        self.string_mutation(
            queries::SCHEDULE_ZKAPP_COMMANDS,
            json!({ "input": details.to_json() }),
            "scheduleZkappCommands",
            "itn_schedule_zkapp_commands",
        )
        .await
    }

    /// Stop the transactions of a schedule handle.
    pub async fn stop_scheduled_transactions(&self, handle: &str) -> Result<String> {
        self.string_mutation(
            queries::STOP_SCHEDULED_TRANSACTIONS,
            json!({ "handle": handle }),
            "stopScheduledTransactions",
            "itn_stop_scheduled_transactions",
        )
        .await
    }

    /// Change the node's connection gating.
    pub async fn update_gating(&self, update: &GatingUpdate) -> Result<String> {
        self.string_mutation(
            queries::UPDATE_GATING,
            json!({ "input": update.to_json() }),
            "updateGating",
            "itn_update_gating",
        )
        .await
    }

    /// Stop the daemon after `delay_seconds` (the daemon's minimum is 5),
    /// deleting its configuration directory if `clean_config` is true.
    pub async fn stop_daemon(
        &self,
        delay_seconds: Option<i64>,
        clean_config: Option<bool>,
    ) -> Result<String> {
        self.string_mutation(
            queries::STOP_DAEMON,
            json!({ "delaySeconds": delay_seconds, "cleanConfig": clean_config }),
            "stopDaemon",
            "itn_stop_daemon",
        )
        .await
    }

    /// Set the block producer's limit of zkApp commands per block; `None`
    /// removes the limit. Returns the limit now in force.
    pub async fn set_zkapp_command_limit(&self, limit: Option<i64>) -> Result<Option<i64>> {
        let data = self
            .execute_query(
                queries::ZKAPP_COMMAND_LIMIT,
                Some(json!({ "limit": limit })),
                "itn_set_zkapp_command_limit",
            )
            .await?;
        Ok(data["zkAppCommandLimit"].as_i64())
    }

    async fn string_mutation(
        &self,
        query: &str,
        variables: Value,
        field: &str,
        query_name: &str,
    ) -> Result<String> {
        let data = self
            .execute_query(query, Some(variables), query_name)
            .await?;
        data[field]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| missing(query_name, field))
    }
}

fn missing(query_name: &str, field: &str) -> Error {
    Error::MissingField {
        query_name: query_name.into(),
        field: field.into(),
    }
}

fn array<'a>(data: &'a Value, field: &str, query_name: &str) -> Result<&'a Vec<Value>> {
    data[field]
        .as_array()
        .ok_or_else(|| missing(query_name, field))
}

/// `UInt16` arrives as a string ("8302"); accept a number too.
fn parse_u16(v: &Value) -> Option<u16> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        _ => None,
    }
}

fn parse_auth(v: &Value) -> Result<ItnAuth> {
    let field = |name: &str| missing("itn_auth", &format!("auth.{name}"));
    Ok(ItnAuth {
        server_uuid: v["serverUuid"]
            .as_str()
            .ok_or_else(|| field("serverUuid"))?
            .to_string(),
        signer_sequence_number: parse_u16(&v["signerSequenceNumber"])
            .ok_or_else(|| field("signerSequenceNumber"))?,
        libp2p_port: parse_u16(&v["libp2pPort"]).ok_or_else(|| field("libp2pPort"))?,
        peer_id: v["peerId"].as_str().map(str::to_string),
        is_block_producer: v["isBlockProducer"]
            .as_bool()
            .ok_or_else(|| field("isBlockProducer"))?,
    })
}

fn parse_log(v: &Value) -> Option<ItnLog> {
    Some(ItnLog {
        id: v["id"].as_i64()?,
        timestamp: v["timestamp"].as_str()?.to_string(),
        message: v["message"].as_str()?.to_string(),
        metadata: v["metadata"]
            .as_array()?
            .iter()
            .map(|m| Some((m["item"].as_str()?.to_string(), m["value"].clone())))
            .collect::<Option<Vec<_>>>()?,
        process: v["process"].as_str().map(str::to_string),
    })
}
