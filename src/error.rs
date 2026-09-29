use std::fmt;

/// Errors returned by the Mina SDK.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The GraphQL endpoint returned one or more errors.
    #[error("GraphQL error in {query_name}: {messages}")]
    Graphql {
        query_name: String,
        messages: String,
        errors: Vec<GraphqlErrorEntry>,
    },

    /// Failed to connect after all retry attempts.
    #[error("failed to execute {query_name} after {attempts} attempts: {source}")]
    Connection {
        query_name: String,
        attempts: u32,
        #[source]
        source: reqwest::Error,
    },

    /// A required field was missing in the response.
    #[error("missing field '{field}' in {query_name} response")]
    MissingField { query_name: String, field: String },

    /// Currency value would underflow (go negative).
    #[error("currency underflow: {0} - {1} would be negative")]
    CurrencyUnderflow(u64, u64),

    /// Invalid currency format string.
    #[error("invalid currency format: {0}")]
    InvalidCurrency(String),

    /// Account not found.
    #[error("account not found: {0}")]
    AccountNotFound(String),

    /// The ITN server rejected the request signature (HTTP 401): the
    /// signature is wrong, or the key's public half is not in the daemon's
    /// `--itn-keys`.
    #[error("ITN server rejected the signature of {query_name} (HTTP 401); is the public key in --itn-keys?")]
    ItnUnauthorized { query_name: String },

    /// The ITN server still rejected the sequence information (HTTP 412)
    /// after a new `auth` handshake.
    #[error(
        "ITN server rejected the sequence number of {query_name} again after a new auth (HTTP 412)"
    )]
    ItnSequencing { query_name: String },

    /// An ITN key could not be decoded.
    #[error("invalid ITN key: {0}")]
    InvalidItnKey(String),
}

/// A single error entry from a GraphQL response.
#[derive(Debug, Clone)]
pub struct GraphqlErrorEntry {
    pub message: String,
}

impl fmt::Display for GraphqlErrorEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
