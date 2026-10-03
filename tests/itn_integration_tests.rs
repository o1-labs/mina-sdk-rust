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

// Operations for harness support (MinaProtocol/mina#19616). They run only with
// MINA_ITN_HARNESS=1, because older daemons do not have them.
fn harness() -> Option<ItnClient> {
    let (uri, key) = itn()?;
    (env::var("MINA_ITN_HARNESS").as_deref() == Ok("1")).then(|| ItnClient::new(&uri, key))
}

/// A random (version 4) UUID.
fn new_uuid() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

#[tokio::test]
async fn test_itn_commit_id_and_listing() {
    let Some(c) = harness() else { return };
    assert!(c.commit_id().await.unwrap().len() >= 7);
    c.scheduled_transactions().await.unwrap();
}

/// createAccounts sends transactions, so it also needs MINA_ITN_FEE_PAYER: the
/// base58 private key of a funded account.
#[tokio::test]
async fn test_itn_create_accounts() {
    let Some(c) = harness() else { return };
    let Ok(fee_payer) = env::var("MINA_ITN_FEE_PAYER") else {
        return;
    };
    let handle = new_uuid();
    let details = CreateAccountsDetails {
        fee_payer,
        num_accounts: 3,
        fee: mina_sdk::Currency::from_mina("0.1").unwrap(),
        amount: mina_sdk::Currency::from_mina("6").unwrap(),
    };
    let created = c.create_accounts(&details, Some(&handle)).await.unwrap();
    assert_eq!(created.handle, handle);
    assert_eq!(created.accounts.len(), 3);
    let again = c.create_accounts(&details, Some(&handle)).await.unwrap();
    assert_eq!(again.accounts[0], created.accounts[0]);
    for _ in 0..120 {
        if !c.scheduled_transactions().await.unwrap().contains(&handle) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    panic!("handle {handle} still listed after 10 minutes");
}
