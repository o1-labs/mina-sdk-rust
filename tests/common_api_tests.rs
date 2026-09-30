//! Tests of the common API methods and fields (spec/SPEC.md) against a mock
//! daemon: the variables each method sends and how it reads the answer.

use std::time::Duration;

use serde_json::{json, Value};
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

use mina_sdk::*;

/// A client and a mock daemon that answers `data` to the operation `op`.
async fn mock(op: &str, data: Value) -> (MinaClient, MockServer) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains(op.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": data })))
        .mount(&server)
        .await;
    let client = MinaClient::with_config(ClientConfig {
        graphql_uri: format!("{}/graphql", server.uri()),
        retries: 1,
        retry_delay: Duration::from_millis(1),
        timeout: Duration::from_secs(5),
    });
    (client, server)
}

/// The variables of the one request the server received.
async fn sent_variables(server: &MockServer) -> Value {
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    serde_json::from_slice::<Value>(&reqs[0].body).unwrap()["variables"].clone()
}

/// A block in the common block selection.
fn block_json() -> Value {
    json!({
        "stateHash": "3NKblock",
        "commandTransactionCount": 1,
        "creatorAccount": { "publicKey": "B62qcreator" },
        "protocolState": {
            "previousStateHash": "3NKprev",
            "consensusState": {
                "blockHeight": "12", "epoch": "1", "slot": "40", "slotSinceGenesis": "7180",
                "blockCreator": "B62qcreator", "coinbaseReceiever": "B62qcoinbase",
                "stakingEpochData": { "epochLength": "3", "seed": "2vseed", "ledger": { "hash": "jxstake" } },
                "nextEpochData": { "seed": "2vnext", "ledger": { "hash": "jxnext" } }
            },
            "blockchainState": {
                "date": "1727600000000", "utcDate": "1727600000000",
                "snarkedLedgerHash": "jxsnarked", "stagedLedgerHash": "jxstaged"
            }
        },
        "transactions": {
            "coinbase": "720000000000",
            "coinbaseReceiverAccount": { "publicKey": "B62qcoinbase" },
            "feeTransfer": [ { "recipient": "B62qsnarker", "fee": "1000000", "type": "Fee_transfer" } ],
            "userCommands": [ {
                "id": "Cmd1", "hash": "CkpHash", "kind": "PAYMENT", "nonce": "4",
                "source": { "publicKey": "B62qsrc" }, "receiver": { "publicKey": "B62qdst" },
                "amount": "5000000000", "fee": "10000000", "memo": "E4Y", "failureReason": null
            } ]
        }
    })
}

#[tokio::test]
async fn test_best_chain_reads_the_common_block_selection() {
    let (client, server) = mock("BestChain", json!({ "bestChain": [block_json()] })).await;
    let blocks = client.get_best_chain(None).await.unwrap();
    assert_eq!(sent_variables(&server).await, json!({ "maxLength": null }));

    let b = &blocks[0];
    assert_eq!(
        (b.height, b.epoch, b.global_slot_since_hard_fork),
        (12, 1, 40)
    );
    assert_eq!(b.previous_state_hash, "3NKprev");
    assert_eq!(b.coinbase_receiver.as_deref(), Some("B62qcoinbase"));
    assert_eq!(
        b.staking_epoch,
        EpochData {
            length: Some(3),
            seed: "2vseed".into(),
            ledger_hash: "jxstake".into()
        }
    );
    assert_eq!(b.next_epoch.ledger_hash, "jxnext");
    assert_eq!(b.coinbase, Currency::from_nanomina(720_000_000_000));
    assert_eq!(b.fee_transfers[0].fee, Currency::from_nanomina(1_000_000));
    let c = &b.user_commands[0];
    assert_eq!(
        (c.nonce, c.source.as_str(), c.receiver.as_str()),
        (4, "B62qsrc", "B62qdst")
    );
    assert_eq!(c.amount, Currency::from_nanomina(5_000_000_000));
}

#[tokio::test]
async fn test_genesis_block_and_block_by_hash_or_height() {
    let (client, _server) = mock("GenesisBlock", json!({ "genesisBlock": block_json() })).await;
    assert_eq!(
        client.get_genesis_block().await.unwrap().state_hash,
        "3NKblock"
    );

    let (client, server) = mock("query Block", json!({ "block": block_json() })).await;
    client.get_block(BlockRef::Height(12)).await.unwrap();
    assert_eq!(
        sent_variables(&server).await,
        json!({ "stateHash": null, "height": 12 })
    );

    let (client, server) = mock("query Block", json!({ "block": block_json() })).await;
    client
        .get_block(BlockRef::StateHash("3NKblock".into()))
        .await
        .unwrap();
    assert_eq!(
        sent_variables(&server).await,
        json!({ "stateHash": "3NKblock", "height": null })
    );
}

