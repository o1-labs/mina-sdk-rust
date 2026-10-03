//! Conformance to the specification in `spec/` (a copy of
//! o1-labs/mina-sdk-spec at the tag in `spec/VERSION`):
//!
//! 1. the query strings in `mina_sdk::queries` are exactly the documents of
//!    `spec/operations.graphql`, up to white space, and every document has
//!    one;
//! 2. likewise the ITN query strings in `mina_sdk::itn::queries` and
//!    `spec/itn-operations.graphql` (feature `itn`).
//!
//! mina-sdk-spec's CI validates the documents against the daemon's schema.

use std::collections::BTreeMap;

use mina_sdk::queries;

fn read(path: &str) -> String {
    std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn tokenize(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = doc.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '#' {
            while chars.next().is_some_and(|c| c != '\n') {}
        } else if c.is_alphanumeric() || c == '_' || c == '$' {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || c == '$' {
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(s);
        } else {
            if "(){}:!,[]".contains(c) {
                out.push(c.to_string());
            }
            chars.next();
        }
    }
    out
}

/// The operations of a document, by name: (kind, tokens of the whole
/// operation).
fn operations(doc: &str) -> BTreeMap<String, (String, Vec<String>)> {
    let toks = tokenize(doc);
    let mut ops = BTreeMap::new();
    let mut i = 0;
    while i < toks.len() {
        let (kind, name) = (toks[i].clone(), toks[i + 1].clone());
        let start = i;
        let mut depth = 0;
        let mut seen_body = false;
        loop {
            match toks[i].as_str() {
                "{" => {
                    depth += 1;
                    seen_body = true;
                }
                "}" => depth -= 1,
                _ => {}
            }
            i += 1;
            if seen_body && depth == 0 {
                break;
            }
        }
        assert!(
            ops.insert(name.clone(), (kind, toks[start..i].to_vec()))
                .is_none(),
            "operation {name} twice"
        );
    }
    ops
}

/// Every query string of `mina_sdk::queries` that belongs to the common API.
const SDK_DOCUMENTS: &[&str] = &[
    queries::SYNC_STATUS,
    queries::DAEMON_STATUS,
    queries::DAEMON_METRICS,
    queries::NETWORK_ID,
    queries::GET_ACCOUNT,
    queries::BEST_CHAIN,
    queries::GENESIS_BLOCK,
    queries::BLOCK,
    queries::GET_PEERS,
    queries::POOLED_USER_COMMANDS,
    queries::POOLED_ZKAPP_COMMANDS,
    queries::TRANSACTION_STATUS,
    queries::GENESIS_CONSTANTS,
    queries::TRACKED_ACCOUNTS,
    queries::SNARK_POOL,
    queries::FORK_CONFIG,
    queries::SEND_PAYMENT,
    queries::SEND_DELEGATION,
    queries::SEND_ZKAPP,
    queries::UNLOCK_ACCOUNT,
    queries::SET_SNARK_WORKER,
    queries::SET_SNARK_WORK_FEE,
];

/// Check that `documents` are exactly the operations of `spec_file`.
fn assert_documents_are_the_spec(spec_file: &str, documents: &[&str]) {
    let spec = operations(&read(spec_file));
    let mut covered = Vec::new();
    for doc in documents {
        let ops = operations(doc);
        assert_eq!(ops.len(), 1, "one named operation per query string:\n{doc}");
        let (name, (_, toks)) = ops.into_iter().next().unwrap();
        let (_, expected) = spec
            .get(&name)
            .unwrap_or_else(|| panic!("{name} is not in the specification"));
        assert_eq!(&toks, expected, "{name} differs from {spec_file}");
        covered.push(name);
    }
    covered.sort();
    let all: Vec<_> = spec.keys().cloned().collect();
    assert_eq!(
        covered, all,
        "every operation of {spec_file} has one query string"
    );
}

#[test]
fn test_sdk_queries_are_the_spec_documents() {
    assert_documents_are_the_spec("spec/operations.graphql", SDK_DOCUMENTS);
}

#[cfg(feature = "itn")]
#[test]
fn test_itn_queries_are_the_spec_documents() {
    use mina_sdk::itn::queries as itn;
    assert_documents_are_the_spec(
        "spec/itn-operations.graphql",
        &[
            itn::AUTH,
            itn::SLOTS_WON,
            itn::INTERNAL_LOGS,
            itn::FLUSH_INTERNAL_LOGS,
            itn::SCHEDULE_PAYMENTS,
            itn::SCHEDULE_ZKAPP_COMMANDS,
            itn::STOP_SCHEDULED_TRANSACTIONS,
            itn::UPDATE_GATING,
            itn::STOP_DAEMON,
            itn::ZKAPP_COMMAND_LIMIT,
            itn::COMMIT_ID,
            itn::SCHEDULED_TRANSACTIONS,
            itn::SCHEDULE_PAYMENTS_WITH_HANDLE,
            itn::SCHEDULE_ZKAPP_COMMANDS_WITH_HANDLE,
            itn::CREATE_ACCOUNTS,
        ],
    );
}
