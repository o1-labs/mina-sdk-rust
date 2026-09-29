//! Offline check of the ITN documents in `mina_sdk::itn::queries` against
//! `schema/itn_graphql_schema.json` (feature `itn`): every selected field
//! must exist on its type, and every argument must exist on its field.
//! The documents are simple (no fragments or aliases), so a small parser
//! is enough.
#![cfg(feature = "itn")]

use serde_json::Value;

use mina_sdk::itn::queries;

#[derive(Debug, PartialEq)]
enum Tok {
    Name(String),
    Punct(char),
}

fn tokenize(doc: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut chars = doc.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || c == '$' {
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Tok::Name(s));
        } else {
            if "(){}:!,[]".contains(c) {
                out.push(Tok::Punct(c));
            }
            chars.next();
        }
    }
    out
}

struct Schema(Value);

impl Schema {
    fn load() -> Self {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/schema/itn_graphql_schema.json"
        );
        Schema(serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
    }

    fn ty(&self, name: &str) -> &Value {
        self.0["data"]["__schema"]["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("type {name} not in schema"))
    }

    fn root(&self, op: &str) -> String {
        let key = if op == "mutation" {
            "mutationType"
        } else {
            "queryType"
        };
        self.0["data"]["__schema"][key]["name"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

/// Name of the named type under NON_NULL / LIST wrappers.
fn base_type(t: &Value) -> String {
    match t["name"].as_str() {
        Some(n) => n.to_string(),
        None => base_type(&t["ofType"]),
    }
}

/// Check the selection set that starts at `toks[i]` (a `{`) against type
/// `ty`; return the index after its closing `}`.
fn check_selection(schema: &Schema, ty: &str, toks: &[Tok], mut i: usize, doc: &str) -> usize {
    assert_eq!(toks[i], Tok::Punct('{'));
    i += 1;
    let fields = schema.ty(ty)["fields"].as_array().unwrap();
    while toks[i] != Tok::Punct('}') {
        let Tok::Name(name) = &toks[i] else {
            panic!("unexpected token {:?} in {doc}", toks[i])
        };
        let field = fields
            .iter()
            .find(|f| f["name"] == name.as_str())
            .unwrap_or_else(|| panic!("{ty}.{name} is not in the ITN schema:\n{doc}"));
        i += 1;
        if toks[i] == Tok::Punct('(') {
            i += 1;
            while toks[i] != Tok::Punct(')') {
                if let (Tok::Name(arg), Tok::Punct(':')) = (&toks[i], &toks[i + 1]) {
                    assert!(
                        field["args"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|a| a["name"] == arg.as_str()),
                        "{ty}.{name} has no argument {arg}:\n{doc}"
                    );
                }
                i += 1;
            }
            i += 1;
        }
        if toks[i] == Tok::Punct('{') {
            i = check_selection(schema, &base_type(&field["type"]), toks, i, doc);
        }
    }
    i + 1
}

fn check(schema: &Schema, doc: &str) {
    let toks = tokenize(doc);
    let op = match &toks[0] {
        Tok::Name(n) if n == "mutation" => "mutation",
        _ => "query",
    };
    // Skip the operation's variable definitions: the selection set is the
    // first `{` at parenthesis depth 0.
    let mut depth = 0;
    let start = toks
        .iter()
        .position(|t| {
            match t {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => depth -= 1,
                _ => {}
            }
            depth == 0 && *t == Tok::Punct('{')
        })
        .unwrap();
    check_selection(schema, &schema.root(op), &toks, start, doc);
}

#[test]
fn test_itn_queries_match_vendored_schema() {
    let schema = Schema::load();
    for doc in [
        queries::AUTH,
        queries::SLOTS_WON,
        queries::INTERNAL_LOGS,
        queries::FLUSH_INTERNAL_LOGS,
        queries::SCHEDULE_PAYMENTS,
        queries::SCHEDULE_ZKAPP_COMMANDS,
        queries::STOP_SCHEDULED_TRANSACTIONS,
        queries::UPDATE_GATING,
        queries::STOP_DAEMON,
        queries::ZKAPP_COMMAND_LIMIT,
    ] {
        check(&schema, doc);
    }
}

#[test]
#[should_panic(expected = "is not in the ITN schema")]
fn test_checker_rejects_unknown_field() {
    check(&Schema::load(), "query { auth { serverUuid noSuchField } }");
}

#[test]
#[should_panic(expected = "has no argument")]
fn test_checker_rejects_unknown_argument() {
    check(
        &Schema::load(),
        "mutation ($x: Int!) { flushInternalLogs(noSuchArg: $x) }",
    );
}

/// The old hand-written Rust client used `stopPayments`, which the daemon
/// does not have; the check must catch that kind of drift.
#[test]
#[should_panic(expected = "is not in the ITN schema")]
fn test_checker_rejects_stop_payments() {
    check(
        &Schema::load(),
        "mutation ($h: String!) { stopPayments(handle: $h) }",
    );
}
