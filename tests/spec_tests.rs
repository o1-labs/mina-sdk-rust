//! The common API specification (`spec/operations.graphql`, see
//! `spec/SPEC.md`) against this SDK:
//!
//! 1. every document of the specification is valid against
//!    `schema/graphql_schema.json` (fields, arguments, nested selections);
//! 2. the query strings in `mina_sdk::queries` are exactly the specification's
//!    documents, up to white space, and every document has one.

use std::collections::BTreeMap;

use serde_json::Value;

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

struct Schema {
    query: String,
    mutation: String,
    types: BTreeMap<String, Value>,
}

fn base(t: &Value) -> String {
    match t["name"].as_str() {
        Some(n) => n.to_string(),
        None => base(&t["ofType"]),
    }
}

impl Schema {
    fn load() -> Self {
        let v: Value = serde_json::from_str(&read("schema/graphql_schema.json")).unwrap();
        let s = &v["data"]["__schema"];
        Schema {
            query: s["queryType"]["name"].as_str().unwrap().into(),
            mutation: s["mutationType"]["name"].as_str().unwrap().into(),
            types: s["types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (t["name"].as_str().unwrap().to_string(), t.clone()))
                .collect(),
        }
    }

    /// Check the selection set at `toks[i]` against type `ty`.
    fn selection(
        &self,
        ty: &str,
        toks: &[String],
        mut i: usize,
        problems: &mut Vec<String>,
    ) -> usize {
        i += 1;
        let fields = self.types[ty]["fields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        while toks[i] != "}" {
            let name = &toks[i];
            let Some(field) = fields.iter().find(|f| f["name"] == name.as_str()) else {
                problems.push(format!("{ty}.{name} is not in the schema"));
                return toks.len() - 1;
            };
            i += 1;
            if toks[i] == "(" {
                let args: Vec<&str> = field["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a["name"].as_str().unwrap())
                    .collect();
                let mut depth = 1;
                i += 1;
                while depth > 0 {
                    match toks[i].as_str() {
                        "(" | "{" => depth += 1,
                        ")" | "}" => depth -= 1,
                        arg if depth == 1
                            && toks[i + 1] == ":"
                            && !arg.starts_with('$')
                            && !args.contains(&arg) =>
                        {
                            problems.push(format!("{ty}.{name} has no argument {arg}"))
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            if toks[i] == "{" {
                i = self.selection(&base(&field["type"]), toks, i, problems);
            } else if self.types[&base(&field["type"])]["kind"] == "OBJECT" {
                problems.push(format!("{ty}.{name} is an object and needs a selection"));
            }
        }
        i + 1
    }

    fn check(&self, kind: &str, toks: &[String]) -> Vec<String> {
        let root = if kind == "mutation" {
            &self.mutation
        } else {
            &self.query
        };
        let mut depth = 0;
        let start = toks
            .iter()
            .position(|t| {
                match t.as_str() {
                    "(" => depth += 1,
                    ")" => depth -= 1,
                    _ => {}
                }
                depth == 0 && t == "{"
            })
            .unwrap();
        let mut problems = Vec::new();
        self.selection(root, toks, start, &mut problems);
        problems
    }
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

#[test]
fn test_spec_documents_are_valid_against_the_schema() {
    let schema = Schema::load();
    let spec = operations(&read("spec/operations.graphql"));
    assert_eq!(spec.len(), 22, "the specification has 22 operations");
    for (name, (kind, toks)) in &spec {
        let problems = schema.check(kind, toks);
        assert!(problems.is_empty(), "{name}: {problems:?}");
    }
}

#[test]
fn test_sdk_queries_are_the_spec_documents() {
    let spec = operations(&read("spec/operations.graphql"));
    let mut covered = Vec::new();
    for doc in SDK_DOCUMENTS {
        let ops = operations(doc);
        assert_eq!(ops.len(), 1, "one named operation per query string:\n{doc}");
        let (name, (_, toks)) = ops.into_iter().next().unwrap();
        let (_, expected) = spec
            .get(&name)
            .unwrap_or_else(|| panic!("{name} is not in the specification"));
        assert_eq!(
            &toks, expected,
            "{name} differs from spec/operations.graphql"
        );
        covered.push(name);
    }
    covered.sort();
    let all: Vec<_> = spec.keys().cloned().collect();
    assert_eq!(
        covered, all,
        "every specification operation has one query string"
    );
}

#[test]
fn test_checker_catches_drift() {
    let schema = Schema::load();
    let check = |doc: &str| {
        let (_, (kind, toks)) = operations(doc).into_iter().next().unwrap();
        schema.check(&kind, &toks)
    };
    assert!(check("query A { snarkPool { workIdz } }")[0].contains("not in the schema"));
    assert!(
        check("query A($p: ID) { transactionStatus(paymentX: $p) }")[0].contains("no argument")
    );
    assert!(check("query A { genesisBlock }")[0].contains("needs a selection"));
    assert!(
        check("mutation A($f: UInt64!) { setSnarkWorkFee(input: {fee: $f}) { lastFee } }")
            .is_empty()
    );
}
