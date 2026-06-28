//! High-level [`Api`] wrapper with convenience methods for common calls.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::decode::read_u64;
use crate::error::Result;
use crate::rpc::Handler;

/// Wraps any [`Handler`] and provides convenience methods for common Ethereum
/// RPC calls.
pub struct Api<H: Handler> {
    /// The underlying handler.
    pub handler: H,
}

impl<H: Handler> Api<H> {
    /// Wraps `handler` in an [`Api`].
    pub fn new(handler: H) -> Api<H> {
        Api { handler }
    }

    /// Performs a JSON-RPC call with positional arguments.
    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        self.handler.call(method, params).await
    }

    /// Performs a call and deserializes the result into `T`.
    pub async fn to<T: DeserializeOwned>(&self, method: &str, params: Vec<Value>) -> Result<T> {
        crate::decode::read_as(self.handler.call(method, params).await)
    }

    /// Returns the current block number from the connected node.
    pub async fn block_number(&self) -> Result<u64> {
        read_u64(self.handler.call("eth_blockNumber", vec![]).await)
    }

    /// Returns the chain id of the connected network.
    pub async fn chain_id(&self) -> Result<u64> {
        read_u64(self.handler.call("eth_chainId", vec![]).await)
    }
}
