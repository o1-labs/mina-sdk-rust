//! Tests of the ITN client against a mock server (feature `itn`). Each test
//! checks the `Authorization` header the way the daemon does
//! (`graphql_internal.ml`): the signature must verify with the public key
//! over the body, or over `be_u16(seq) || uuid || body` for a sequenced
//! request.
#![cfg(feature = "itn")]

use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::{json, Value};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use mina_sdk::itn::*;
use mina_sdk::{ClientConfig, Currency, Error};

const UUID: &str = "5f0c2e6e-6a57-4bd4-9a8c-3a8f6c7e8b21";

fn auth_body(seq: u16) -> Value {
    json!({ "data": { "auth": {
        "serverUuid": UUID,
        "signerSequenceNumber": seq.to_string(),
        "libp2pPort": "8302",
        "peerId": "12D3KooWGCh9hCWdBjXp7dvxhhPKoa264Ny3BByoCinc9gyDEWNF",
        "isBlockProducer": true,
    } } })
}

fn client(server: &MockServer, key: ItnKey) -> ItnClient {
    ItnClient::with_config(
        ClientConfig {
            graphql_uri: format!("{}/graphql", server.uri()),
            retries: 2,
            retry_delay: Duration::from_millis(10),
            timeout: Duration::from_secs(5),
        },
        key,
    )
}

/// Mount an `auth` answer with sequence number `seq`.
async fn mount_auth(server: &MockServer, seq: u16) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("auth"))
        .respond_with(ResponseTemplate::new(200).set_body_json(auth_body(seq)))
        .mount(server)
        .await;
}

/// The parsed `Authorization` header: (pk, sig, Some((uuid, seq))).
fn header(req: &Request) -> (String, String, Option<(String, u16)>) {
    let h = req.headers.get("authorization").unwrap().to_str().unwrap();
    let parts: Vec<&str> = h.split(' ').collect();
    match parts.as_slice() {
        ["Signature", pk, sig] => (pk.to_string(), sig.to_string(), None),
        ["Signature", pk, sig, ";", "Sequencing", uuid, seq] => (
            pk.to_string(),
            sig.to_string(),
            Some((uuid.to_string(), seq.parse().unwrap())),
        ),
        _ => panic!("unexpected Authorization header: {h}"),
    }
}

/// Verify the request's signature as the daemon does; return (uuid, seq).
fn verify(req: &Request, key: &ItnKey) -> Option<(String, u16)> {
    let (pk, sig, seq) = header(req);
    assert_eq!(pk, key.public_key_base64());
    let pk: [u8; 32] = BASE64.decode(pk).unwrap().try_into().unwrap();
    let sig: [u8; 64] = BASE64.decode(sig).unwrap().try_into().unwrap();
    let msg = match &seq {
        None => req.body.clone(),
        Some((uuid, n)) => [&n.to_be_bytes()[..], uuid.as_bytes(), &req.body].concat(),
    };
    VerifyingKey::from_bytes(&pk)
        .unwrap()
        .verify(&msg, &Signature::from_bytes(&sig))
        .expect("signature must verify over the daemon's message");
    seq
}

fn is_auth(req: &Request) -> bool {
    String::from_utf8_lossy(&req.body).contains("auth")
}

#[tokio::test]
async fn test_auth_is_unsequenced_and_signed_over_body() {
    let server = MockServer::start().await;
    mount_auth(&server, 4).await;
    let key = ItnKey::generate();
    let auth = client(&server, key.clone()).auth().await.unwrap();

    assert_eq!(auth.server_uuid, UUID);
    assert_eq!(auth.signer_sequence_number, 4);
    assert_eq!(auth.libp2p_port, 8302);
    assert!(auth.is_block_producer);
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(verify(&reqs[0], &key), None);
}

#[tokio::test]
async fn test_sequenced_requests_start_at_auth_number_and_count_up() {
    let server = MockServer::start().await;
    mount_auth(&server, 41).await;
    Mock::given(method("POST"))
        .and(body_string_contains("internalLogs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "internalLogs": [
            { "id": 3, "timestamp": "2026-09-29T08:00:00Z", "message": "Block produced",
              "metadata": [ { "item": "height", "value": 12 } ], "process": null }
        ] } })),
        )
        .mount(&server)
        .await;
    let key = ItnKey::generate();
    let c = client(&server, key.clone());

    let logs = c.internal_logs(0).await.unwrap();
    c.internal_logs(4).await.unwrap();

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].id, 3);
    assert_eq!(logs[0].metadata, vec![("height".to_string(), json!(12))]);
    let reqs = server.received_requests().await.unwrap();
    let seqs: Vec<_> = reqs
        .iter()
        .filter(|r| !is_auth(r))
        .map(|r| verify(r, &key).expect("sequenced"))
        .collect();
    assert_eq!(seqs, vec![(UUID.to_string(), 41), (UUID.to_string(), 42)]);
    assert_eq!(reqs.iter().filter(|r| is_auth(r)).count(), 1);
}

