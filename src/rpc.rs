//! The [`RPC`] client and the [`Handler`] trait.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use rsurl::aio;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::jsonrpc::{Request, Response};

/// A locally-handled RPC method. Receives the positional parameters and returns
/// a JSON value (or an error). Registered with [`RPC::set_override`].
pub type OverrideFn = Arc<dyn Fn(&[Value]) -> Result<Value> + Send + Sync>;

/// Any backend capable of executing JSON-RPC calls. Implemented by [`RPC`] and
/// [`RpcList`](crate::RpcList). Mirrors Go's `Handler` interface.
#[async_trait]
pub trait Handler: Send + Sync {
    /// Performs a JSON-RPC call with positional parameters.
    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value>;
}

/// A connection to an Ethereum JSON-RPC endpoint over HTTP, with optional basic
/// authentication and local method overrides.
#[derive(Clone, Default)]
pub struct RPC {
    host: String,
    lag: Duration,
    block: u64,
    username: String,
    password: String,
    overrides: HashMap<String, OverrideFn>,
}

impl RPC {
    /// Returns a new RPC client targeting the given endpoint. Pass an empty host
    /// to build an override-only handler.
    pub fn new(host: impl Into<String>) -> RPC {
        RPC {
            host: host.into(),
            ..Default::default()
        }
    }

    /// Redirects calls to `method` to a local function instead of the remote
    /// node. The function receives the call's positional parameters.
    pub fn set_override<F>(&mut self, method: impl Into<String>, f: F)
    where
        F: Fn(&[Value]) -> Result<Value> + Send + Sync + 'static,
    {
        self.overrides.insert(method.into(), Arc::new(f));
    }

    /// Sets the host for subsequent requests.
    pub fn set_host(&mut self, host: impl Into<String>) {
        self.host = host.into();
    }

    /// Returns the configured host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Sets HTTP basic auth credentials for subsequent requests.
    pub fn set_basic_auth(&mut self, username: impl Into<String>, password: impl Into<String>) {
        self.username = username.into();
        self.password = password.into();
    }

    /// Returns how long this endpoint took to answer `eth_blockNumber` during
    /// [`Evaluate`](crate::Evaluate), or zero if it was never probed.
    pub fn lag(&self) -> Duration {
        self.lag
    }

    /// Returns the latest block number observed during
    /// [`Evaluate`](crate::Evaluate), or zero if it was never probed.
    pub fn block(&self) -> u64 {
        self.block
    }

    pub(crate) fn set_probe(&mut self, lag: Duration, block: u64) {
        self.lag = lag;
        self.block = block;
    }

    /// Performs a request using positional arguments.
    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        self.send(&Request::new(method, params)).await
    }

    /// Performs a request using named arguments.
    pub async fn call_named(&self, method: &str, params: Map<String, Value>) -> Result<Value> {
        self.send(&Request::with_map(method, params)).await
    }

    /// Performs a request and deserializes the result into `T`.
    pub async fn call_as<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Vec<Value>,
    ) -> Result<T> {
        crate::decode::read_as(self.call(method, params).await)
    }

    /// Sends a raw [`Request`] to the endpoint and returns the raw result value.
    pub async fn send(&self, req: &Request) -> Result<Value> {
        // Local override: run the function instead of hitting the node.
        if let Some(f) = self.overrides.get(&req.method) {
            return match &req.params {
                Value::Array(arr) => f(arr),
                _ => Err(Error::Other(
                    "function requires positional arguments instead of named arguments".to_string(),
                )),
            };
        }

        if self.host.is_empty() {
            // Override-only handler: anything else is "not found".
            return Err(Error::NotFound);
        }

        let body = serde_json::to_vec(req)?;
        let mut hreq = aio::Request::post(self.host.clone(), body)
            .header("Content-Type", "application/json");

        if !self.username.is_empty() || !self.password.is_empty() {
            let token = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", self.username, self.password));
            hreq = hreq.header("Authorization", format!("Basic {token}"));
        }

        let rt = aio::TokioRuntime;
        let resp = aio::request(&rt, &hreq).await?;
        let status = resp.status;
        let body = resp.body;

        // Some servers return JSON-RPC errors over HTTP 4xx/5xx. Try to decode
        // either way; if decoding fails on a non-2xx, surface the HTTP status.
        let decoded: std::result::Result<Response, _> = serde_json::from_slice(&body);

        if !(200..300).contains(&status) {
            if let Ok(r) = &decoded {
                if let Some(eo) = &r.error {
                    return Err(Error::Rpc(eo.clone()));
                }
            }
            return Err(Error::Http {
                status,
                method: req.method.clone(),
                body: snippet(&body),
            });
        }

        let res = decoded?;
        if let Some(eo) = res.error {
            return Err(Error::Rpc(eo));
        }
        Ok(res.result)
    }

    /// Performs a request and deserializes the result into `target` via serde.
    pub async fn to<T: DeserializeOwned>(&self, method: &str, params: Vec<Value>) -> Result<T> {
        self.call_as(method, params).await
    }
}

#[async_trait]
impl Handler for RPC {
    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        RPC::call(self, method, params).await
    }
}

/// Returns a trimmed, lossy snippet of up to 200 bytes of `body`.
fn snippet(body: &[u8]) -> String {
    let end = body.len().min(200);
    String::from_utf8_lossy(&body[..end]).trim().to_string()
}

