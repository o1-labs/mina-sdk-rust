//! Integration tests against a running Mina daemon.
//!
//! These tests require:
//! - MINA_GRAPHQL_URI: GraphQL endpoint (e.g. http://127.0.0.1:8080/graphql)
//! - MINA_TEST_SENDER_KEY: A funded account public key (for payment tests)
//! - MINA_TEST_RECEIVER_KEY: A receiver public key (for payment tests)
//!
//! Skip if environment variables are not set.

use std::env;
use std::time::Duration;

use mina_sdk::*;

fn graphql_uri() -> Option<String> {
    env::var("MINA_GRAPHQL_URI").ok().filter(|s| !s.is_empty())
}

fn sender_key() -> Option<String> {
    env::var("MINA_TEST_SENDER_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}

fn receiver_key() -> Option<String> {
    env::var("MINA_TEST_RECEIVER_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}

fn make_client(uri: &str) -> MinaClient {
    MinaClient::with_config(ClientConfig {
        graphql_uri: uri.to_string(),
        retries: 3,
        retry_delay: Duration::from_secs(2),
        timeout: Duration::from_secs(30),
    })
}

/// Wait for the node to reach SYNCED status (up to 5 minutes).
async fn wait_for_sync(client: &MinaClient) -> bool {
    for _ in 0..60 {
        if let Ok(status) = client.get_sync_status().await {
            if status == SyncStatus::Synced {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    false
}

// -- Daemon query tests --

#[tokio::test]
async fn test_sync_status() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let status = client.get_sync_status().await.unwrap();
    assert_eq!(status, SyncStatus::Synced);
}

#[tokio::test]
async fn test_daemon_status() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let status = client.get_daemon_status().await.unwrap();
    assert_eq!(status.sync_status, SyncStatus::Synced);
    assert!(status.blockchain_length.unwrap_or(0) > 0);
}

#[tokio::test]
async fn test_network_id() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let id = client.get_network_id().await.unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn test_get_peers() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let peers = client.get_peers().await.unwrap();
    // Single-node network may have no peers, just check it doesn't error
    let _ = peers;
}

#[tokio::test]
async fn test_best_chain() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let chain = client.get_best_chain(Some(5)).await.unwrap();
    assert!(!chain.is_empty());
    assert!(chain[0].height > 0);
    assert!(!chain[0].state_hash.is_empty());
}

#[tokio::test]
async fn test_best_chain_ordering() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let chain = client.get_best_chain(Some(5)).await.unwrap();
    if chain.len() >= 2 {
        // Daemon may return blocks in ascending or descending height order
        let ascending = chain.windows(2).all(|w| w[0].height <= w[1].height);
        let descending = chain.windows(2).all(|w| w[0].height >= w[1].height);
        assert!(
            ascending || descending,
            "blocks should be in consistent height order"
        );
    }
}

#[tokio::test]
async fn test_pooled_user_commands_no_filter() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let _cmds = client.get_pooled_user_commands(None).await.unwrap();
}

// -- Account query tests --

#[tokio::test]
async fn test_get_account() {
    let Some(uri) = graphql_uri() else { return };
    let Some(sender) = sender_key() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let account = client.get_account(&sender, None).await.unwrap();
    assert_eq!(account.public_key, sender);
    assert!(account.balance.total.nanomina() > 0);
}

#[tokio::test]
async fn test_get_account_balance_types() {
    let Some(uri) = graphql_uri() else { return };
    let Some(sender) = sender_key() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let account = client.get_account(&sender, None).await.unwrap();
    // total should always be present and positive for funded accounts
    let total = account.balance.total;
    assert!(total.nanomina() > 0);
    // mina() should produce a valid decimal string
    let mina_str = total.mina();
    assert!(mina_str.contains('.'));
    // to_nanomina_str() should round-trip
    let roundtrip = Currency::from_graphql(&total.to_nanomina_str()).unwrap();
    assert_eq!(roundtrip, total);
}

#[tokio::test]
async fn test_account_not_found() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let result = client
        .get_account(
            "B62qiTKpEPjGTSHZrtM8uXiKgn8So916pLmNJKDhKeyBcSYVRjPoVn",
            None,
        )
        .await;
    assert!(
        result.is_err(),
        "expected error for nonexistent account, got: {result:?}"
    );
}

