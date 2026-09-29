use serde_json::{json, Value};

use crate::Currency;

/// Result of the `auth` query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItnAuth {
    /// UUID of the ITN GraphQL server. It is new after each daemon restart.
    pub server_uuid: String,
    /// The sequence number the next sequenced request of this key must carry.
    pub signer_sequence_number: u16,
    /// The node's libp2p port.
    pub libp2p_port: u16,
    /// The node's libp2p peer ID.
    pub peer_id: Option<String>,
    /// Whether the node produces blocks.
    pub is_block_producer: bool,
}

/// One entry of the daemon's internal log (`internalLogs`).
#[derive(Debug, Clone, PartialEq)]
pub struct ItnLog {
    /// The log ID; IDs increase.
    pub id: i64,
    /// Timestamp of the log.
    pub timestamp: String,
    /// The log message.
    pub message: String,
    /// Metadata, as (item, value) pairs; the values are arbitrary JSON.
    pub metadata: Vec<(String, Value)>,
    /// The process that sent the log if it is not the daemon (prover or verifier).
    pub process: Option<String>,
}

/// Input of `schedulePayments`.
#[derive(Debug, Clone)]
pub struct PaymentsDetails {
    /// Length of the scheduler run, in minutes.
    pub duration_min: i64,
    /// Frequency of transactions, per second.
    pub tps: f64,
    /// Memo, up to 32 characters.
    pub memo_prefix: String,
    /// Maximum fee.
    pub max_fee: Currency,
    /// Minimum fee.
    pub min_fee: Currency,
    /// Amount of each payment.
    pub amount: Currency,
    /// Public key of the receiver of the payments.
    pub receiver: String,
    /// Private keys (base58) of the accounts to send from.
    pub senders: Vec<String>,
}

impl PaymentsDetails {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "durationMin": self.duration_min,
            "tps": self.tps,
            "memoPrefix": self.memo_prefix,
            "maxFee": self.max_fee.to_nanomina_str(),
            "minFee": self.min_fee.to_nanomina_str(),
            "amount": self.amount.to_nanomina_str(),
            "receiver": self.receiver,
            "senders": self.senders,
        })
    }
}

/// Input of `scheduleZkappCommands`.
#[derive(Debug, Clone)]
pub struct ZkappCommandsDetails {
    /// Each generated zkApp transaction has `2 * max_account_updates + 2`
    /// account updates (including balancing and fee payer).
    pub max_account_updates: Option<i64>,
    /// Generate max-cost zkApp commands.
    pub max_cost: bool,
    /// Size of the queue of recently used accounts.
    pub account_queue_size: i64,
    /// Fee for the initial deployment of zkApp accounts.
    pub deployment_fee: Currency,
    /// Maximum fee.
    pub max_fee: Currency,
    /// Minimum fee.
    pub min_fee: Currency,
    /// Initial balance of the zkApp accounts deployed for the test.
    pub init_balance: Currency,
    /// Maximum balance of a new zkApp account.
    pub max_new_zkapp_balance: Currency,
    /// Minimum balance of a new zkApp account.
    pub min_new_zkapp_balance: Currency,
    /// Maximum balance change.
    pub max_balance_change: Currency,
    /// Minimum balance change.
    pub min_balance_change: Currency,
    /// Disable the precondition in account updates.
    pub no_precondition: bool,
    /// Prefix of the memo.
    pub memo_prefix: String,
    /// Length of the scheduler run, in minutes.
    pub duration_min: i64,
    /// Frequency of transactions, per second.
    pub tps: f64,
    /// Number of zkApp accounts the scheduler creates during the test.
    pub num_new_accounts: i64,
    /// Number of zkApp accounts deployed at the start of the test.
    pub num_zkapps_to_deploy: i64,
    /// Private keys (base58) of the fee payers, which also create the accounts.
    pub fee_payers: Vec<String>,
    /// Load a custom (non-default) owned token. Only an unreleased daemon
    /// branch has this field; released daemons (for example 4.0.0) ignore it,
    /// because ocaml-graphql-server does not check input fields that its
    /// schema does not declare. It is sent only when set.
    pub non_default_token: Option<bool>,
}

impl ZkappCommandsDetails {
    pub(crate) fn to_json(&self) -> Value {
        let mut v = json!({
            "maxAccountUpdates": self.max_account_updates,
            "maxCost": self.max_cost,
            "accountQueueSize": self.account_queue_size,
            "deploymentFee": self.deployment_fee.to_nanomina_str(),
            "maxFee": self.max_fee.to_nanomina_str(),
            "minFee": self.min_fee.to_nanomina_str(),
            "initBalance": self.init_balance.to_nanomina_str(),
            "maxNewZkappBalance": self.max_new_zkapp_balance.to_nanomina_str(),
            "minNewZkappBalance": self.min_new_zkapp_balance.to_nanomina_str(),
            "maxBalanceChange": self.max_balance_change.to_nanomina_str(),
            "minBalanceChange": self.min_balance_change.to_nanomina_str(),
            "noPrecondition": self.no_precondition,
            "memoPrefix": self.memo_prefix,
            "durationMin": self.duration_min,
            "tps": self.tps,
            "numNewAccounts": self.num_new_accounts,
            "numZkappsToDeploy": self.num_zkapps_to_deploy,
            "feePayers": self.fee_payers,
        });
        if let Some(t) = self.non_default_token {
            v["nonDefaultToken"] = Value::Bool(t);
        }
        v
    }
}

/// A peer in a [`GatingUpdate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPeer {
    /// IP address of the remote host.
    pub host: String,
    /// libp2p port of the remote host.
    pub libp2p_port: u16,
    /// Base58 peer ID.
    pub peer_id: String,
}

impl NetworkPeer {
    fn to_json(&self) -> Value {
        json!({
            "host": self.host,
            "libp2pPort": self.libp2p_port,
            "peerId": self.peer_id,
        })
    }
}

/// Input of `updateGating`.
#[derive(Debug, Clone, Default)]
pub struct GatingUpdate {
    /// Peers to connect to.
    pub added_peers: Vec<NetworkPeer>,
    /// Reset the added peers, including the seeds, to an empty list.
    pub clean_added_peers: bool,
    /// Allow connections only from trusted peers.
    pub isolate: bool,
    /// Peers never allowed to connect, unless they are also trusted.
    pub banned_peers: Vec<NetworkPeer>,
    /// Peers always allowed to connect.
    pub trusted_peers: Vec<NetworkPeer>,
}

impl GatingUpdate {
    pub(crate) fn to_json(&self) -> Value {
        let peers = |ps: &[NetworkPeer]| ps.iter().map(NetworkPeer::to_json).collect::<Vec<_>>();
        json!({
            "addedPeers": peers(&self.added_peers),
            "cleanAddedPeers": self.clean_added_peers,
            "isolate": self.isolate,
            "bannedPeers": peers(&self.banned_peers),
            "trustedPeers": peers(&self.trusted_peers),
        })
    }
}
