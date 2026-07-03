//! A lightweight async library for making JSON-RPC calls to Ethereum-compatible
//! nodes. A Rust port of the Go [`ethrpc`](https://github.com/KarpelesLab/ethrpc)
//! library, built on the [`rsurl`] async HTTP client.
//!
//! # Quick start
//!
//! ```no_run
//! use ethrpc_rs::{Rpc, ValueExt};
//!
//! # async fn ex() -> Result<(), ethrpc_rs::Error> {
//! let rpc = Rpc::new("https://cloudflare-eth.com");
//! let block = rpc.call("eth_blockNumber", vec![]).await?.to_u64()?;
//! println!("block: {block}");
//! # Ok(()) }
//! ```
//!
//! All network methods are `async` and must be awaited inside a Tokio runtime.

#![warn(missing_docs)]

#[cfg(feature = "abi")]
pub mod abi;
mod api;
pub mod chains;
mod decode;
mod error;
mod evaluator;
mod jsonrpc;
mod rpc;

pub use api::Api;
pub use decode::ValueExt;
pub use error::{Error, Result};
pub use evaluator::{evaluate, RpcList};
pub use jsonrpc::{ErrorObject, Request, Response, ResponseIntf};
pub use rpc::{ForwardOptions, ForwardResponse, Handler, OverrideFn, Rpc};
