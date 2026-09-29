//! Integration tests against a running daemon's ITN GraphQL server
//! (feature `itn`).
//!
//! These tests require:
//! - MINA_ITN_URI: ITN GraphQL endpoint (e.g. http://127.0.0.1:3086/graphql)
//! - MINA_ITN_KEY: base64 ed25519 seed whose public key is in the daemon's
//!   `--itn-keys`
//!
//! Skip if environment variables are not set. None of them stops the daemon
//! or sends transactions.
#![cfg(feature = "itn")]

use std::env;

use mina_sdk::itn::*;
use mina_sdk::Error;

fn itn() -> Option<(String, ItnKey)> {
    let uri = env::var("MINA_ITN_URI").ok().filter(|s| !s.is_empty())?;
    let key = env::var("MINA_ITN_KEY").ok().filter(|s| !s.is_empty())?;
    Some((uri, ItnKey::from_base64(&key).expect("MINA_ITN_KEY")))
}

#[tokio::test]
async fn test_itn_auth() {
    let Some((uri, key)) = itn() else { return };
    let auth = ItnClient::new(&uri, key).auth().await.unwrap();
    assert!(!auth.server_uuid.is_empty());
    assert!(auth.libp2p_port > 0);
}

#[tokio::test]
async fn test_itn_unknown_key_is_unauthorized() {
    let Some((uri, _)) = itn() else { return };
    let err = ItnClient::new(&uri, ItnKey::generate())
        .auth()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ItnUnauthorized { .. }), "{err}");
}

/// Several sequenced requests in a row: each must carry the next number.
#[tokio::test]
async fn test_itn_sequenced_requests() {
    let Some((uri, key)) = itn() else { return };
    let client = ItnClient::new(&uri, key);
    for _ in 0..3 {
        client.internal_logs(0).await.unwrap();
    }
    let limit = client.set_zkapp_command_limit(Some(7)).await.unwrap();
    assert_eq!(limit, Some(7));
    let limit = client.set_zkapp_command_limit(None).await.unwrap();
    assert_eq!(limit, None);
}

/// Two clients with the same key share the daemon's sequence number: after
/// the second one sends, the first one's number is stale (412) and it must
/// recover with a new auth.
#[tokio::test]
async fn test_itn_recovers_from_stale_sequence_number() {
    let Some((uri, key)) = itn() else { return };
    let a = ItnClient::new(&uri, key.clone());
    let b = ItnClient::new(&uri, key);
    a.internal_logs(0).await.unwrap();
    b.internal_logs(0).await.unwrap();
    a.internal_logs(0).await.unwrap();
}

#[tokio::test]
async fn test_itn_internal_logs_and_flush() {
    let Some((uri, key)) = itn() else { return };
    let client = ItnClient::new(&uri, key);
    let logs = client.internal_logs(0).await.unwrap();
    if let Some(last) = logs.last() {
        client.flush_internal_logs(last.id).await.unwrap();
        let rest = client.internal_logs(0).await.unwrap();
        assert!(rest.iter().all(|l| l.id > last.id));
    }
}

/// A block producer answers with its slots, or with a GraphQL error while
/// its VRF evaluation runs; a node that is not a block producer answers
/// with a GraphQL error too. Anything else is a transport or schema problem.
#[tokio::test]
async fn test_itn_slots_won() {
    let Some((uri, key)) = itn() else { return };
    match ItnClient::new(&uri, key).slots_won().await {
        Ok(_) | Err(Error::Graphql { .. }) => {}
        Err(e) => panic!("{e}"),
    }
}

/// An empty update keeps the node's peers; it checks that the input type
/// matches the daemon's `GatingUpdate`.
#[tokio::test]
async fn test_itn_update_gating_empty() {
    let Some((uri, key)) = itn() else { return };
    ItnClient::new(&uri, key)
        .update_gating(&GatingUpdate::default())
        .await
        .unwrap();
}
