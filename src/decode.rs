//! Decode helpers for Ethereum RPC results.
//!
//! Ethereum encodes integer "quantities" as hex strings (e.g. `"0x1b4"`), which
//! serde won't parse into a number on its own. [`ValueExt`] adds methods to
//! [`serde_json::Value`] that decode these, so a call result is decoded with
//! ordinary method syntax and the `?` operator:
//!
//! ```no_run
//! use ethrpc_rs::{Rpc, ValueExt};
//! # async fn ex(rpc: &Rpc) -> Result<(), ethrpc_rs::Error> {
//! let block = rpc.call("eth_blockNumber", vec![]).await?.to_u64()?;
//! # let _ = block; Ok(()) }
//! ```

use num_bigint::BigInt;
use num_traits::Num;
use serde_json::Value;

use crate::error::{Error, Result};

/// Decoding helpers for Ethereum-encoded [`serde_json::Value`] results.
///
/// Implemented for [`serde_json::Value`], so any RPC call result can be decoded
/// in place: `rpc.call(...).await?.to_u64()?`.
pub trait ValueExt {
    /// Decodes an Ethereum quantity as a `u64`. Accepts a hex/decimal JSON
    /// string (e.g. `"0x1b4"`) or a JSON number.
    fn to_u64(&self) -> Result<u64>;

    /// Decodes an Ethereum quantity as a [`BigInt`]. Accepts a hex/decimal JSON
    /// string or a JSON number.
    fn to_big_int(&self) -> Result<BigInt>;

    /// Returns the value as a string slice, erroring if it is not a JSON string.
    fn to_str(&self) -> Result<&str>;
}

impl ValueExt for Value {
    fn to_u64(&self) -> Result<u64> {
        match self {
            Value::String(s) => parse_uint_auto(s),
            Value::Number(n) => n
                .as_u64()
                .ok_or_else(|| Error::Other(format!("value {n} is not a u64"))),
            other => Err(Error::Other(format!("cannot decode {other} as u64"))),
        }
    }

    fn to_big_int(&self) -> Result<BigInt> {
        match self {
            Value::String(s) => {
                let (radix, digits) = split_radix(s);
                BigInt::from_str_radix(digits, radix)
                    .map_err(|_| Error::Other("invalid integer value".to_string()))
            }
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(BigInt::from(i))
                } else if let Some(u) = n.as_u64() {
                    Ok(BigInt::from(u))
                } else {
                    // Very large integers that don't fit i64/u64: fall back to
                    // the decimal text representation.
                    BigInt::from_str_radix(&n.to_string(), 10)
                        .map_err(|_| Error::Other("invalid integer value".to_string()))
                }
            }
            other => Err(Error::Other(format!("cannot decode {other} as integer"))),
        }
    }

    fn to_str(&self) -> Result<&str> {
        self.as_str()
            .ok_or_else(|| Error::Other(format!("cannot decode {self} as string")))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;
    use serde_json::json;

    #[test]
    fn u64_variants() {
        assert_eq!(json!("0x1b4").to_u64().unwrap(), 436);
        assert_eq!(json!("100").to_u64().unwrap(), 100);
        assert_eq!(json!(42).to_u64().unwrap(), 42);
        assert_eq!(json!("0x0").to_u64().unwrap(), 0);
        assert!(json!("notanumber").to_u64().is_err());
        assert!(json!({}).to_u64().is_err());
    }

    #[test]
    fn big_int_variants() {
        assert_eq!(json!("0x1b4").to_big_int().unwrap(), BigInt::from(436));
        assert_eq!(json!("100").to_big_int().unwrap(), BigInt::from(100));
        assert_eq!(json!(42).to_big_int().unwrap(), BigInt::from(42));
        assert_eq!(
            json!("0xDE0B6B3A7640000").to_big_int().unwrap(),
            BigInt::from(1_000_000_000_000_000_000u64)
        );
        assert!(json!("notanumber").to_big_int().is_err());
    }

    #[test]
    fn str_variants() {
        assert_eq!(json!("hello").to_str().unwrap(), "hello");
        assert_eq!(json!("").to_str().unwrap(), "");
        assert_eq!(json!("0xdead").to_str().unwrap(), "0xdead");
        assert!(json!(123).to_str().is_err());
    }
}
