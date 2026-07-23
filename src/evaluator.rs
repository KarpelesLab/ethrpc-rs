//! Server list ([`RpcList`]) and fastest-server selection ([`evaluate`](crate::evaluate)).

use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;
// `web-time` is `std::time` on native and a browser-clock shim on wasm32, where
// `std::time::Instant::now()` panics.
use web_time::Instant;

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

/// A runtime-agnostic sleep. Native builds reuse rsurl's Tokio-backed timer —
/// the same runtime that drives the requests — so we don't depend on tokio
/// directly; `wasm32` uses the browser's timer via `gloo-timers`.
#[cfg(not(target_arch = "wasm32"))]
async fn sleep(dur: Duration) {
    use rsurl::aio::Runtime;
    rsurl::aio::TokioRuntime.sleep(dur).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep(dur: Duration) {
    gloo_timers::future::TimeoutFuture::new(dur.as_millis() as u32).await;
}

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

            // Phase 1: wait for the first successful probe, remembering the last
            // transport error in case none succeed.
            loop {
                match futs.next().await {
                    None => return Err(last_err.unwrap_or(Error::NoAvailableServer)),
                    Some(Ok(r)) => {
                        res.push(r);
                        break;
                    }
                    Some(Err(e)) => last_err = Some(e),
                }
            }

            // Phase 2: keep collecting successes, but race the remaining probes
            // against a grace timer so we don't block on the slowest endpoint.
            // `futures::future::select` is runtime-agnostic (works on wasm too).
            let mut grace = std::pin::pin!(sleep(SELECTION_GRACE));
            loop {
                match futures::future::select(futs.next(), grace.as_mut()).await {
                    // Grace elapsed, or every server has reported in.
                    futures::future::Either::Right(_)
                    | futures::future::Either::Left((None, _)) => break,
                    futures::future::Either::Left((Some(Ok(r)), _)) => res.push(r),
                    futures::future::Either::Left((Some(Err(_)), _)) => {}
                }
            }

            Ok(Box::new(RpcList(res)))
        }
    }
}
