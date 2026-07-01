//! JSON-RPC 2.0 request, response, and error types.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Monotonic request id counter, shared across all requests.
static RPC_ID: AtomicU64 = AtomicU64::new(0);

/// Returns the next request id. The first id is 1, matching the Go
/// implementation's `atomic.AddUint64(&rpcId, 1)`.
fn next_id() -> u64 {
    RPC_ID.fetch_add(1, Ordering::SeqCst) + 1
}

/// A JSON-RPC 2.0 request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The method name.
    pub method: String,
    /// Either a positional array or a named-parameter object.
    pub params: Value,
    /// The request id.
    pub id: Value,
}

impl Request {
    /// Builds a new request with positional parameters, fit to use with
    /// [`Rpc::send`](crate::Rpc::send). An empty `params` is encoded as `[]`,
    /// never `null`.
    pub fn new(method: impl Into<String>, params: Vec<Value>) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params: Value::Array(params),
            id: Value::from(next_id()),
        }
    }

    /// Builds a new request with named parameters.
    pub fn with_map(method: impl Into<String>, params: serde_json::Map<String, Value>) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params: Value::Object(params),
            id: Value::from(next_id()),
        }
    }

    /// Wraps an error into a JSON-RPC response carrying this request's id. If
    /// `e` is itself a JSON-RPC error it is preserved verbatim, otherwise it is
    /// wrapped with the internal-error code `-32603`.
    pub(crate) fn make_error(&self, e: &crate::Error) -> ResponseIntf {
        let error = match e {
            crate::Error::Rpc(eo) => eo.clone(),
            other => ErrorObject {
                code: -32603,
                message: other.to_string(),
                data: None,
            },
        };
        ResponseIntf {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id: self.id.clone(),
        }
    }
}

/// A JSON-RPC 2.0 response with a raw JSON result.
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// The result value, if any.
    #[serde(default)]
    pub result: Value,
    /// The error object, if the call failed.
    #[serde(default)]
    pub error: Option<ErrorObject>,
    /// The request id echoed back.
    #[serde(default)]
    pub id: Value,
}

/// A JSON-RPC 2.0 response where `result` is an arbitrary serializable value,
/// used when encoding a locally-produced (overridden) response.
#[derive(Debug, Clone, Serialize)]
pub struct ResponseIntf {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The result value, omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error object, omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
    /// The request id.
    pub id: Value,
}

/// A JSON-RPC 2.0 error object returned by a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorObject {
    /// The numeric error code.
    pub code: i64,
    /// A human-readable error message.
    pub message: String,
    /// Optional structured error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl fmt::Display for ErrorObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "jsonrpc error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for ErrorObject {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_request_empty_params() {
        let req = Request::new("eth_blockNumber", vec![]);
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "eth_blockNumber");
        assert_eq!(req.params, json!([]));
        // Id should be a non-zero number.
        assert!(req.id.as_u64().unwrap() > 0);
    }

    #[test]
    fn new_request_with_params() {
        let req = Request::new("eth_getBalance", vec![json!("0xdead"), json!("latest")]);
        assert_eq!(req.params, json!(["0xdead", "latest"]));
    }

    #[test]
    fn new_request_map() {
        let mut m = serde_json::Map::new();
        m.insert("to".to_string(), json!("0xdead"));
        let req = Request::with_map("eth_call", m);
        assert_eq!(req.method, "eth_call");
        assert_eq!(req.params["to"], json!("0xdead"));
    }

    #[test]
    fn ids_are_monotonic() {
        let a = Request::new("a", vec![]).id.as_u64().unwrap();
        let b = Request::new("b", vec![]).id.as_u64().unwrap();
        assert!(b > a);
    }

    #[test]
    fn error_object_display() {
        let e = ErrorObject {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        };
        assert_eq!(e.to_string(), "jsonrpc error -32601: Method not found");
    }

    #[test]
    fn make_error_generic_and_passthrough() {
        let req = Request::new("eth_test", vec![]);
        let resp = req.make_error(&crate::Error::Other("something broke".to_string()));
        let eo = resp.error.unwrap();
        assert_eq!(eo.code, -32603);
        assert_eq!(eo.message, "something broke");

        let original = ErrorObject {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        };
        let resp = req.make_error(&crate::Error::Rpc(original));
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
