//! Minimal Ethereum contract-call helpers (behind the default `abi` feature).
//!
//! Covers the common ABI types — `address`, `uint<M>`, `int<M>`, `bool`,
//! `bytes<N>`, dynamic `bytes`/`string`, and arrays of those — which is enough
//! for ERC-20/721 reads and most `view` calls. It is a deliberately small codec,
//! not a full ABI implementation: tuples/structs and nested-array corner cases
//! are out of scope.
//!
//! ```no_run
//! use ethrpc_rs::{Rpc, abi::{eth_call_abi, ParamType, Token}};
//! use num_bigint::BigInt;
//!
//! # async fn ex() -> Result<(), ethrpc_rs::Error> {
//! let rpc = Rpc::new("https://cloudflare-eth.com");
//! // balanceOf(address) -> uint256
//! let out = eth_call_abi(
//!     &rpc,
//!     "0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT
//!     "balanceOf(address)",
//!     &[Token::address("0x28C6c06298d514Db089934071355E5743bf21d60")?],
//!     &[ParamType::Uint(256)],
//! )
//! .await?;
//! let balance: &BigInt = out[0].as_uint().unwrap();
//! println!("balance: {balance}");
//! # Ok(()) }
//! ```

use num_bigint::{BigInt, Sign};
use serde_json::{json, Map, Value};

use crate::decode::ValueExt;
use crate::error::{Error, Result};
use crate::rpc::Handler;

/// A decoded/encodable ABI value. The variant, not a separate type string,
/// carries what is needed to encode it.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A 20-byte `address`.
    Address([u8; 20]),
    /// An unsigned integer (`uint<M>`); always encoded in 32 bytes.
    Uint(BigInt),
    /// A signed integer (`int<M>`); two's-complement in 32 bytes.
    Int(BigInt),
    /// A `bool`.
    Bool(bool),
    /// A fixed byte array `bytes<N>` (`N <= 32`), left-aligned in its word.
    FixedBytes(Vec<u8>),
    /// Dynamic `bytes`.
    Bytes(Vec<u8>),
    /// Dynamic `string` (UTF-8).
    String(String),
    /// A homogeneous array `T[]`.
    Array(Vec<Token>),
}

impl Token {
    /// Builds an [`Token::Address`] from a `0x`-prefixed (or bare) 40-hex-char
    /// string.
    pub fn address(s: &str) -> Result<Token> {
        let bytes = from_hex(strip_0x(s))?;
        if bytes.len() != 20 {
            return Err(Error::Other(format!(
                "address must be 20 bytes, got {}",
                bytes.len()
            )));
        }
        let mut a = [0u8; 20];
        a.copy_from_slice(&bytes);
        Ok(Token::Address(a))
    }

    /// Convenience constructor for a `uint256` from any integer.
    pub fn uint(v: impl Into<BigInt>) -> Token {
        Token::Uint(v.into())
    }

    /// Returns the integer value for [`Token::Uint`]/[`Token::Int`].
    pub fn as_uint(&self) -> Option<&BigInt> {
        match self {
            Token::Uint(v) | Token::Int(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the 20-byte address for [`Token::Address`].
    pub fn as_address(&self) -> Option<[u8; 20]> {
        match self {
            Token::Address(a) => Some(*a),
            _ => None,
        }
    }

    /// Returns the address as a lowercase `0x`-prefixed hex string.
    pub fn as_address_hex(&self) -> Option<String> {
        self.as_address().map(|a| format!("0x{}", to_hex(&a)))
    }

    /// Returns the boolean value for [`Token::Bool`].
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Token::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the bytes for [`Token::Bytes`]/[`Token::FixedBytes`].
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Token::Bytes(b) | Token::FixedBytes(b) => Some(b),
            _ => None,
        }
    }

    /// Returns the string for [`Token::String`].
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Token::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the elements for [`Token::Array`].
    pub fn as_array(&self) -> Option<&[Token]> {
        match self {
            Token::Array(v) => Some(v),
            _ => None,
        }
    }

    fn is_dynamic(&self) -> bool {
        matches!(self, Token::Bytes(_) | Token::String(_) | Token::Array(_))
    }
}

/// The ABI type of a value to decode. Needed for decoding a call's return data,
/// since the raw bytes carry no type information.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// `address`
    Address,
    /// `uint<M>` (the bit width is informational; decoding always reads 32 bytes)
    Uint(usize),
    /// `int<M>`
    Int(usize),
    /// `bool`
    Bool,
    /// `bytes<N>`
    FixedBytes(usize),
    /// dynamic `bytes`
    Bytes,
    /// `string`
    String,
    /// `T[]`
    Array(Box<ParamType>),
}

