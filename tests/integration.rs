mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::{rpc_err, rpc_ok, serve, Reply};
use ethrpc_rs::{evaluate, Error, Handler, Request, Rpc, RpcList, ValueExt};
use serde_json::json;

#[tokio::test]
async fn basic_call_and_to() {
    let url = serve(|req| rpc_ok(req, json!("0x1b4"))).await;
    let rpc = Rpc::new(url);

    let result = rpc.call("eth_blockNumber", vec![]).await.unwrap();
    assert_eq!(result.to_u64().unwrap(), 436);

    let s: String = rpc.call_as("eth_blockNumber", vec![]).await.unwrap();
    assert_eq!(s, "0x1b4");
}

#[tokio::test]
async fn jsonrpc_error_is_rpc_error() {
    let url = serve(|req| rpc_err(req, -32601, "Method not found")).await;
    let rpc = Rpc::new(url);

    let err = rpc.call("nonexistent_method", vec![]).await.unwrap_err();
    let eo = err.as_rpc_error().expect("expected ErrorObject");
    assert_eq!(eo.code, -32601);
}

#[tokio::test]
async fn override_runs_locally() {
    let mut rpc = Rpc::new("");
    rpc.set_override("test_method", |_args| Ok(json!("overridden")));

    let result = rpc.call("test_method", vec![]).await.unwrap();
    assert_eq!(result, json!("overridden"));
}

#[tokio::test]
async fn no_host_no_override_errors() {
    let rpc = Rpc::new("");
    let err = rpc.call("eth_blockNumber", vec![]).await.unwrap_err();
    assert!(matches!(err, Error::NotFound));
}

#[tokio::test]
async fn basic_auth() {
    let url = serve(|req| {
        let expected = format!("Basic {}", base64_std("myuser:mypass"));
        match req.header("Authorization") {
            Some(got) if got == expected => rpc_ok(req, json!("ok")),
            _ => Reply::status(401, b"unauthorized".to_vec()),
        }
    })
    .await;

    let mut rpc = Rpc::new(url);
    rpc.set_basic_auth("myuser", "mypass");
    let s: String = rpc.call_as("test", vec![]).await.unwrap();
    assert_eq!(s, "ok");
}

#[tokio::test]
async fn null_body_does_not_panic() {
    let url = serve(|_req| Reply::ok(b"null".to_vec())).await;
    let rpc = Rpc::new(url);
    // Tolerate any error, but never panic.
    let _ = rpc.call("eth_blockNumber", vec![]).await;
}

#[tokio::test]
async fn http_error_with_jsonrpc_body() {
    let url = serve(|req| {
        let body = json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid Request"},"id":req.rpc_id()});
        Reply::status(400, serde_json::to_vec(&body).unwrap())
    })
    .await;
    let rpc = Rpc::new(url);
    let err = rpc.call("bogus", vec![]).await.unwrap_err();
    assert_eq!(err.as_rpc_error().unwrap().code, -32600);
}

#[tokio::test]
async fn http_error_with_html_body() {
    let url = serve(|_req| Reply::status(502, b"<html>502 Bad Gateway</html>".to_vec())).await;
    let rpc = Rpc::new(url);
    let err = rpc.call("eth_blockNumber", vec![]).await.unwrap_err();
    assert!(err.to_string().contains("HTTP 502"), "got: {err}");
}

#[tokio::test]
async fn send_raw_request() {
    let url = serve(|req| rpc_ok(req, json!("0x1"))).await;
    let rpc = Rpc::new(url);
    let res = rpc
        .send(&Request::new("eth_chainId", vec![]))
        .await
        .unwrap();
    assert_eq!(res, json!("0x1"));
}

// ---- evaluator ----

#[tokio::test]
async fn rpclist_empty() {
    let list = RpcList(vec![]);
    let err = list.call("eth_blockNumber", vec![]).await.unwrap_err();
    assert!(matches!(err, Error::NoAvailableServer));
}

#[tokio::test]
async fn evaluate_no_servers() {
    assert!(matches!(evaluate(&[]).await, Err(Error::NoAvailableServer)));
}

#[tokio::test]
async fn evaluate_single_server() {
    let url = serve(|req| rpc_ok(req, json!("0x1"))).await;
    let h = evaluate(&[url.as_str()]).await.unwrap();
    let v = h.call("eth_blockNumber", vec![]).await.unwrap();
    assert_eq!(v.to_u64().unwrap(), 1);
}

