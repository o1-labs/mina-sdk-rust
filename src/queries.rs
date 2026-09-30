//! GraphQL query and mutation strings for the Mina daemon API.
//!
//! These are the documents of `spec/operations.graphql`, the common API of
//! the Mina SDKs (see `spec/SPEC.md`). `spec/` is a copy of
//! [mina-sdk-spec](https://github.com/o1-labs/mina-sdk-spec) at the tag in
//! `spec/VERSION`; `tests/spec_tests.rs` checks that these documents stay
//! identical. Change the specification first.

/// Node sync status.
pub const SYNC_STATUS: &str = r#"query SyncStatus {
  syncStatus
}"#;

/// Daemon status: sync, chain length, peers, addresses, keys.
pub const DAEMON_STATUS: &str = r#"query DaemonStatus {
  daemonStatus {
    syncStatus
    blockchainLength
    highestBlockLengthReceived
    highestUnvalidatedBlockLengthReceived
    uptimeSecs
    stateHash
    commitId
    numAccounts
    ledgerMerkleRoot
    chainId
    catchupStatus
    blockProductionKeys
    coinbaseReceiver
    peers {
      peerId
      host
      libp2pPort
    }
    addrsAndPorts {
      externalIp
      bindIp
      clientPort
      libp2pPort
    }
  }
}"#;

/// Daemon metrics: transaction and snark pools, block production delay.
pub const DAEMON_METRICS: &str = r#"query DaemonMetrics {
  daemonStatus {
    metrics {
      blockProductionDelay
      transactionPoolDiffReceived
      transactionPoolDiffBroadcasted
      transactionsAddedToPool
      transactionPoolSize
      snarkPoolDiffReceived
      snarkPoolDiffBroadcasted
      pendingSnarkWork
      snarkPoolSize
    }
  }
}"#;

/// Network identifier.
pub const NETWORK_ID: &str = r#"query NetworkId {
  networkID
}"#;

/// One account; `$token` is null for the default MINA token.
pub const GET_ACCOUNT: &str = r#"query Account($publicKey: PublicKey!, $token: TokenId) {
  account(publicKey: $publicKey, token: $token) {
    publicKey
    nonce
    delegate
    tokenId
    tokenSymbol
    votingFor
    receiptChainHash
    balance {
      total
      liquid
      locked
      blockHeight
    }
    timing {
      initialMinimumBalance
      cliffTime
      cliffAmount
      vestingPeriod
      vestingIncrement
    }
    permissions {
      editState
      send
      receive
      access
      setDelegate
      setPermissions
      setVerificationKey {
        auth
        txnVersion
      }
      setZkappUri
      editActionState
      setTokenSymbol
      incrementNonce
      setVotingFor
      setTiming
    }
    zkappState
    provedState
    zkappUri
  }
}"#;

/// The same document as [`GET_ACCOUNT`], which now takes the token too.
#[deprecated(since = "0.3.0", note = "use GET_ACCOUNT; its $token is optional")]
pub const GET_ACCOUNT_WITH_TOKEN: &str = GET_ACCOUNT;

/// Blocks of the best chain.
pub const BEST_CHAIN: &str = r#"query BestChain($maxLength: Int) {
  bestChain(maxLength: $maxLength) {
    stateHash
    commandTransactionCount
    creatorAccount {
      publicKey
    }
    protocolState {
      previousStateHash
      consensusState {
        blockHeight
        epoch
        slot
        slotSinceGenesis
        blockCreator
        coinbaseReceiever
        stakingEpochData {
          epochLength
          seed
          ledger {
            hash
          }
        }
        nextEpochData {
          seed
          ledger {
            hash
          }
        }
      }
      blockchainState {
        date
        utcDate
        snarkedLedgerHash
        stagedLedgerHash
      }
    }
    transactions {
      coinbase
      coinbaseReceiverAccount {
        publicKey
      }
      feeTransfer {
        recipient
        fee
        type
      }
      userCommands {
        id
        hash
        kind
        nonce
        source {
          publicKey
        }
        receiver {
          publicKey
        }
        amount
        fee
        memo
        failureReason
      }
    }
  }
}"#;

/// The genesis block.
pub const GENESIS_BLOCK: &str = r#"query GenesisBlock {
  genesisBlock {
    stateHash
    commandTransactionCount
    creatorAccount {
      publicKey
    }
    protocolState {
      previousStateHash
      consensusState {
        blockHeight
        epoch
        slot
        slotSinceGenesis
        blockCreator
        coinbaseReceiever
        stakingEpochData {
          epochLength
          seed
          ledger {
            hash
          }
        }
        nextEpochData {
          seed
          ledger {
            hash
          }
        }
      }
      blockchainState {
        date
        utcDate
        snarkedLedgerHash
        stagedLedgerHash
      }
    }
    transactions {
      coinbase
      coinbaseReceiverAccount {
        publicKey
      }
      feeTransfer {
        recipient
        fee
        type
      }
      userCommands {
        id
        hash
        kind
        nonce
        source {
          publicKey
        }
        receiver {
          publicKey
        }
        amount
        fee
        memo
        failureReason
      }
    }
  }
}"#;

