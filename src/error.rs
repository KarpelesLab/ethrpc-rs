//! Error types for the crate.

use crate::jsonrpc::ErrorObject;

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by RPC operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No servers were provided or none were reachable. Returned by
    /// [`evaluate`](crate::evaluate) and [`RpcList`](crate::RpcList).
    #[error("no available server")]
    NoAvailableServer,

    /// The method was not found locally and no host is configured (the RPC was
    /// created with an empty host and only handles overrides).
    #[error("method not found")]
    NotFound,

    /// A JSON-RPC error object returned by the server. This is a *valid*
    /// response — it is not retried against other servers by
    /// [`RpcList`](crate::RpcList).
    #[error("{0}")]
    Rpc(ErrorObject),

    /// A non-2xx HTTP status was returned with a body that was not a JSON-RPC
    /// error. `body` is a trimmed snippet (at most 200 bytes).
    #[error("HTTP {status} during {method}: {body}")]
    Http {
        /// The HTTP status code.
        status: u16,
        /// The RPC method that was being called.
        method: String,
        /// A trimmed snippet of the response body.
        body: String,
    },

    /// A transport-level failure from the underlying HTTP client.
    #[error("transport error: {0}")]
    Transport(#[from] rsurl::Error),

    /// A JSON encoding or decoding failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Any other error (e.g. an override function failure, or an unsupported
    /// argument shape).
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Reports whether this error is a JSON-RPC error object (a valid response
    /// from the server). Mirrors Go's `errors.As(err, &*ErrorObject)`.
    pub fn is_rpc_error(&self) -> bool {
        matches!(self, Error::Rpc(_))
    }

    /// Returns the underlying [`ErrorObject`] if this is a JSON-RPC error.
    pub fn as_rpc_error(&self) -> Option<&ErrorObject> {
        match self {
            Error::Rpc(eo) => Some(eo),
            _ => None,
        }
    }
}