/// Options controlling how [`RPC::forward`] builds its response.
#[derive(Debug, Clone, Default)]
pub struct ForwardOptions {
    /// Pretty-print the JSON body.
    pub pretty: bool,
    /// If set, emit `Cache-Control: public, max-age=<seconds>`.
    pub cache: Option<Duration>,
}

/// A response produced by [`RPC::forward`], ready to be written to whatever HTTP
/// framework the caller uses.
#[derive(Debug, Clone)]
pub struct ForwardResponse {
    /// The HTTP status code to send.
    pub status: u16,
    /// The response headers, in order.
    pub headers: Vec<(String, String)>,
    /// The response body.
    pub body: Vec<u8>,
}

/// Hop-by-hop headers per RFC 7230 §6.1 — must not be forwarded by
/// intermediaries. Compared case-insensitively.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP.iter().any(|h| name.eq_ignore_ascii_case(h))
}

impl RPC {
    /// Builds the response for a JSON-RPC request suitable for proxying back to
    /// an HTTP client. Overridden methods are executed locally; otherwise the
    /// request is forwarded to the node and its response relayed (with
    /// hop-by-hop headers stripped). The returned [`ForwardResponse`] is
    /// framework-agnostic — write its status, headers, and body to your
    /// response object.
    pub async fn forward(&self, req: &Request, opts: &ForwardOptions) -> ForwardResponse {
        // Local override: run it and encode a JSON-RPC response body.
        if let Some(f) = self.overrides.get(&req.method) {
            let mut headers = vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                (
                    "Access-Control-Allow-Methods".to_string(),
                    "GET, POST, OPTIONS".to_string(),
                ),
            ];
            cache_header(&mut headers, opts);

            let body = match &req.params {
                Value::Array(arr) => match f(arr) {
                    Ok(res) => encode_json(
                        &crate::jsonrpc::ResponseIntf {
                            jsonrpc: "2.0".to_string(),
                            result: Some(res),
                            error: None,
                            id: req.id.clone(),
                        },
                        opts.pretty,
                    ),
                    // JSON-RPC convention: transport stays 200, error in body.
                    Err(e) => encode_json(&req.make_error(&e), opts.pretty),
                },
                _ => encode_json(
                    &req.make_error(&Error::Other(
                        "function only supports positional arguments".to_string(),
                    )),
                    opts.pretty,
                ),
            };
            return ForwardResponse {
                status: 200,
                headers,
                body,
            };
        }

        if self.host.is_empty() {
            return ForwardResponse {
                status: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"404 page not found\n".to_vec(),
            };
        }

        // Forward to the node.
        let enc = match serde_json::to_vec(req) {
            Ok(b) => b,
            Err(e) => return internal_error(&e.to_string()),
        };
        let mut hreq = aio::Request::post(self.host.clone(), enc)
            .header("Content-Type", "application/json");
        if !self.username.is_empty() || !self.password.is_empty() {
            let token = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", self.username, self.password));
            hreq = hreq.header("Authorization", format!("Basic {token}"));
        }

        let rt = aio::TokioRuntime;
        let resp = match aio::request(&rt, &hreq).await {
            Ok(r) => r,
            Err(e) => return internal_error(&e.to_string()),
        };

        let mut headers: Vec<(String, String)> = resp
            .headers
            .iter()
            .filter(|(k, _)| !is_hop_by_hop(k))
            .filter(|(k, _)| !(opts.pretty && k.eq_ignore_ascii_case("content-length")))
            .cloned()
            .collect();
        headers.push((
            "Access-Control-Allow-Methods".to_string(),
            "GET, POST, OPTIONS".to_string(),
        ));
        cache_header(&mut headers, opts);

        let body = if opts.pretty {
            match serde_json::from_slice::<Value>(&resp.body) {
                // Go re-indents the proxied body with two spaces.
                Ok(v) => encode_json_indent(&v, b"  "),
                Err(_) => resp.body.clone(),
            }
        } else {
            resp.body.clone()
        };

        ForwardResponse {
            status: resp.status,
            headers,
            body,
        }
    }
}

fn cache_header(headers: &mut Vec<(String, String)>, opts: &ForwardOptions) {
    if let Some(d) = opts.cache {
        if d > Duration::ZERO {
            headers.push((
                "Cache-Control".to_string(),
                format!("public, max-age={}", d.as_secs()),
            ));
        }
    }
}

/// Encodes a serializable value to JSON bytes; when `pretty`, uses a 4-space
/// indent to match the Go override encoder (`SetIndent("", "    ")`).
fn encode_json<T: serde::Serialize>(v: &T, pretty: bool) -> Vec<u8> {
    if pretty {
        return encode_json_indent(v, b"    ");
    }
    serde_json::to_vec(v).unwrap_or_default()
}

/// Encodes `v` as pretty JSON using the given indent unit.
fn encode_json_indent<T: serde::Serialize>(v: &T, indent: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent);
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    if v.serialize(&mut ser).is_ok() {
        buf
    } else {
        serde_json::to_vec(v).unwrap_or_default()
    }
}

fn internal_error(msg: &str) -> ForwardResponse {
    ForwardResponse {
        status: 500,
        headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
        body: format!("{msg}\n").into_bytes(),
    }
}
