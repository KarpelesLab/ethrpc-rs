//! Server list ([`RpcList`]) and fastest-server selection ([`evaluate`](crate::evaluate)).

use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;

use crate::decode::ValueExt;
use crate::error::{Error, Result};
use crate::rpc::{Handler, Rpc};

/// A list of [`Rpc`] endpoints that implements [`Handler`] with failover.
#[derive(Default)]
pub struct RpcList(pub Vec<Rpc>);

#[async_trait]
impl Handler for RpcList {
    /// Performs a call against the servers in order, failing over to the next on
    /// transport errors. A JSON-RPC error response is returned immediately
    /// without failover, since it represents a valid answer from the server.
    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        if self.0.is_empty() {
            return Err(Error::NoAvailableServer);
        }
        let mut last_err: Option<Error> = None;
        for srv in &self.0 {
            match srv.call(method, params.clone()).await {
                Ok(res) => return Ok(res),
                Err(err) => {
                    // A JSON-RPC error is a valid response — don't retry.
                    if err.is_rpc_error() {
                        return Err(err);
                    }
                    last_err = Some(err);
                }
            }
        }
        Err(last_err.unwrap_or(Error::NoAvailableServer))
    }
}

/// Probes a single server with `eth_blockNumber`, recording its lag and block.
async fn probe(host: String) -> Result<Rpc> {
    let mut r = Rpc::new(host);
    let start = Instant::now();
    let block = r.call("eth_blockNumber", vec![]).await?.to_u64()?;
    r.set_probe(start.elapsed(), block);
    Ok(r)
}

/// How long [`evaluate`](crate::evaluate) keeps waiting for additional servers after the first
/// success, so it doesn't block on the slowest endpoint.
const SELECTION_GRACE: Duration = Duration::from_millis(200);

/// Calls every server with `eth_blockNumber`, measures response time, and
/// returns a [`Handler`] backed by the servers that responded.
///
/// With a single server, the returned handler is that one [`Rpc`] (and an error
/// is returned if it fails to respond). With multiple servers, the result is an
/// [`RpcList`] of every server that answered within a short grace period
/// (200&nbsp;ms) of the first success (or all that eventually answer, whichever
/// comes first).
pub async fn evaluate(servers: &[&str]) -> Result<Box<dyn Handler>> {
    match servers.len() {
        0 => Err(Error::NoAvailableServer),
        1 => {
            // Probe it so we honor the contract of returning working servers.
            let r = probe(servers[0].to_string()).await?;
            Ok(Box::new(r))
        }
        _ => {
            let mut futs: FuturesUnordered<_> =
                servers.iter().map(|s| probe(s.to_string())).collect();

            let mut res: Vec<Rpc> = Vec::new();
            let mut last_err: Option<Error> = None;
            // Armed after the first success; once it fires we stop waiting.
            let mut grace = std::pin::pin!(futures::future::OptionFuture::from(None));

            loop {
                tokio::select! {
                    biased;
                    _ = grace.as_mut(), if !res.is_empty() => {
                        return Ok(Box::new(RpcList(res)));
                    }
                    next = futs.next() => match next {
                        None => {
                            // Every server has reported in.
                            if !res.is_empty() {
                                return Ok(Box::new(RpcList(res)));
                            }
                            return Err(last_err.unwrap_or(Error::NoAvailableServer));
                        }
                        Some(Ok(r)) => {
                            let first = res.is_empty();
                            res.push(r);
                            if first {
                                grace.set(Some(tokio::time::sleep(SELECTION_GRACE)).into());
                            }
                        }
                        Some(Err(e)) => {
                            last_err = Some(e);
                        }
                    }
                }
            }
        }
    }
}
