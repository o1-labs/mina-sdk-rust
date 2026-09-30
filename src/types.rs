use serde::{Deserialize, Serialize};

use crate::Currency;

/// Sync status of a Mina daemon node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncStatus {
    Connecting,
    Listening,
    Offline,
    Bootstrap,
    Synced,
    Catchup,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connecting => write!(f, "CONNECTING"),
            Self::Listening => write!(f, "LISTENING"),
            Self::Offline => write!(f, "OFFLINE"),
            Self::Bootstrap => write!(f, "BOOTSTRAP"),
            Self::Synced => write!(f, "SYNCED"),
            Self::Catchup => write!(f, "CATCHUP"),
        }
    }
}

/// Information about a connected peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub peer_id: String,
    pub host: String,
    pub port: i64,
}

/// Comprehensive daemon status.
#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub sync_status: SyncStatus,
    pub blockchain_length: Option<i64>,
    pub highest_block_length_received: Option<i64>,
    pub highest_unvalidated_block_length_received: Option<i64>,
    pub uptime_secs: Option<i64>,
    pub state_hash: Option<String>,
    pub commit_id: Option<String>,
    pub num_accounts: Option<i64>,
    pub ledger_merkle_root: Option<String>,
    pub chain_id: Option<String>,
    pub catchup_status: Option<Vec<String>>,
    pub block_production_keys: Vec<String>,
    pub coinbase_receiver: Option<String>,
    pub peers: Option<Vec<PeerInfo>>,
    pub addrs_and_ports: Option<AddrsAndPorts>,
}

/// The daemon's network addresses and ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddrsAndPorts {
    pub external_ip: String,
    pub bind_ip: String,
    pub client_port: i64,
    pub libp2p_port: i64,
}

/// Daemon metrics (`daemonStatus.metrics`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonMetrics {
    pub block_production_delay: Vec<i64>,
    pub transaction_pool_diff_received: i64,
    pub transaction_pool_diff_broadcasted: i64,
    pub transactions_added_to_pool: i64,
    pub transaction_pool_size: i64,
    pub snark_pool_diff_received: i64,
    pub snark_pool_diff_broadcasted: i64,
    pub pending_snark_work: i64,
    pub snark_pool_size: i64,
}

/// Account balance with total, liquid, and locked amounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalance {
    pub total: Currency,
    pub liquid: Option<Currency>,
    pub locked: Option<Currency>,
    /// Height of the block the balance was read at.
    pub block_height: Option<u64>,
}

/// Vesting schedule of a timed account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTiming {
    pub initial_minimum_balance: Option<Currency>,
    pub cliff_time: Option<u64>,
    pub cliff_amount: Option<Currency>,
    pub vesting_period: Option<u64>,
    pub vesting_increment: Option<Currency>,
}

/// Account permissions, as the daemon names the authorization levels
/// (for example `Signature`, `Proof`, `None`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountPermissions {
    pub edit_state: Option<String>,
    pub send: Option<String>,
    pub receive: Option<String>,
    pub access: Option<String>,
    pub set_delegate: Option<String>,
    pub set_permissions: Option<String>,
    /// (authorization, transaction version)
    pub set_verification_key: Option<(String, String)>,
    pub set_zkapp_uri: Option<String>,
    pub edit_action_state: Option<String>,
    pub set_token_symbol: Option<String>,
    pub increment_nonce: Option<String>,
    pub set_voting_for: Option<String>,
    pub set_timing: Option<String>,
}

/// Account data returned by the daemon.
#[derive(Debug, Clone)]
pub struct AccountData {
    pub public_key: String,
    pub nonce: u64,
    pub balance: AccountBalance,
    pub delegate: Option<String>,
    pub token_id: Option<String>,
    pub token_symbol: Option<String>,
    pub voting_for: Option<String>,
    pub receipt_chain_hash: Option<String>,
    /// Null on an account without a vesting schedule.
    pub timing: Option<AccountTiming>,
    pub permissions: Option<AccountPermissions>,
    /// Null on an account that is not a zkApp.
    pub zkapp_state: Option<Vec<String>>,
    pub proved_state: Option<bool>,
    pub zkapp_uri: Option<String>,
}