/// One block, by exactly one of `$stateHash` and `$height`.
pub const BLOCK: &str = r#"query Block($stateHash: String, $height: Int) {
  block(stateHash: $stateHash, height: $height) {
    stateHash
    commandTransactionCount
    creatorAccount {
      publicKey
    }
    protocolState {
      previousStateHash
      consensusState {
        blockHeight
        epoch
        slot
        slotSinceGenesis
        blockCreator
        coinbaseReceiever
        stakingEpochData {
          epochLength
          seed
          ledger {
            hash
          }
        }
        nextEpochData {
          seed
          ledger {
            hash
          }
        }
      }
      blockchainState {
        date
        utcDate
        snarkedLedgerHash
        stagedLedgerHash
      }
    }
    transactions {
      coinbase
      coinbaseReceiverAccount {
        publicKey
      }
      feeTransfer {
        recipient
        fee
        type
      }
      userCommands {
        id
        hash
        kind
        nonce
        source {
          publicKey
        }
        receiver {
          publicKey
        }
        amount
        fee
        memo
        failureReason
      }
    }
  }
}"#;

/// Connected peers.
pub const GET_PEERS: &str = r#"query Peers {
  getPeers {
    peerId
    host
    libp2pPort
  }
}"#;

/// Pending user commands; `$publicKey` is null for every sender.
pub const POOLED_USER_COMMANDS: &str = r#"query PooledUserCommands($publicKey: PublicKey) {
  pooledUserCommands(publicKey: $publicKey) {
    id
    hash
    kind
    nonce
    amount
    fee
    from
    to
    source {
      publicKey
    }
    receiver {
      publicKey
    }
    memo
    failureReason
  }
}"#;

/// Pending zkApp commands; `$publicKey` is null for every fee payer.
pub const POOLED_ZKAPP_COMMANDS: &str = r#"query PooledZkappCommands($publicKey: PublicKey) {
  pooledZkappCommands(publicKey: $publicKey) {
    id
    hash
    zkappCommand {
      memo
      feePayer {
        body {
          publicKey
          fee
          nonce
          validUntil
        }
      }
    }
    failureReason {
      index
      failures
    }
  }
}"#;

/// Status of a transaction, by exactly one of `$payment` and `$zkappTransaction`.
pub const TRANSACTION_STATUS: &str = r#"query TransactionStatus($payment: ID, $zkappTransaction: ID) {
  transactionStatus(payment: $payment, zkappTransaction: $zkappTransaction)
}"#;

/// Genesis timestamp, coinbase and account creation fee.
pub const GENESIS_CONSTANTS: &str = r#"query GenesisConstants {
  genesisConstants {
    genesisTimestamp
    coinbase
    accountCreationFee
  }
}"#;

/// Accounts the daemon tracks (its wallet keys).
pub const TRACKED_ACCOUNTS: &str = r#"query TrackedAccounts {
  trackedAccounts {
    publicKey
    balance {
      total
    }
  }
}"#;

/// Completed snark work with the lowest fees.
pub const SNARK_POOL: &str = r#"query SnarkPool {
  snarkPool {
    fee
    prover
    workIds
  }
}"#;

/// The daemon's fork configuration, a JSON value.
pub const FORK_CONFIG: &str = r#"query ForkConfig {
  fork_config
}"#;

/// Send a payment; `$signature` is null to let the daemon sign.
pub const SEND_PAYMENT: &str = r#"mutation SendPayment($input: SendPaymentInput!, $signature: SignatureInput) {
  sendPayment(input: $input, signature: $signature) {
    payment {
      id
      hash
      kind
      nonce
      source {
        publicKey
      }
      receiver {
        publicKey
      }
      amount
      fee
      memo
    }
  }
}"#;

/// Send a stake delegation; `$signature` is null to let the daemon sign.
pub const SEND_DELEGATION: &str = r#"mutation SendDelegation($input: SendDelegationInput!, $signature: SignatureInput) {
  sendDelegation(input: $input, signature: $signature) {
    delegation {
      id
      hash
      kind
      nonce
      source {
        publicKey
      }
      receiver {
        publicKey
      }
      amount
      fee
      memo
    }
  }
}"#;

/// Send a signed zkApp command.
pub const SEND_ZKAPP: &str = r#"mutation SendZkapp($input: SendZkappInput!) {
  sendZkapp(input: $input) {
    zkapp {
      id
      hash
      zkappCommand {
        memo
        feePayer {
          body {
            publicKey
            fee
            nonce
            validUntil
          }
        }
      }
      failureReason {
        index
        failures
      }
    }
  }
}"#;

/// Unlock an account in the daemon's keystore.
pub const UNLOCK_ACCOUNT: &str = r#"mutation UnlockAccount($input: UnlockInput!) {
  unlockAccount(input: $input) {
    publicKey
  }
}"#;

/// Set or unset the snark worker key.
pub const SET_SNARK_WORKER: &str = r#"mutation SetSnarkWorker($input: SetSnarkWorkerInput!) {
  setSnarkWorker(input: $input) {
    lastSnarkWorker
  }
}"#;

/// Set the snark work fee.
pub const SET_SNARK_WORK_FEE: &str = r#"mutation SetSnarkWorkFee($fee: UInt64!) {
  setSnarkWorkFee(input: {fee: $fee}) {
    lastFee
  }
}"#;
