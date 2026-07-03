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
use ethrpc_rs::{Rpc, ValueExt};

#[tokio::main]
async fn main() -> Result<(), ethrpc_rs::Error> {
    let rpc = Rpc::new("https://cloudflare-eth.com");
    let block = rpc.call("eth_blockNumber", vec![]).await?.to_u64()?;
    println!("block: {block}");
    Ok(())
}
```

## Features

### Positional and named arguments

```rust
use serde_json::json;

// Positional arguments
let balance = rpc.call("eth_getBalance", vec![json!(addr), json!("latest")]).await?.to_big_int()?;

// Named arguments
let mut params = serde_json::Map::new();
params.insert("to".into(), json!("0xContract"));
params.insert("data".into(), json!("0xCalldata"));
let result = rpc.call_named("eth_call", params).await?;
```

### Decode helpers

The [`ValueExt`] trait adds decoding methods to the `serde_json::Value` a call
returns, so results decode in place with `?` — no hex-string juggling:

```rust
use ethrpc_rs::ValueExt;

let block = rpc.call("eth_blockNumber", vec![]).await?.to_u64()?;
let balance = rpc.call("eth_getBalance", vec![json!(addr), json!("latest")]).await?.to_big_int()?;
let hash = rpc.call("eth_sendRawTransaction", vec![json!(signed_tx)]).await?.to_str()?.to_owned();

// Decode into any type implementing serde::Deserialize
let block: MyBlockType =
    rpc.call_as("eth_getBlockByNumber", vec![json!("0x1b4"), json!(true)]).await?;
```

### Deserialize into a target type

```rust
let peers: Vec<serde_json::Value> = rpc.call_as("net_peerCount", vec![]).await?;
```

### Basic authentication

```rust
let mut rpc = Rpc::new("https://my-node.example.com");
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
let block = handler.call("eth_blockNumber", vec![]).await?.to_u64()?;
```

### Contract calls (ABI)

The `abi` module (on by default) turns a function signature and typed arguments
into calldata, performs the `eth_call`, and decodes the result — no manual hex
juggling. It covers the common ABI types (`address`, `uint<M>`, `int<M>`, `bool`,
`bytes<N>`, dynamic `bytes`/`string`, and arrays of those), which is enough for
ERC-20/721 reads and most `view` calls:

```rust
use ethrpc_rs::abi::{eth_call_abi, ParamType, Token};

// balanceOf(address) -> uint256
let out = eth_call_abi(
    &rpc,
    "0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT
    "balanceOf(address)",
    &[Token::address("0x28C6c06298d514Db089934071355E5743bf21d60")?],
    &[ParamType::Uint(256)],
).await?;
let balance = out[0].as_uint().unwrap();
```

Selectors use Keccak-256 from [`purecrypto`](https://crates.io/crates/purecrypto).
Disable the whole thing (and that dependency) with `default-features = false` for
a lean raw-JSON-RPC build. Lower-level `encode`, `decode`, `encode_call`, and
`function_selector` helpers are exposed too.

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
- The `abi` contract-call helper (`eth_call_abi`) has no Go counterpart — the Go
  library only exposed raw JSON-RPC.

## License

MIT