/// A block (from `get_best_chain`, `get_genesis_block` or `get_block`).
#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub state_hash: String,
    pub height: u64,
    pub global_slot_since_hard_fork: u64,
    pub global_slot_since_genesis: u64,
    pub creator_pk: String,
    pub command_transaction_count: i64,
    pub previous_state_hash: String,
    pub epoch: u64,
    pub block_creator: String,
    /// The consensus state's coinbase receiver (`coinbaseReceiever`).
    pub coinbase_receiver: Option<String>,
    pub staking_epoch: EpochData,
    pub next_epoch: EpochData,
    pub date: String,
    pub utc_date: String,
    pub snarked_ledger_hash: String,
    pub staged_ledger_hash: String,
    pub coinbase: Currency,
    /// The account that received the coinbase, if any.
    pub coinbase_receiver_account: Option<String>,
    pub fee_transfers: Vec<FeeTransfer>,
    pub user_commands: Vec<BlockTransaction>,
}

/// Epoch data of a block's consensus state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EpochData {
    /// Only for the staking epoch.
    pub length: Option<u64>,
    pub seed: String,
    pub ledger_hash: String,
}

/// A fee transfer in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeTransfer {
    pub recipient: String,
    pub fee: Currency,
    pub transfer_type: String,
}

/// A user command in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTransaction {
    pub id: String,
    pub hash: String,
    pub kind: String,
    pub nonce: u64,
    pub source: String,
    pub receiver: String,
    pub amount: Currency,
    pub fee: Currency,
    pub memo: String,
    pub failure_reason: Option<String>,
}

/// A command the daemon accepted into its pool (`SubmittedCommand` in the
/// common API).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedCommand {
    pub id: String,
    pub hash: String,
    pub nonce: u64,
    pub kind: String,
    pub source: String,
    pub receiver: String,
    pub amount: Option<Currency>,
    pub fee: Option<Currency>,
    pub memo: String,
}

/// Result of a send_payment mutation.
pub type SendPaymentResult = SubmittedCommand;

/// Result of a send_delegation mutation.
pub type SendDelegationResult = SubmittedCommand;

/// A signature made outside the daemon, for example with `mina-signer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInput {
    pub field: String,
    pub scalar: String,
}

/// A zkApp command in the pool or just sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkappCommandResult {
    pub id: String,
    pub hash: String,
    pub memo: String,
    pub fee_payer: ZkappFeePayer,
    /// One entry for each failing account update; `None` if it did not fail.
    pub failure_reason: Option<Vec<ZkappFailure>>,
}

/// The fee payer of a zkApp command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkappFeePayer {
    pub public_key: String,
    pub fee: Currency,
    pub nonce: u64,
    pub valid_until: Option<u64>,
}

/// Why an account update of a zkApp command failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkappFailure {
    pub index: Option<u64>,
    pub failures: Vec<String>,
}

/// Completed snark work in the snark pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedWork {
    pub prover: String,
    pub fee: Currency,
    pub work_ids: Vec<i64>,
}

/// Status of a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    Pending,
    Included,
    Unknown,
}

/// The genesis constants of the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisConstants {
    pub genesis_timestamp: String,
    pub coinbase: Currency,
    pub account_creation_fee: Currency,
}

/// An account the daemon tracks (one of its wallet keys).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedAccount {
    pub public_key: String,
    pub balance: Currency,
}

/// Which block `get_block` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockRef {
    StateHash(String),
    Height(u64),
}

/// Which transaction `get_transaction_status` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionRef {
    /// A payment or delegation, by its ID.
    Payment(String),
    /// A zkApp command, by its ID.
    Zkapp(String),
}