#[tokio::test]
async fn evaluate_multiple_servers() {
    let url1 = serve(|req| rpc_ok(req, json!("0xa"))).await;
    let url2 = serve(|req| rpc_ok(req, json!("0xa"))).await;
    let h = evaluate(&[url1.as_str(), url2.as_str()]).await.unwrap();
    let v = h.call("eth_blockNumber", vec![]).await.unwrap();
    assert_eq!(v.to_u64().unwrap(), 10);
}

#[tokio::test]
async fn rpclist_failover() {
    let bad = serve(|_req| Reply::status(502, Vec::new())).await;
    let good = serve(|req| rpc_ok(req, json!("0x2a"))).await;
    let list = RpcList(vec![Rpc::new(bad), Rpc::new(good)]);
    let res = list.call("eth_blockNumber", vec![]).await.unwrap();
    assert_eq!(res.to_u64().unwrap(), 0x2a);
}

#[tokio::test]
async fn rpclist_no_failover_on_jsonrpc_error() {
    let good_calls = Arc::new(AtomicUsize::new(0));
    let first = serve(|req| rpc_err(req, -32601, "Method not found")).await;
    let counter = good_calls.clone();
    let second = serve(move |req| {
        counter.fetch_add(1, Ordering::SeqCst);
        rpc_ok(req, json!("0x1"))
    })
    .await;

    let list = RpcList(vec![Rpc::new(first), Rpc::new(second)]);
    let err = list.call("any", vec![]).await.unwrap_err();
    assert_eq!(err.as_rpc_error().unwrap().code, -32601);
    assert_eq!(good_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn evaluate_single_server_failure() {
    let url = serve(|_req| Reply::status(502, Vec::new())).await;
    let err = evaluate(&[url.as_str()]).await;
    assert!(err.is_err());
}

// ---- abi / contract calls ----

#[cfg(feature = "abi")]
#[tokio::test]
async fn eth_call_abi_balance_of() {
    use ethrpc_rs::abi::{eth_call_abi, ParamType, Token};
    use num_bigint::BigInt;

    // Mock node: verify the eth_call carries balanceOf's selector, then answer
    // with an ABI-encoded uint256 of 100.
    let url = serve(|req| {
        let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        let data = v["params"][0]["data"].as_str().unwrap_or("");
        assert!(data.starts_with("0x70a08231"), "wrong selector: {data}");
        // 100 == 0x64, left-padded to 32 bytes.
        let word = format!("0x{:0>64x}", 100);
        rpc_ok(req, json!(word))
    })
    .await;

    let rpc = Rpc::new(url);
    let out = eth_call_abi(
        &rpc,
        "0x0000000000000000000000000000000000000dead",
        "balanceOf(address)",
        &[Token::address("0x0000000000000000000000000000000000000001").unwrap()],
        &[ParamType::Uint(256)],
    )
    .await
    .unwrap();
    assert_eq!(out[0].as_uint().unwrap(), &BigInt::from(100));
}

// ---- forward ----

#[tokio::test]
async fn forward_override_local() {
    use ethrpc_rs::ForwardOptions;
    let mut rpc = Rpc::new("");
    rpc.set_override("eth_chainId", |_args| Ok(json!("0x1")));

    let resp = rpc
        .forward(
            &Request::new("eth_chainId", vec![]),
            &ForwardOptions::default(),
        )
        .await;
    assert_eq!(resp.status, 200);
    let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(v["result"], json!("0x1"));
    assert_eq!(v["jsonrpc"], json!("2.0"));
}

#[tokio::test]
async fn forward_proxies_node() {
    use ethrpc_rs::ForwardOptions;
    let url = serve(|req| rpc_ok(req, json!("0x1b4"))).await;
    let rpc = Rpc::new(url);

    let resp = rpc
        .forward(
            &Request::new("eth_blockNumber", vec![]),
            &ForwardOptions::default(),
        )
        .await;
    assert_eq!(resp.status, 200);
    let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(v["result"], json!("0x1b4"));
    // Hop-by-hop headers must not be forwarded.
    assert!(!resp
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("connection")));
}

#[tokio::test]
async fn forward_no_host_404() {
    use ethrpc_rs::ForwardOptions;
    let rpc = Rpc::new("");
    let resp = rpc
        .forward(
            &Request::new("eth_blockNumber", vec![]),
            &ForwardOptions::default(),
        )
        .await;
    assert_eq!(resp.status, 404);
}

fn base64_std(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s)
}
