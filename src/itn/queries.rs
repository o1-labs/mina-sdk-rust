//! GraphQL documents for the daemon's ITN server.
//!
//! They are the documents of `spec/itn-operations.graphql` (a copy of
//! [mina-sdk-spec](https://github.com/o1-labs/mina-sdk-spec), whose CI
//! validates them against the daemon's `schema_itn`); a test checks that they
//! stay identical. Use them with
//! [`ItnClient::execute_query`](super::ItnClient::execute_query) for custom selections.

/// Server UUID and the signer's sequence number; the handshake before any
/// sequenced request. The only operation that accepts an unsequenced signature.
pub const AUTH: &str = r#"
query Auth {
  auth {
    serverUuid
    signerSequenceNumber
    libp2pPort
    peerId
    isBlockProducer
  }
}
"#;

/// Global slots the node's block producer keys won in the current epoch.
pub const SLOTS_WON: &str = r#"
query SlotsWon {
  slotsWon
}
"#;

/// Internal logs with an ID of at least `$startLogId`.
pub const INTERNAL_LOGS: &str = r#"
query InternalLogs($startLogId: Int!) {
  internalLogs(startLogId: $startLogId) {
    id
    timestamp
    message
    metadata {
      item
      value
    }
    process
  }
}
"#;

/// Drop internal logs up to and including `$endLogId`.
pub const FLUSH_INTERNAL_LOGS: &str = r#"
mutation FlushInternalLogs($endLogId: Int!) {
  flushInternalLogs(endLogId: $endLogId)
}
"#;

/// Start sending payments; returns a handle for `stopScheduledTransactions`.
pub const SCHEDULE_PAYMENTS: &str = r#"
mutation SchedulePayments($input: PaymentsDetails!) {
  schedulePayments(input: $input)
}
"#;

/// Start sending zkApp commands; returns a handle for `stopScheduledTransactions`.
pub const SCHEDULE_ZKAPP_COMMANDS: &str = r#"
mutation ScheduleZkappCommands($input: ZkappCommandsDetails!) {
  scheduleZkappCommands(input: $input)
}
"#;

/// Stop payments or zkApp commands started by a schedule mutation.
pub const STOP_SCHEDULED_TRANSACTIONS: &str = r#"
mutation StopScheduledTransactions($handle: String!) {
  stopScheduledTransactions(handle: $handle)
}
"#;

/// Change the node's connection gating: added, trusted and banned peers.
pub const UPDATE_GATING: &str = r#"
mutation UpdateGating($input: GatingUpdate!) {
  updateGating(input: $input)
}
"#;

/// Stop the daemon after `$delaySeconds`, optionally deleting its config directory.
pub const STOP_DAEMON: &str = r#"
mutation StopDaemon($delaySeconds: Int, $cleanConfig: Boolean) {
  stopDaemon(delaySeconds: $delaySeconds, cleanConfig: $cleanConfig)
}
"#;

/// Set the block producer's limit of zkApp commands per block; `null` removes it.
pub const ZKAPP_COMMAND_LIMIT: &str = r#"
mutation ZkappCommandLimit($limit: Int) {
  zkAppCommandLimit(limit: $limit)
}
"#;