#[tokio::test]
async fn test_daemon_status_new_fields_and_metrics() {
    let (client, _s) = mock(
        "DaemonStatus",
        json!({ "daemonStatus": {
            "syncStatus": "SYNCED", "numAccounts": 5, "chainId": "abc",
            "blockProductionKeys": ["B62qbp"], "catchupStatus": null,
            "addrsAndPorts": { "externalIp": "1.2.3.4", "bindIp": "0.0.0.0", "clientPort": 8301, "libp2pPort": 8302 },
            "peers": []
        } }),
    )
    .await;
    let s = client.get_daemon_status().await.unwrap();
    assert_eq!(
        (s.num_accounts, s.chain_id.as_deref()),
        (Some(5), Some("abc"))
    );
    assert_eq!(s.block_production_keys, vec!["B62qbp".to_string()]);
    assert_eq!(s.addrs_and_ports.unwrap().client_port, 8301);

    let (client, _s) = mock(
        "DaemonMetrics",
        json!({ "daemonStatus": { "metrics": {
            "blockProductionDelay": [1, 2], "transactionPoolDiffReceived": 3,
            "transactionPoolDiffBroadcasted": 4, "transactionsAddedToPool": 5,
            "transactionPoolSize": 6, "snarkPoolDiffReceived": 7, "snarkPoolDiffBroadcasted": 8,
            "pendingSnarkWork": 9, "snarkPoolSize": 10
        } } }),
    )
    .await;
    let m = client.get_daemon_metrics().await.unwrap();
    assert_eq!(m.block_production_delay, vec![1, 2]);
    assert_eq!((m.transaction_pool_size, m.snark_pool_size), (6, 10));
}

#[tokio::test]
async fn test_account_sends_a_null_token_and_reads_new_fields() {
    let (client, server) = mock(
        "query Account",
        json!({ "account": {
            "publicKey": "B62qacc", "nonce": "3", "tokenId": "wSHV", "tokenSymbol": "MINA",
            "balance": { "total": "1000", "liquid": "900", "locked": "100", "blockHeight": "12" },
            "timing": { "initialMinimumBalance": null, "cliffTime": null, "cliffAmount": null,
                        "vestingPeriod": null, "vestingIncrement": null },
            "permissions": { "send": "Signature", "setVerificationKey": { "auth": "Signature", "txnVersion": "3" } },
            "zkappState": null, "provedState": false
        } }),
    )
    .await;
    let a = client.get_account("B62qacc", None).await.unwrap();
    assert_eq!(
        sent_variables(&server).await,
        json!({ "publicKey": "B62qacc", "token": null })
    );
    assert_eq!(a.balance.block_height, Some(12));
    assert_eq!(a.token_symbol.as_deref(), Some("MINA"));
    assert!(
        a.timing.is_none(),
        "an all-null timing is an untimed account"
    );
    let p = a.permissions.unwrap();
    assert_eq!(p.send.as_deref(), Some("Signature"));
    assert_eq!(
        p.set_verification_key,
        Some(("Signature".into(), "3".into()))
    );
    assert_eq!((a.zkapp_state, a.proved_state), (None, Some(false)));
}

#[tokio::test]
async fn test_pooled_commands() {
    let (client, _s) = mock(
        "PooledUserCommands",
        json!({ "pooledUserCommands": [ {
            "id": "c", "hash": "h", "kind": "PAYMENT", "nonce": "1", "amount": "5", "fee": "1",
            "from": "B62qa", "to": "B62qb", "source": { "publicKey": "B62qa" },
            "receiver": { "publicKey": "B62qb" }, "memo": "m", "failureReason": null
        } ] }),
    )
    .await;
    let c = &client.get_pooled_user_commands(None).await.unwrap()[0];
    assert_eq!(
        (c.source.as_str(), c.receiver.as_str(), c.memo.as_str()),
        ("B62qa", "B62qb", "m")
    );

    let (client, server) = mock(
        "PooledZkappCommands",
        json!({ "pooledZkappCommands": [ {
            "id": "z", "hash": "5Jz",
            "zkappCommand": { "memo": "E4Y", "feePayer": { "body": {
                "publicKey": "B62qpayer", "fee": "100000000", "nonce": "2", "validUntil": null } } },
            "failureReason": null
        } ] }),
    )
    .await;
    let z = &client
        .get_pooled_zkapp_commands(Some("B62qpayer"))
        .await
        .unwrap()[0];
    assert_eq!(
        sent_variables(&server).await,
        json!({ "publicKey": "B62qpayer" })
    );
    assert_eq!(z.fee_payer.fee, Currency::from_nanomina(100_000_000));
    assert_eq!((z.fee_payer.nonce, z.fee_payer.valid_until), (2, None));
    assert!(z.failure_reason.is_none());
}

