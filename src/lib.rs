//! A lightweight async library for making JSON-RPC calls to Ethereum-compatible
//! nodes. A Rust port of the Go [`ethrpc`](https://github.com/KarpelesLab/ethrpc)
//! library, built on the [`rsurl`](https://crates.io/crates/rsurl) async HTTP
//! client.
//!
//! # Quick start
//!
//! ```no_run
//! # #[cfg(feature = "rpc")]
//! # async fn ex() -> Result<(), ethrpc_rs::Error> {
//! use ethrpc_rs::{Rpc, ValueExt};
//!
//! let rpc = Rpc::new("https://cloudflare-eth.com");
//! let block = rpc.call("eth_blockNumber", vec![]).await?.to_u64()?;
//! println!("block: {block}");
//! # Ok(()) }
//! ```
//!
//! All network methods are `async`. On native targets they run on Tokio (via
//! rsurl's adapter); on `wasm32` they go through the browser's Fetch API, whose
//! futures are `!Send` — so on that target `Handler` drops its `Send` bounds
//! rather than asking for something the browser cannot give.
//!
//! # Feature flags
//!
//! - `rpc` *(default)* — the JSON-RPC client: `Rpc`, `Api`, `Handler`,
//!   `RpcList`, `evaluate`, and `abi::eth_call_abi`. Turning it off drops rsurl
//!   and the async stack, leaving the parts that need no network: [`chains`],
//!   [`ValueExt`], and the `abi` codec.
//! - `abi` *(default)* — the contract-call ABI codec, which pulls in
//!   `purecrypto` for Keccak-256 selectors.

#![warn(missing_docs)]

#[cfg(feature = "abi")]
pub mod abi;
#[cfg(feature = "rpc")]
mod api;
pub mod chains;
mod decode;
mod error;
#[cfg(feature = "rpc")]
mod evaluator;
mod jsonrpc;
#[cfg(feature = "rpc")]
mod rpc;

#[cfg(feature = "rpc")]
pub use api::Api;
pub use decode::ValueExt;
pub use error::{Error, Result};
#[cfg(feature = "rpc")]
pub use evaluator::{evaluate, RpcList};
pub use jsonrpc::{ErrorObject, Request, Response, ResponseIntf};
#[cfg(feature = "rpc")]
pub use rpc::{ForwardOptions, ForwardResponse, Handler, MaybeSendSync, OverrideFn, Rpc};
