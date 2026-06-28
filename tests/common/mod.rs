//! A minimal HTTP/1.1 mock server for exercising the RPC client.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A parsed inbound HTTP request.
pub struct Incoming {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Incoming {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Parses the request body as a JSON-RPC request and returns its `id`.
    pub fn rpc_id(&self) -> serde_json::Value {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .ok()
            .and_then(|v| v.get("id").cloned())
            .unwrap_or(serde_json::Value::Null)
    }
}

/// A handler's reply: an HTTP status and a body.
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn ok(body: impl Into<Vec<u8>>) -> Reply {
        Reply {
            status: 200,
            body: body.into(),
        }
    }
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Reply {
        Reply {
            status,
            body: body.into(),
        }
    }
}

/// Builds a JSON-RPC success reply echoing the request id.
pub fn rpc_ok(req: &Incoming, result: serde_json::Value) -> Reply {
    let resp = serde_json::json!({"jsonrpc":"2.0","result":result,"id":req.rpc_id()});
    Reply::ok(serde_json::to_vec(&resp).unwrap())
}

/// Builds a JSON-RPC error reply (HTTP 200) echoing the request id.
pub fn rpc_err(req: &Incoming, code: i64, message: &str) -> Reply {
    let resp = serde_json::json!({
        "jsonrpc":"2.0",
        "error":{"code":code,"message":message},
        "id":req.rpc_id(),
    });
    Reply::ok(serde_json::to_vec(&resp).unwrap())
}

/// Starts a mock server on an ephemeral port and returns its base URL. The
/// server runs until the test's runtime is dropped. `handler` is invoked once
/// per request.
pub async fn serve<F>(handler: F) -> String
where
    F: Fn(&Incoming) -> Reply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let handler = Arc::new(handler);

    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => break,
            };
            let handler = handler.clone();
            tokio::spawn(async move {
                let Some(req) = read_request(&mut sock).await else {
                    return;
                };
                let reply = handler(&req);
                let head = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.status,
                    reason(reply.status),
                    reply.body.len(),
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&reply.body).await;
                let _ = sock.flush().await;
            });
        }
    });

    url
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Incoming> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in head.split("\r\n").skip(1) {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_string();
            let v = v.trim().to_string();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            headers.push((k, v));
        }
    }

    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }

    Some(Incoming { headers, body })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        502 => "Bad Gateway",
        _ => "Status",
    }
}
