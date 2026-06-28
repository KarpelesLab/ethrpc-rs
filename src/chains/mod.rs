//! Static metadata for known EVM-compatible chains.
//!
//! The full chain registry is embedded in the binary and parsed once on first
//! access.
//!
//! ```
//! let eth = ethrpc_rs::chains::get(1).unwrap();
//! assert_eq!(eth.name, "Ethereum Mainnet");
//! assert_eq!(eth.native_currency.as_ref().unwrap().symbol, "ETH");
//! assert!(eth.has_feature("EIP1559"));
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// A feature supported by a chain (e.g. `EIP155`, `EIP1559`).
#[derive(Debug, Clone, Deserialize)]
pub struct ChainFeature {
    /// The feature name.
    pub name: String,
}

/// A chain's native currency.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainCurrency {
    /// The currency name (e.g. `Ether`).
    pub name: String,
    /// The currency symbol (e.g. `ETH`).
    pub symbol: String,
    /// The number of decimals.
    pub decimals: i64,
}

/// ENS registry information for a chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainEns {
    /// The ENS registry contract address.
    pub registry: String,
}

/// A block explorer for a chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainExplorer {
    /// The explorer name.
    pub name: String,
    /// The explorer base URL.
    pub url: String,
    /// The standard implemented (e.g. `EIP3091`).
    #[serde(default)]
    pub standard: String,
}

/// Metadata for an EVM-compatible chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainInfo {
    /// The chain's human-readable name.
    pub name: String,
    /// The short chain identifier (e.g. `ETH`).
    #[serde(default)]
    pub chain: String,
    /// An icon identifier, if any.
    #[serde(default)]
    pub icon: String,
    /// Known RPC endpoints.
    #[serde(default)]
    pub rpc: Vec<String>,
    /// Supported features.
    #[serde(default)]
    pub features: Vec<ChainFeature>,
    /// Faucet URLs.
    #[serde(default)]
    pub faucets: Vec<String>,
    /// The native currency.
    #[serde(rename = "nativeCurrency", default)]
    pub native_currency: Option<ChainCurrency>,
    /// An informational URL.
    #[serde(rename = "infoURL", default)]
    pub info_url: String,
    /// A short name (e.g. `eth`).
    #[serde(rename = "shortName", default)]
    pub short_name: String,
    /// The numeric chain id.
    #[serde(rename = "chainId")]
    pub chain_id: u64,
    /// The network id.
    #[serde(rename = "networkId", default)]
    pub network_id: u64,
    /// SLIP-44 coin type, if any.
    #[serde(default)]
    pub slip44: Option<i64>,
    /// ENS registry information, if any.
    #[serde(default)]
    pub ens: Option<ChainEns>,
    /// Known block explorers.
    #[serde(default)]
    pub explorers: Vec<ChainExplorer>,
}

impl ChainInfo {
    /// Reports whether the chain supports the named feature.
    pub fn has_feature(&self, feat: &str) -> bool {
        self.features.iter().any(|f| f.name == feat)
    }

    /// Returns a URL to view `tx_hash` on the chain's first explorer, or `None`
    /// if no explorer is configured.
    pub fn transaction_url(&self, tx_hash: &str) -> Option<String> {
        self.explorers
            .first()
            .map(|e| format!("{}/tx/{}", e.url, tx_hash))
    }

    /// Returns the URL of the chain's first block explorer, or `None`.
    pub fn explorer_url(&self) -> Option<&str> {
        self.explorers.first().map(|e| e.url.as_str())
    }
}

/// The embedded chain registry, keyed by chain id as a string.
const CHAINS_JSON: &str = include_str!("chains_data.json");

fn registry() -> &'static HashMap<u64, ChainInfo> {
    static REGISTRY: OnceLock<HashMap<u64, ChainInfo>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let raw: HashMap<String, ChainInfo> =
            serde_json::from_str(CHAINS_JSON).expect("embedded chains_data.json must be valid");
        raw.into_iter()
            .filter_map(|(k, v)| k.parse::<u64>().ok().map(|id| (id, v)))
            .collect()
    })
}

/// Returns the [`ChainInfo`] for the given chain id, or `None` if unknown.
pub fn get(id: u64) -> Option<&'static ChainInfo> {
    registry().get(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ethereum_mainnet() {
        let ci = get(1).expect("chain 1");
        assert_eq!(ci.name, "Ethereum Mainnet");
        assert_eq!(ci.chain_id, 1);
        assert_eq!(ci.native_currency.as_ref().unwrap().symbol, "ETH");
    }

    #[test]
    fn cached_same_pointer() {
        let a = get(1).unwrap();
        let b = get(1).unwrap();
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn unknown_chain() {
        assert!(get(0).is_none());
    }

    #[test]
    fn features() {
        let ci = get(1).unwrap();
        assert!(ci.has_feature("EIP155"));
        assert!(ci.has_feature("EIP1559"));
        assert!(!ci.has_feature("nonexistent"));
    }

    #[test]
    fn transaction_and_explorer_urls() {
        let ci = get(1).unwrap();
        assert_eq!(
            ci.transaction_url("0xabc123").unwrap(),
            "https://etherscan.io/tx/0xabc123"
        );
        assert_eq!(ci.explorer_url().unwrap(), "https://etherscan.io");
    }

    #[test]
    fn no_explorer() {
        let ci = ChainInfo {
            name: "Test".to_string(),
            chain: String::new(),
            icon: String::new(),
            rpc: vec![],
            features: vec![],
            faucets: vec![],
            native_currency: None,
            info_url: String::new(),
            short_name: String::new(),
            chain_id: 0,
            network_id: 0,
            slip44: None,
            ens: None,
            explorers: vec![],
        };
        assert!(ci.transaction_url("0xabc").is_none());
        assert!(ci.explorer_url().is_none());
    }
}