/// Parameters for sending a payment transaction.
///
/// Built using a fluent API where each method documents its purpose:
///
/// ```
/// use mina_sdk::{Payment, Currency};
///
/// let payment = Payment::sender("B62qsender...")
///     .to("B62qreceiver...")
///     .amount(Currency::from_nanomina(1_500_000_000))
///     .fee(Currency::from_nanomina(10_000_000))
///     .memo("coffee payment")
///     .nonce(42);
/// ```
#[derive(Debug, Clone)]
pub struct Payment {
    pub sender: String,
    pub receiver: String,
    pub amount: Currency,
    pub fee: Currency,
    pub memo: Option<String>,
    pub nonce: Option<u64>,
    /// A signature made outside the daemon; `None` lets the daemon sign with
    /// its keystore.
    pub signature: Option<SignatureInput>,
}

impl Payment {
    /// Start building a payment with the sender's public key.
    pub fn sender(sender: &str) -> Self {
        Self {
            sender: sender.to_string(),
            receiver: String::new(),
            amount: Currency::from_nanomina(0),
            fee: Currency::from_nanomina(0),
            memo: None,
            nonce: None,
            signature: None,
        }
    }

    /// Set the receiver's public key.
    pub fn to(mut self, receiver: &str) -> Self {
        self.receiver = receiver.to_string();
        self
    }

    /// Set the payment amount.
    pub fn amount(mut self, amount: Currency) -> Self {
        self.amount = amount;
        self
    }

    /// Set the transaction fee.
    pub fn fee(mut self, fee: Currency) -> Self {
        self.fee = fee;
        self
    }

    /// Set an optional memo.
    pub fn memo(mut self, memo: &str) -> Self {
        self.memo = Some(memo.to_string());
        self
    }

    /// Set an explicit nonce (otherwise the daemon picks the next nonce).
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Send with a signature made outside the daemon.
    pub fn signature(mut self, signature: SignatureInput) -> Self {
        self.signature = Some(signature);
        self
    }
}

/// Parameters for sending a stake delegation transaction.
///
/// ```
/// use mina_sdk::{Delegation, Currency};
///
/// let delegation = Delegation::sender("B62qsender...")
///     .to("B62qdelegate...")
///     .fee(Currency::from_nanomina(10_000_000))
///     .memo("staking");
/// ```
#[derive(Debug, Clone)]
pub struct Delegation {
    pub sender: String,
    pub delegate_to: String,
    pub fee: Currency,
    pub memo: Option<String>,
    pub nonce: Option<u64>,
    /// A signature made outside the daemon; `None` lets the daemon sign with
    /// its keystore.
    pub signature: Option<SignatureInput>,
}

impl Delegation {
    /// Start building a delegation with the sender's public key.
    pub fn sender(sender: &str) -> Self {
        Self {
            sender: sender.to_string(),
            delegate_to: String::new(),
            fee: Currency::from_nanomina(0),
            memo: None,
            nonce: None,
            signature: None,
        }
    }

    /// Set the delegate's public key.
    pub fn to(mut self, delegate_to: &str) -> Self {
        self.delegate_to = delegate_to.to_string();
        self
    }

    /// Set the transaction fee.
    pub fn fee(mut self, fee: Currency) -> Self {
        self.fee = fee;
        self
    }

    /// Set an optional memo.
    pub fn memo(mut self, memo: &str) -> Self {
        self.memo = Some(memo.to_string());
        self
    }

    /// Set an explicit nonce (otherwise the daemon picks the next nonce).
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Send with a signature made outside the daemon.
    pub fn signature(mut self, signature: SignatureInput) -> Self {
        self.signature = Some(signature);
        self
    }
}

/// A pooled user command from the transaction pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PooledUserCommand {
    pub id: String,
    pub hash: String,
    pub kind: String,
    pub nonce: String,
    pub amount: String,
    pub fee: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub receiver: String,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub failure_reason: Option<String>,
}