/// The daemon counts a request once it accepted the signature, even when
/// the GraphQL result is an error; the client must count it too.
#[tokio::test]
async fn test_graphql_error_still_uses_up_the_sequence_number() {
    let server = MockServer::start().await;
    mount_auth(&server, 0).await;
    Mock::given(method("POST"))
        .and(body_string_contains("slotsWon"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({ "errors": [ { "message": "Not a block producing node" } ] }),
            ),
        )
        .mount(&server)
        .await;
    let key = ItnKey::generate();
    let c = client(&server, key.clone());

    assert!(matches!(c.slots_won().await, Err(Error::Graphql { .. })));
    assert!(matches!(c.slots_won().await, Err(Error::Graphql { .. })));

    let reqs = server.received_requests().await.unwrap();
    let seqs: Vec<u16> = reqs
        .iter()
        .filter(|r| !is_auth(r))
        .map(|r| verify(r, &key).unwrap().1)
        .collect();
    assert_eq!(seqs, vec![0, 1]);
}

#[tokio::test]
async fn test_412_runs_auth_again_and_repeats_once() {
    let server = MockServer::start().await;
    mount_auth(&server, 9).await;
    Mock::given(method("POST"))
        .and(body_string_contains("flushInternalLogs"))
        .respond_with(ResponseTemplate::new(412).set_body_string("Invalid sequence number"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("flushInternalLogs"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "flushInternalLogs": "5" } })),
        )
        .mount(&server)
        .await;
    let c = client(&server, ItnKey::generate());

    assert_eq!(c.flush_internal_logs(5).await.unwrap(), "5");
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.iter().filter(|r| is_auth(r)).count(), 2);
    assert_eq!(reqs.iter().filter(|r| !is_auth(r)).count(), 2);
}

#[tokio::test]
async fn test_412_twice_is_an_error() {
    let server = MockServer::start().await;
    mount_auth(&server, 0).await;
    Mock::given(method("POST"))
        .and(body_string_contains("stopScheduledTransactions"))
        .respond_with(ResponseTemplate::new(412))
        .mount(&server)
        .await;
    let err = client(&server, ItnKey::generate())
        .stop_scheduled_transactions("h")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ItnSequencing { .. }), "{err}");
}

#[tokio::test]
async fn test_401_is_unauthorized_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;
    let err = client(&server, ItnKey::generate())
        .auth()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ItnUnauthorized { .. }), "{err}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// A sequenced mutation that fails in transport is not repeated (the daemon
/// may have run it), and the next request starts a new session.
#[tokio::test]
async fn test_failed_mutation_is_not_repeated_and_session_is_dropped() {
    let server = MockServer::start().await;
    mount_auth(&server, 0).await;
    Mock::given(method("POST"))
        .and(body_string_contains("schedulePayments"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let c = client(&server, ItnKey::generate());
    let details = PaymentsDetails {
        duration_min: 1,
        tps: 0.5,
        memo_prefix: "t".into(),
        max_fee: Currency::from_nanomina(2),
        min_fee: Currency::from_nanomina(1),
        amount: Currency::from_nanomina(1_000),
        receiver: "B62qrPN5Y5yq8kGE3FbVKbGTdTAJNdtNtB5sNVpxyRwWGcDEhpMzc8g".into(),
        senders: vec!["EKE...".into()],
    };

    assert!(matches!(
        c.schedule_payments(&details).await,
        Err(Error::Connection { attempts: 1, .. })
    ));
    let _ = c.schedule_payments(&details).await;
    let reqs = server.received_requests().await.unwrap();
    let kinds: Vec<bool> = reqs.iter().map(is_auth).collect();
    assert_eq!(kinds, vec![true, false, true, false]);
    let input = &serde_json::from_slice::<Value>(&reqs[1].body).unwrap()["variables"]["input"];
    assert_eq!(input["amount"], "1000");
    assert_eq!(input["maxFee"], "2");
}

#[tokio::test]
async fn test_sequence_number_wraps() {
    let server = MockServer::start().await;
    mount_auth(&server, u16::MAX).await;
    Mock::given(method("POST"))
        .and(body_string_contains("zkAppCommandLimit"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "zkAppCommandLimit": null } })),
        )
        .mount(&server)
        .await;
    let key = ItnKey::generate();
    let c = client(&server, key.clone());
    assert_eq!(c.set_zkapp_command_limit(None).await.unwrap(), None);
    assert_eq!(c.set_zkapp_command_limit(None).await.unwrap(), None);
    let reqs = server.received_requests().await.unwrap();
    let seqs: Vec<u16> = reqs
        .iter()
        .filter(|r| !is_auth(r))
        .map(|r| verify(r, &key).unwrap().1)
        .collect();
    assert_eq!(seqs, vec![u16::MAX, 0]);
}

