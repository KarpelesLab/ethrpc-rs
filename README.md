[![Crates.io](https://img.shields.io/crates/v/ethrpc-rs.svg)](https://crates.io/crates/ethrpc-rs)
[![Docs.rs](https://docs.rs/ethrpc-rs/badge.svg)](https://docs.rs/ethrpc-rs)

# ethrpc-rs

A lightweight async Rust library for making JSON-RPC calls to Ethereum-compatible
nodes. A port of the Go [`ethrpc`](https://github.com/KarpelesLab/ethrpc) library,
built on the async [`rsurl`](https://crates.io/crates/rsurl) HTTP client and Tokio.

## Install

```bash
cargo add ethrpc-rs
```

All network methods are `async` and run inside a Tokio runtime.

## Quick start

```rust
use ethrpc_rs::{RPC, read_u64};

#[tokio::main]
async fn main() -> Result<(), ethrpc_rs::Error> {
    let rpc = RPC::new("https://cloudflare-eth.com");
    let block = read_u64(rpc.call("eth_blockNumber", vec![]).await)?;
    println!("block: {block}");
    Ok(())
}
```

## Features

### Positional and named arguments

```rust
use serde_json::json;

// Positional arguments
let balance = read_big_int(rpc.call("eth_getBalance", vec![json!(addr), json!("latest")]).await)?;

// Named arguments
let mut params = serde_json::Map::new();
params.insert("to".into(), json!("0xContract"));
params.insert("data".into(), json!("0xCalldata"));
let result = rpc.call_named("eth_call", params).await?;
```

### Decode helpers

Decoders wrap the `Result` of a `call`, so they chain directly:

```rust
let block = read_u64(rpc.call("eth_blockNumber", vec![]).await)?;
let balance = read_big_int(rpc.call("eth_getBalance", vec![json!(addr), json!("latest")]).await)?;
let hash = read_string(rpc.call("eth_sendRawTransaction", vec![json!(signed_tx)]).await)?;

// Decode into any type implementing serde::Deserialize
let block: MyBlockType = read_as(rpc.call("eth_getBlockByNumber", vec![json!("0x1b4"), json!(true)]).await)?;
```

### Unmarshal into a target type

```rust
let peers: Vec<serde_json::Value> = rpc.to("net_peerCount", vec![]).await?;
```

### Basic authentication

```rust
let mut rpc = RPC::new("https://my-node.example.com");
rpc.set_basic_auth("user", "password");
```

### Method overrides

Intercept RPC methods locally without hitting the remote node:

```rust
rpc.set_override("eth_chainId", |_args| Ok(serde_json::json!("0x1")));
```

### Server evaluation

Select the fastest endpoints by racing `eth_blockNumber` calls:

```rust
let handler = ethrpc_rs::evaluate(&[
    "https://node1.example.com",
    "https://node2.example.com",
    "https://node3.example.com",
]).await?;
// handler implements ethrpc_rs::Handler with the best responding servers
let block = read_u64(handler.call("eth_blockNumber", vec![]).await)?;
```

### HTTP response forwarding

Build a JSON-RPC response (running overrides locally or proxying to the node,
stripping hop-by-hop headers) ready to write to any HTTP framework:

```rust
use ethrpc_rs::{ForwardOptions, Request};
use std::time::Duration;

let resp = rpc.forward(
    &Request::new("eth_blockNumber", vec![]),
    &ForwardOptions { pretty: true, cache: Some(Duration::from_secs(30)) },
).await;
// resp.status, resp.headers, resp.body
```

### Chain metadata

The `chains` module provides static metadata for known EVM-compatible chains:

```rust
let eth = ethrpc_rs::chains::get(1).unwrap();          // Ethereum Mainnet
println!("{}", eth.name);                              // "Ethereum Mainnet"
println!("{}", eth.native_currency.as_ref().unwrap().symbol); // "ETH"
println!("{}", eth.has_feature("EIP1559"));            // true
println!("{:?}", eth.transaction_url("0xabc..."));     // Some("https://etherscan.io/tx/0xabc...")
println!("{:?}", eth.explorer_url());                  // Some("https://etherscan.io")
```

## Differences from the Go library

- All RPC methods are `async`; cancellation is via dropping the future or
  `tokio::time::timeout` rather than a `context.Context`.
- `Forward` returns a framework-agnostic `ForwardResponse { status, headers, body }`
  instead of writing to an `http.ResponseWriter`.
- Method overrides are closures `Fn(&[Value]) -> Result<Value>` rather than
  reflection-based arbitrary Go functions.

## License

MIT
