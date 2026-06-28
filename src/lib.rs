//! A lightweight async library for making JSON-RPC calls to Ethereum-compatible
//! nodes. A Rust port of the Go [`ethrpc`](https://github.com/KarpelesLab/ethrpc)
//! library, built on the [`rsurl`] async HTTP client.
//!
//! # Quick start
//!
//! ```no_run
//! use ethrpc_rs::{RPC, read_u64};
//!
//! # async fn ex() -> Result<(), ethrpc_rs::Error> {
//! let rpc = RPC::new("https://cloudflare-eth.com");
//! let block = read_u64(rpc.call("eth_blockNumber", vec![]).await)?;
//! println!("block: {block}");
//! # Ok(()) }
//! ```
//!
//! All network methods are `async` and must be awaited inside a Tokio runtime.

#![warn(missing_docs)]

mod api;
pub mod chains;
mod decode;
mod error;
mod evaluator;
mod jsonrpc;
mod rpc;

pub use api::Api;
pub use decode::{read_as, read_big_int, read_string, read_u64};
pub use error::{Error, Result};
pub use evaluator::{evaluate, RpcList};
pub use jsonrpc::{ErrorObject, Request, Response, ResponseIntf};
pub use rpc::{ForwardOptions, ForwardResponse, Handler, OverrideFn, RPC};