impl ParamType {
    fn is_dynamic(&self) -> bool {
        matches!(
            self,
            ParamType::Bytes | ParamType::String | ParamType::Array(_)
        )
    }
}

/// Computes the 4-byte function selector `keccak256(signature)[..4]`.
///
/// `signature` must be the canonical form, e.g. `"transfer(address,uint256)"`.
pub fn function_selector(signature: &str) -> [u8; 4] {
    let h = purecrypto::hash::keccak256(signature.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

/// Encodes a full calldata payload: the 4-byte selector followed by the
/// ABI-encoded `args`.
pub fn encode_call(signature: &str, args: &[Token]) -> Vec<u8> {
    let mut out = function_selector(signature).to_vec();
    out.extend(encode(args));
    out
}

/// ABI-encodes a list of tokens as a head/tail tuple.
pub fn encode(tokens: &[Token]) -> Vec<u8> {
    let head_len: usize = tokens.len() * 32;
    let mut head = Vec::with_capacity(head_len);
    let mut tail = Vec::new();
    for t in tokens {
        if t.is_dynamic() {
            head.extend_from_slice(&word_usize(head_len + tail.len()));
            tail.extend(encode_value(t));
        } else {
            head.extend(encode_value(t));
        }
    }
    head.extend(tail);
    head
}

/// Encodes a single token (static value inline, or dynamic payload for the tail).
fn encode_value(t: &Token) -> Vec<u8> {
    match t {
        Token::Address(a) => {
            let mut w = [0u8; 32];
            w[12..].copy_from_slice(a);
            w.to_vec()
        }
        Token::Uint(v) => encode_uint(v),
        Token::Int(v) => encode_int(v),
        Token::Bool(b) => {
            let mut w = [0u8; 32];
            w[31] = *b as u8;
            w.to_vec()
        }
        Token::FixedBytes(b) => {
            let mut w = [0u8; 32];
            w[..b.len()].copy_from_slice(b);
            w.to_vec()
        }
        Token::Bytes(b) => encode_dynamic_bytes(b),
        Token::String(s) => encode_dynamic_bytes(s.as_bytes()),
        Token::Array(elems) => {
            let mut out = word_usize(elems.len()).to_vec();
            out.extend(encode(elems));
            out
        }
    }
}

fn encode_dynamic_bytes(b: &[u8]) -> Vec<u8> {
    let mut out = word_usize(b.len()).to_vec();
    out.extend_from_slice(b);
    // Right-pad the payload up to a 32-byte boundary.
    let padded = b.len().div_ceil(32) * 32;
    out.resize(32 + padded, 0);
    out
}

fn encode_uint(v: &BigInt) -> Vec<u8> {
    let (sign, mag) = v.to_bytes_be();
    if sign == Sign::Minus {
        // Callers should use Token::Int for negatives; treat as two's complement.
        return encode_int(v);
    }
    let mut w = [0u8; 32];
    let start = 32usize.saturating_sub(mag.len());
    w[start..].copy_from_slice(&mag[mag.len().saturating_sub(32)..]);
    w.to_vec()
}

fn encode_int(v: &BigInt) -> Vec<u8> {
    let encoded = if v.sign() == Sign::Minus {
        // two's complement over 256 bits
        let modulus = BigInt::from(1) << 256;
        modulus + v
    } else {
        v.clone()
    };
    let (_, mag) = encoded.to_bytes_be();
    let mut w = [0u8; 32];
    let take = mag.len().min(32);
    w[32 - take..].copy_from_slice(&mag[mag.len() - take..]);
    w.to_vec()
}

/// ABI-decodes `data` according to `types`.
pub fn decode(types: &[ParamType], data: &[u8]) -> Result<Vec<Token>> {
    decode_tuple(types, data, 0)
}

fn decode_tuple(types: &[ParamType], data: &[u8], base: usize) -> Result<Vec<Token>> {
    let mut out = Vec::with_capacity(types.len());
    for (i, ty) in types.iter().enumerate() {
        let head_pos = base + i * 32;
        if ty.is_dynamic() {
            let off = read_usize(data, head_pos)?;
            out.push(decode_value(ty, data, base + off)?);
        } else {
            out.push(decode_static(ty, data, head_pos)?);
        }
    }
    Ok(out)
}

fn decode_static(ty: &ParamType, data: &[u8], pos: usize) -> Result<Token> {
    let w = word(data, pos)?;
    Ok(match ty {
        ParamType::Address => {
            let mut a = [0u8; 20];
            a.copy_from_slice(&w[12..32]);
            Token::Address(a)
        }
        ParamType::Uint(_) => Token::Uint(BigInt::from_bytes_be(Sign::Plus, w)),
        ParamType::Int(_) => {
            let mut v = BigInt::from_bytes_be(Sign::Plus, w);
            if w[0] & 0x80 != 0 {
                v -= BigInt::from(1) << 256;
            }
            Token::Int(v)
        }
        ParamType::Bool => Token::Bool(w.iter().any(|&b| b != 0)),
        ParamType::FixedBytes(n) => Token::FixedBytes(w[..(*n).min(32)].to_vec()),
        _ => return Err(Error::Other("decode_static called on dynamic type".into())),
    })
}

fn decode_value(ty: &ParamType, data: &[u8], pos: usize) -> Result<Token> {
    match ty {
        ParamType::Bytes => {
            let len = read_usize(data, pos)?;
            let start = pos + 32;
            let end = start
                .checked_add(len)
                .ok_or_else(|| Error::Other("abi length overflow".into()))?;
            slice(data, start, end).map(|s| Token::Bytes(s.to_vec()))
        }
        ParamType::String => {
            let len = read_usize(data, pos)?;
            let start = pos + 32;
            let end = start
                .checked_add(len)
                .ok_or_else(|| Error::Other("abi length overflow".into()))?;
            let s = slice(data, start, end)?;
            Ok(Token::String(String::from_utf8(s.to_vec()).map_err(
                |_| Error::Other("invalid utf-8 in string".into()),
            )?))
        }
        ParamType::Array(inner) => {
            let len = read_usize(data, pos)?;
            let elem_base = pos + 32;
            let types = vec![(**inner).clone(); len];
            Ok(Token::Array(decode_tuple(&types, data, elem_base)?))
        }
        _ => decode_static(ty, data, pos),
    }
}

/// Performs an `eth_call` against `to` with ABI-encoded `args` and decodes the
/// return value per `returns`. `signature` is the canonical function signature
/// used to compute the selector, e.g. `"balanceOf(address)"`.
pub async fn eth_call_abi<H: Handler + ?Sized>(
    handler: &H,
    to: &str,
    signature: &str,
    args: &[Token],
    returns: &[ParamType],
) -> Result<Vec<Token>> {
    let calldata = encode_call(signature, args);
    let mut tx = Map::new();
    tx.insert("to".to_string(), json!(to));
    tx.insert(
        "data".to_string(),
        json!(format!("0x{}", to_hex(&calldata))),
    );

    let ret = handler
        .call("eth_call", vec![Value::Object(tx), json!("latest")])
        .await?;
    let hex = ret.to_str()?;
    let bytes = from_hex(strip_0x(hex))?;
    decode(returns, &bytes)
}

// ---- word / hex helpers ----

fn word_usize(v: usize) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&(v as u64).to_be_bytes());
    w
}