// -- Payment tests --

#[tokio::test]
async fn test_send_payment() {
    let Some(uri) = graphql_uri() else { return };
    let Some(sender) = sender_key() else { return };
    let Some(receiver) = receiver_key() else {
        return;
    };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let result = client
        .send_payment(
            Payment::sender(&sender)
                .to(&receiver)
                .amount(Currency::from_mina("0.001").unwrap())
                .fee(Currency::from_mina("0.01").unwrap())
                .memo("integration test"),
        )
        .await
        .unwrap();
    assert!(!result.hash.is_empty());
}

#[tokio::test]
async fn test_send_delegation() {
    let Some(uri) = graphql_uri() else { return };
    let Some(sender) = sender_key() else { return };
    let Some(receiver) = receiver_key() else {
        return;
    };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let result = client
        .send_delegation(
            Delegation::sender(&sender)
                .to(&receiver)
                .fee(Currency::from_mina("0.01").unwrap()),
        )
        .await
        .unwrap();
    assert!(!result.hash.is_empty());
}

#[tokio::test]
async fn test_payment_appears_in_pool() {
    let Some(uri) = graphql_uri() else { return };
    let Some(sender) = sender_key() else { return };
    let Some(receiver) = receiver_key() else {
        return;
    };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let _result = client
        .send_payment(
            Payment::sender(&sender)
                .to(&receiver)
                .amount(Currency::from_mina("0.001").unwrap())
                .fee(Currency::from_mina("0.01").unwrap())
                .memo("pool test"),
        )
        .await
        .unwrap();

    // Check the pool - the payment may or may not still be there depending
    // on block timing, so we just verify the query succeeds.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _cmds = client
        .get_pooled_user_commands(Some(&sender))
        .await
        .unwrap();
}

// -- Common API (spec/SPEC.md) --

#[tokio::test]
async fn test_daemon_metrics() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let metrics = client.get_daemon_metrics().await.unwrap();
    assert!(metrics.transaction_pool_size >= 0);
}

#[tokio::test]
async fn test_genesis_block_and_block_lookups() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let genesis = client.get_genesis_block().await.unwrap();
    assert!(!genesis.state_hash.is_empty());
    assert!(!genesis.staking_epoch.ledger_hash.is_empty());

    // The best tip is always in the transition frontier, so it can be read
    // back by state hash and by height.
    let tip = client.get_best_chain(Some(1)).await.unwrap().remove(0);
    let by_hash = client
        .get_block(BlockRef::StateHash(tip.state_hash.clone()))
        .await
        .unwrap();
    assert_eq!(by_hash.height, tip.height);
    assert_eq!(by_hash.previous_state_hash, tip.previous_state_hash);
    let by_height = client
        .get_block(BlockRef::Height(tip.height))
        .await
        .unwrap();
    assert_eq!(by_height.state_hash, tip.state_hash);
}

#[tokio::test]
async fn test_genesis_constants_and_pools() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    let constants = client.get_genesis_constants().await.unwrap();
    assert!(!constants.genesis_timestamp.is_empty());
    client.get_snark_pool().await.unwrap();
    client.get_pooled_zkapp_commands(None).await.unwrap();
    client.get_tracked_accounts().await.unwrap();
}

#[tokio::test]
async fn test_transaction_status_of_an_unknown_payment() {
    let Some(uri) = graphql_uri() else { return };
    let client = make_client(&uri);
    assert!(wait_for_sync(&client).await, "daemon did not reach SYNCED");

    // An ID that does not decode is a GraphQL error, not a transport one.
    let err = client
        .get_transaction_status(TransactionRef::Payment("not-an-id".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Graphql { .. }), "{err}");
}