#[test]
fn test_key_encoding() {
    let key = ItnKey::generate();
    let again = ItnKey::from_base64(&format!(" {}\n", key.to_base64())).unwrap();
    assert_eq!(key.public_key_base64(), again.public_key_base64());
    assert_eq!(BASE64.decode(key.public_key_base64()).unwrap().len(), 32);
    assert!(matches!(
        ItnKey::from_base64("AAAA"),
        Err(Error::InvalidItnKey(_))
    ));
    assert!(matches!(
        ItnKey::from_base64("not base64!"),
        Err(Error::InvalidItnKey(_))
    ));
    let debug = format!("{key:?}");
    assert!(debug.contains(&key.public_key_base64()));
    assert!(!debug.contains(&key.to_base64()));
}

#[tokio::test]
async fn test_non_default_token_is_sent_only_when_set() {
    let server = MockServer::start().await;
    mount_auth(&server, 0).await;
    Mock::given(method("POST"))
        .and(body_string_contains("scheduleZkappCommands"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "scheduleZkappCommands": "h1" } })),
        )
        .mount(&server)
        .await;
    let c = client(&server, ItnKey::generate());
    let mut d = ZkappCommandsDetails {
        max_account_updates: None,
        max_cost: false,
        account_queue_size: 10,
        deployment_fee: Currency::from_nanomina(1),
        max_fee: Currency::from_nanomina(2),
        min_fee: Currency::from_nanomina(1),
        init_balance: Currency::from_nanomina(3),
        max_new_zkapp_balance: Currency::from_nanomina(4),
        min_new_zkapp_balance: Currency::from_nanomina(1),
        max_balance_change: Currency::from_nanomina(5),
        min_balance_change: Currency::from_nanomina(0),
        no_precondition: false,
        memo_prefix: "z".into(),
        duration_min: 1,
        tps: 0.1,
        num_new_accounts: 0,
        num_zkapps_to_deploy: 1,
        fee_payers: vec![],
        non_default_token: None,
    };
    assert_eq!(c.schedule_zkapp_commands(&d).await.unwrap(), "h1");
    d.non_default_token = Some(true);
    c.schedule_zkapp_commands(&d).await.unwrap();

    let inputs: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| !is_auth(r))
        .map(|r| serde_json::from_slice::<Value>(&r.body).unwrap()["variables"]["input"].clone())
        .collect();
    assert!(inputs[0].get("nonDefaultToken").is_none());
    assert_eq!(inputs[1]["nonDefaultToken"], true);
}

/// The operations for harness support (MinaProtocol/mina#19616): parsing of
/// the results, and the variables sent (`handle: null` when it is omitted).
#[tokio::test]
async fn test_harness_support_operations() {
    let server = MockServer::start().await;
    let key = ItnKey::generate();
    // CommitId selects `auth` too, so it needs a higher priority than the
    // handshake mock.
    Mock::given(method("POST"))
        .and(body_string_contains("commitId"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "auth": { "commitId": "abc123" } } })),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    mount_auth(&server, 0).await;
    for (needle, data) in [
        (
            "scheduledTransactions",
            json!({ "scheduledTransactions": ["h1", "h2"] }),
        ),
        (
            "createAccounts",
            json!({ "createAccounts": {
                "handle": "h3",
                "accounts": [ { "publicKey": "B62qa", "privateKey": "EKa" } ] } }),
        ),
        ("schedulePayments", json!({ "schedulePayments": "h4" })),
        (
            "scheduleZkappCommands",
            json!({ "scheduleZkappCommands": "h5" }),
        ),
    ] {
        Mock::given(method("POST"))
            .and(body_string_contains(needle))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": data })))
            .with_priority(2)
            .mount(&server)
            .await;
    }
    let c = client(&server, key.clone());
    assert_eq!(c.commit_id().await.unwrap(), "abc123");
    assert_eq!(c.scheduled_transactions().await.unwrap(), ["h1", "h2"]);
    let details = CreateAccountsDetails {
        fee_payer: "EKfee".into(),
        num_accounts: 2,
        fee: Currency::from_nanomina(100),
        amount: Currency::from_nanomina(5000),
    };
    let created = c.create_accounts(&details, None).await.unwrap();
    assert_eq!(created.handle, "h3");
    assert_eq!(created.accounts[0].private_key, "EKa");
    c.create_accounts(&details, Some("u1")).await.unwrap();
    let payments = PaymentsDetails {
        duration_min: 1,
        tps: 0.5,
        memo_prefix: "m".into(),
        max_fee: Currency::from_nanomina(20),
        min_fee: Currency::from_nanomina(10),
        amount: Currency::from_nanomina(1),
        receiver: "B62qr".into(),
        senders: vec![],
    };
    assert_eq!(
        c.schedule_payments_with_handle(&payments, "u2")
            .await
            .unwrap(),
        "h4"
    );

    let vars: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| verify(r, &key).is_some())
        .map(|r| serde_json::from_slice::<Value>(&r.body).unwrap()["variables"].clone())
        .collect();
    assert_eq!(vars[2]["handle"], Value::Null);
    assert_eq!(
        vars[2]["input"],
        json!({ "feePayer": "EKfee", "numAccounts": 2, "fee": "100", "amount": "5000" })
    );
    assert_eq!(vars[3]["handle"], "u1");
    assert_eq!(vars[4]["handle"], "u2");
}