fn word(data: &[u8], pos: usize) -> Result<&[u8]> {
    slice(data, pos, pos + 32)
}

fn read_usize(data: &[u8], pos: usize) -> Result<usize> {
    let w = word(data, pos)?;
    // Guard against values that don't fit in usize (the top 24 bytes must be 0).
    if w[..24].iter().any(|&b| b != 0) {
        return Err(Error::Other("abi offset/length too large".into()));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&w[24..32]);
    Ok(u64::from_be_bytes(buf) as usize)
}

fn slice(data: &[u8], start: usize, end: usize) -> Result<&[u8]> {
    data.get(start..end)
        .ok_or_else(|| Error::Other("abi data truncated".into()))
}

fn strip_0x(s: &str) -> &str {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn from_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(Error::Other("odd-length hex string".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Other("invalid hex".into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        from_hex(strip_0x(s)).unwrap()
    }

    #[test]
    fn selectors() {
        assert_eq!(to_hex(&function_selector("balanceOf(address)")), "70a08231");
        assert_eq!(
            to_hex(&function_selector("transfer(address,uint256)")),
            "a9059cbb"
        );
    }

    #[test]
    fn encode_transfer_calldata() {
        // transfer(0x00..0064, 1) — canonical ABI encoding.
        let to = Token::address("0x0000000000000000000000000000000000000064").unwrap();
        let calldata = encode_call("transfer(address,uint256)", &[to, Token::uint(1u64)]);
        let expected = hex("a9059cbb\
             0000000000000000000000000000000000000000000000000000000000000064\
             0000000000000000000000000000000000000000000000000000000000000001");
        assert_eq!(calldata, expected);
    }

    #[test]
    fn roundtrip_static() {
        let tokens = vec![
            Token::address("0x00000000000000000000000000000000000000ff").unwrap(),
            Token::uint(12345u64),
            Token::Bool(true),
        ];
        let enc = encode(&tokens);
        let dec = decode(
            &[ParamType::Address, ParamType::Uint(256), ParamType::Bool],
            &enc,
        )
        .unwrap();
        assert_eq!(dec, tokens);
    }

    #[test]
    fn roundtrip_dynamic_string_and_bytes() {
        let tokens = vec![
            Token::String("hello world, this is longer than 32 bytes!!".to_string()),
            Token::Uint(BigInt::from(7)),
            Token::Bytes(vec![1, 2, 3, 4, 5]),
        ];
        let enc = encode(&tokens);
        let dec = decode(
            &[ParamType::String, ParamType::Uint(256), ParamType::Bytes],
            &enc,
        )
        .unwrap();
        assert_eq!(dec, tokens);
    }

    #[test]
    fn roundtrip_array_of_addresses() {
        let arr = Token::Array(vec![
            Token::address("0x0000000000000000000000000000000000000001").unwrap(),
            Token::address("0x0000000000000000000000000000000000000002").unwrap(),
        ]);
        let enc = encode(std::slice::from_ref(&arr));
        let dec = decode(&[ParamType::Array(Box::new(ParamType::Address))], &enc).unwrap();
        assert_eq!(dec, vec![arr]);
    }

    #[test]
    fn decode_negative_int() {
        // int256(-1) is all 0xff.
        let data = hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
        let dec = decode(&[ParamType::Int(256)], &data).unwrap();
        assert_eq!(dec[0].as_uint().unwrap(), &BigInt::from(-1));
        // And it round-trips through encoding.
        assert_eq!(encode(&[Token::Int(BigInt::from(-1))]), data);
    }

    #[test]
    fn decode_uint_string_result() {
        // Simulate an ERC-20 `symbol()` returning "USDC".
        let tokens = vec![Token::String("USDC".to_string())];
        let enc = encode(&tokens);
        let dec = decode(&[ParamType::String], &enc).unwrap();
        assert_eq!(dec[0].as_string(), Some("USDC"));
    }
}