#[tokio::test]
async fn test_send_payment_with_and_without_signature() {
    let submitted = json!({ "sendPayment": { "payment": {
        "id": "id", "hash": "h", "kind": "PAYMENT", "nonce": "7",
        "source": { "publicKey": "B62qa" }, "receiver": { "publicKey": "B62qb" },
        "amount": "5", "fee": "1", "memo": ""
    } } });
    let payment = || {
        Payment::sender("B62qa")
            .to("B62qb")
            .amount(Currency::from_nanomina(5))
            .fee(Currency::from_nanomina(1))
    };

    let (client, server) = mock("SendPayment", submitted.clone()).await;
    let r = client.send_payment(payment()).await.unwrap();
    assert_eq!(sent_variables(&server).await["signature"], Value::Null);
    assert_eq!(
        (r.nonce, r.source.as_str(), r.amount),
        (7, "B62qa", Some(Currency::from_nanomina(5)))
    );

    let (client, server) = mock("SendPayment", submitted).await;
    let signature = SignatureInput {
        field: "123".into(),
        scalar: "456".into(),
    };
    client
        .send_payment(payment().signature(signature))
        .await
        .unwrap();
    assert_eq!(
        sent_variables(&server).await["signature"],
        json!({ "field": "123", "scalar": "456" })
    );
}

#[tokio::test]
async fn test_send_zkapp_and_unlock_account() {
    let (client, server) = mock(
        "SendZkapp",
        json!({ "sendZkapp": { "zkapp": {
            "id": "z", "hash": "5Jz",
            "zkappCommand": { "memo": "", "feePayer": { "body": {
                "publicKey": "B62qpayer", "fee": "1", "nonce": "0", "validUntil": "100" } } },
            "failureReason": [ { "index": "1", "failures": ["Cancelled"] } ]
        } } }),
    )
    .await;
    let command = json!({ "feePayer": {}, "accountUpdates": [], "memo": "" });
    let z = client.send_zkapp(command.clone()).await.unwrap();
    assert_eq!(
        sent_variables(&server).await,
        json!({ "input": { "zkappCommand": command } })
    );
    assert_eq!(z.fee_payer.valid_until, Some(100));
    assert_eq!(
        z.failure_reason.unwrap()[0].failures,
        vec!["Cancelled".to_string()]
    );

    let (client, server) = mock(
        "UnlockAccount",
        json!({ "unlockAccount": { "publicKey": "B62qacc" } }),
    )
    .await;
    assert_eq!(
        client.unlock_account("B62qacc", "pw").await.unwrap(),
        "B62qacc"
    );
    assert_eq!(
        sent_variables(&server).await,
        json!({ "input": { "publicKey": "B62qacc", "password": "pw" } })
    );
}

#[tokio::test]
async fn test_transaction_status_and_small_queries() {
    let (client, server) = mock(
        "TransactionStatus",
        json!({ "transactionStatus": "INCLUDED" }),
    )
    .await;
    let status = client
        .get_transaction_status(TransactionRef::Zkapp("5Jz".into()))
        .await
        .unwrap();
    assert_eq!(status, TransactionStatus::Included);
    assert_eq!(
        sent_variables(&server).await,
        json!({ "payment": null, "zkappTransaction": "5Jz" })
    );

    let (client, _s) = mock(
        "GenesisConstants",
        json!({ "genesisConstants": {
            "genesisTimestamp": "2024-04-09T00:00:00Z", "coinbase": "720000000000",
            "accountCreationFee": "1000000000" } }),
    )
    .await;
    let g = client.get_genesis_constants().await.unwrap();
    assert_eq!(
        g.account_creation_fee,
        Currency::from_nanomina(1_000_000_000)
    );

    let (client, _s) = mock(
        "TrackedAccounts",
        json!({ "trackedAccounts": [ { "publicKey": "B62qt", "balance": { "total": "42" } } ] }),
    )
    .await;
    assert_eq!(
        client.get_tracked_accounts().await.unwrap(),
        vec![TrackedAccount {
            public_key: "B62qt".into(),
            balance: Currency::from_nanomina(42)
        }]
    );

    let (client, _s) = mock(
        "SnarkPool",
        json!({ "snarkPool": [ { "fee": "10", "prover": "B62qp", "workIds": [3, 4] } ] }),
    )
    .await;
    let w = &client.get_snark_pool().await.unwrap()[0];
    assert_eq!(
        (w.prover.as_str(), w.work_ids.clone()),
        ("B62qp", vec![3, 4])
    );

    let (client, _s) = mock(
        "ForkConfig",
        json!({ "fork_config": { "proof": { "fork": null } } }),
    )
    .await;
    assert_eq!(
        client.get_fork_config().await.unwrap()["proof"]["fork"],
        Value::Null
    );
}
