//! Decode helpers for RPC results.
//!
//! These mirror the Go `ReadUint64` / `ReadBigInt` / `ReadString` / `ReadTo` /
//! `ReadAs` helpers. Each takes the `Result` of a [`call`](crate::RPC::call) so
//! it can be chained directly:
//!
//! ```no_run
//! # use ethrpc_rs::{RPC, read_u64};
//! # async fn ex(rpc: &RPC) -> Result<(), ethrpc_rs::Error> {
//! let block = read_u64(rpc.call("eth_blockNumber", vec![]).await)?;
//! # let _ = block; Ok(()) }
//! ```

use num_bigint::BigInt;
use num_traits::Num;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Result};

/// Parses an unsigned integer from a string, auto-detecting the base from a
/// `0x`/`0o`/`0b` prefix (or leading `0` for octal), mirroring Go's
/// `strconv.ParseUint(s, 0, 64)`.
fn parse_uint_auto(s: &str) -> Result<u64> {
    let (radix, digits) = split_radix(s);
    u64::from_str_radix(digits, radix).map_err(|e| Error::Other(e.to_string()))
}

/// Splits an optionally-prefixed integer literal into its radix and the
/// remaining digits. Recognizes `0x`/`0X`, `0o`/`0O`, `0b`/`0B`, and a bare
/// leading `0` (octal), matching Go's base-0 parsing.
fn split_radix(s: &str) -> (u32, &str) {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        match bytes[1] {
            b'x' | b'X' => return (16, &s[2..]),
            b'o' | b'O' => return (8, &s[2..]),
            b'b' | b'B' => return (2, &s[2..]),
            // Leading zero with more digits => octal (Go base-0 behavior).
            b'0'..=b'7' => return (8, &s[1..]),
            _ => {}
        }
    }
    (10, s)
}

/// Decodes the value as a `u64`. Accepts a hex/decimal JSON string (e.g.
/// `"0x1b4"`) or a JSON number. Propagates an upstream error unchanged.
pub fn read_u64(v: Result<Value>) -> Result<u64> {
    let v = v?;
    match v {
        Value::String(s) => parse_uint_auto(&s),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| Error::Other(format!("value {n} is not a u64"))),
        other => Err(Error::Other(format!("cannot decode {other} as u64"))),
    }
}

/// Decodes the value as a [`BigInt`]. Accepts a hex/decimal JSON string or a
/// JSON number. Propagates an upstream error unchanged.
pub fn read_big_int(v: Result<Value>) -> Result<BigInt> {
    let v = v?;
    match v {
        Value::String(s) => {
            let (radix, digits) = split_radix(&s);
            BigInt::from_str_radix(digits, radix)
                .map_err(|_| Error::Other("invalid integer value".to_string()))
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(BigInt::from(i))
            } else if let Some(u) = n.as_u64() {
                Ok(BigInt::from(u))
            } else {
                // Fall back to the decimal text representation for very large
                // integers that don't fit i64/u64.
                BigInt::from_str_radix(&n.to_string(), 10)
                    .map_err(|_| Error::Other("invalid integer value".to_string()))
            }
        }
        other => Err(Error::Other(format!("cannot decode {other} as integer"))),
    }
}

/// Decodes the value as a string. Propagates an upstream error unchanged.
pub fn read_string(v: Result<Value>) -> Result<String> {
    let v = v?;
    match v {
        Value::String(s) => Ok(s),
        other => Err(Error::Other(format!("cannot decode {other} as string"))),
    }
}

/// Deserializes the value into any [`DeserializeOwned`] type `T`. Replaces both
/// Go's `ReadTo` and `ReadAs`. Propagates an upstream error unchanged.
pub fn read_as<T: DeserializeOwned>(v: Result<Value>) -> Result<T> {
    let v = v?;
    Ok(serde_json::from_value(v)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;
    use serde_json::json;

    fn ok(v: Value) -> Result<Value> {
        Ok(v)
    }
    fn err() -> Result<Value> {
        Err(Error::Other("rpc failed".to_string()))
    }

    #[test]
    fn u64_variants() {
        assert_eq!(read_u64(ok(json!("0x1b4"))).unwrap(), 436);
        assert_eq!(read_u64(ok(json!("100"))).unwrap(), 100);
        assert_eq!(read_u64(ok(json!(42))).unwrap(), 42);
        assert_eq!(read_u64(ok(json!("0x0"))).unwrap(), 0);
        assert!(read_u64(err()).is_err());
        assert!(read_u64(ok(json!("notanumber"))).is_err());
        assert!(read_u64(ok(json!({}))).is_err());
    }

    #[test]
    fn big_int_variants() {
        assert_eq!(read_big_int(ok(json!("0x1b4"))).unwrap(), BigInt::from(436));
        assert_eq!(read_big_int(ok(json!("100"))).unwrap(), BigInt::from(100));
        assert_eq!(read_big_int(ok(json!(42))).unwrap(), BigInt::from(42));
        assert_eq!(
            read_big_int(ok(json!("0xDE0B6B3A7640000"))).unwrap(),
            BigInt::from(1_000_000_000_000_000_000u64)
        );
        assert!(read_big_int(err()).is_err());
        assert!(read_big_int(ok(json!("notanumber"))).is_err());
    }

    #[test]
    fn string_variants() {
        assert_eq!(read_string(ok(json!("hello"))).unwrap(), "hello");
        assert_eq!(read_string(ok(json!(""))).unwrap(), "");
        assert_eq!(read_string(ok(json!("0xdead"))).unwrap(), "0xdead");
        assert!(read_string(err()).is_err());
        assert!(read_string(ok(json!(123))).is_err());
    }

    #[test]
    fn as_struct() {
        #[derive(serde::Deserialize)]
        struct Block {
            number: String,
        }
        let got: Block = read_as(ok(json!({"number":"0x1b4"}))).unwrap();
        assert_eq!(got.number, "0x1b4");
        assert!(read_as::<String>(err()).is_err());
    }
}
