//! Environment configuration for `ust1-oracle-service`.

use alloy::primitives::Address;
use eyre::{eyre, Result};
use std::str::FromStr;
use url::Url;

/// Canonical Venus vFDUSD vToken on BSC mainnet (EVM-03 / glab #9).
const CANONICAL_VENUS_VFDUSD_BSC_MAINNET: &str = "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba";

/// When the only allowed EVM chain is BSC mainnet (56), require the canonical Venus vFDUSD vToken.
/// For BSC testnet (97), Anvil (31337), etc., any valid contract address is accepted so deployments can use mocks.
fn validate_venus_vtoken_address(addr: &str, allowed_bsc_chain_ids: &[u64]) -> Result<()> {
    let got = Address::from_str(addr.trim())
        .map_err(|e| eyre!("VENUS_VTOKEN_ADDRESS is not a valid EVM address: {}", e))?;
    let mainnet_only =
        allowed_bsc_chain_ids.len() == 1 && allowed_bsc_chain_ids.first().copied() == Some(56);
    if !mainnet_only {
        return Ok(());
    }
    let want = Address::from_str(CANONICAL_VENUS_VFDUSD_BSC_MAINNET)
        .map_err(|_| eyre!("internal error: canonical Venus vFDUSD address failed to parse"))?;
    if got != want {
        return Err(eyre!(
            "VENUS_VTOKEN_ADDRESS must be canonical Venus vFDUSD on BSC mainnet ({}); got {}",
            CANONICAL_VENUS_VFDUSD_BSC_MAINNET,
            addr.trim()
        ));
    }
    Ok(())
}

fn parse_allowed_bsc_chain_ids() -> Result<Vec<u64>> {
    let raw = std::env::var("BSC_ALLOWED_CHAIN_IDS").unwrap_or_else(|_| "56".to_string());
    let ids: Vec<u64> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| eyre!("invalid chain id in BSC_ALLOWED_CHAIN_IDS: {:?}", s))
        })
        .collect::<Result<_, _>>()?;
    if ids.is_empty() {
        return Err(eyre!(
            "BSC_ALLOWED_CHAIN_IDS must list at least one chain id"
        ));
    }
    Ok(ids)
}

fn dev_allow_http() -> bool {
    matches!(
        std::env::var("DEV_ALLOW_HTTP").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Rejects non-HTTPS RPC/LCD URLs unless `DEV_ALLOW_HTTP=1` and the host is loopback/local.
fn validate_https_url(url_str: &str, field: &str, allow_dev_http: bool) -> Result<()> {
    let url = Url::parse(url_str)
        .map_err(|e| eyre!("{} is not a valid URL ({}): {}", field, url_str, e))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_dev_http => match url.host_str() {
            Some("localhost") | Some("127.0.0.1") | Some("::1") => Ok(()),
            _ => Err(eyre!(
                "{} with http:// is only allowed for localhost / 127.0.0.1 / ::1 when DEV_ALLOW_HTTP is set: {}",
                field,
                url_str
            )),
        },
        _ => Err(eyre!("{} must use https:// (got scheme {:?}): {}", field, url.scheme(), url_str)),
    }
}

#[derive(Clone)]
pub struct Config {
    pub bsc_rpc_urls: Vec<String>,
    /// How many blocks behind `latest` to read `exchangeRateStored` (reorg protection).
    pub bsc_confirmation_blocks: u64,
    /// Allowed `eth_chainId` values for every BSC RPC URL (default `56`).
    /// Use `56,97,31337` to allow BSC mainnet, BSC testnet (Chapel), and Anvil localnet.
    pub allowed_bsc_chain_ids: Vec<u64>,
    pub venus_vtoken_address: String,
    pub terra_lcd_url: String,
    pub terra_chain_id: String,
    pub terra_mnemonic: String,
    pub oracle_contract: String,
    pub poll_interval_secs: u64,
    /// Emit a loud log if no successful Terra broadcast for this many seconds (default 8h).
    pub max_silence_since_broadcast_secs: u64,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bsc_rpc_urls", &self.bsc_rpc_urls)
            .field("bsc_confirmation_blocks", &self.bsc_confirmation_blocks)
            .field("allowed_bsc_chain_ids", &self.allowed_bsc_chain_ids)
            .field("venus_vtoken_address", &self.venus_vtoken_address)
            .field("terra_lcd_url", &self.terra_lcd_url)
            .field("terra_chain_id", &self.terra_chain_id)
            .field("terra_mnemonic", &"<redacted>")
            .field("oracle_contract", &self.oracle_contract)
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field(
                "max_silence_since_broadcast_secs",
                &self.max_silence_since_broadcast_secs,
            )
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        let allow_dev_http = dev_allow_http();
        let bsc_raw = std::env::var("BSC_RPC_URLS")
            .map_err(|_| eyre!("BSC_RPC_URLS is required (comma-separated)"))?;
        let bsc_rpc_urls = crate::evm_rpc::parse_comma_separated_rpc_urls(&bsc_raw);
        if bsc_rpc_urls.len() < 2 {
            return Err(eyre!(
                "BSC_RPC_URLS must list at least two comma-separated URLs (multi-provider consensus)"
            ));
        }
        for url in &bsc_rpc_urls {
            validate_https_url(url, "BSC_RPC_URLS entry", allow_dev_http)?;
        }
        let allowed_bsc_chain_ids = parse_allowed_bsc_chain_ids()?;
        let venus_vtoken_address = std::env::var("VENUS_VTOKEN_ADDRESS")
            .map_err(|_| eyre!("VENUS_VTOKEN_ADDRESS is required"))?;
        validate_venus_vtoken_address(&venus_vtoken_address, &allowed_bsc_chain_ids)?;
        let terra_lcd_url = std::env::var("TERRA_LCD_URL").map_err(|_| eyre!("TERRA_LCD_URL is required"))?;
        validate_https_url(&terra_lcd_url, "TERRA_LCD_URL", allow_dev_http)?;
        Ok(Config {
            bsc_rpc_urls,
            bsc_confirmation_blocks: std::env::var("BSC_CONFIRMATION_BLOCKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            allowed_bsc_chain_ids,
            venus_vtoken_address,
            terra_lcd_url,
            terra_chain_id: std::env::var("TERRA_CHAIN_ID")
                .unwrap_or_else(|_| "columbus-5".to_string()),
            terra_mnemonic: std::env::var("TERRA_MNEMONIC")
                .map_err(|_| eyre!("TERRA_MNEMONIC is required"))?,
            oracle_contract: std::env::var("ORACLE_CONTRACT")
                .map_err(|_| eyre!("ORACLE_CONTRACT is required"))?,
            poll_interval_secs: std::env::var("POLL_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(21_600),
            max_silence_since_broadcast_secs: std::env::var("ORACLE_MAX_SILENCE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(28_800),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_canonical_checksummed() {
        validate_venus_vtoken_address(CANONICAL_VENUS_VFDUSD_BSC_MAINNET, &[56]).unwrap();
    }

    #[test]
    fn validate_accepts_canonical_lowercase() {
        validate_venus_vtoken_address("0xc4ef4229fec74ccfe17b2bdef7715fac740ba0ba", &[56]).unwrap();
    }

    #[test]
    fn validate_accepts_non_canonical_on_testnet_allowlist() {
        validate_venus_vtoken_address("0x2170Ed0880ac9A755fd29B2688956BD959F933F8", &[97]).unwrap();
    }

    #[test]
    fn validate_rejects_wrong_token() {
        let err =
            validate_venus_vtoken_address("0x2170Ed0880ac9A755fd29B2688956BD959F933F8", &[56])
                .unwrap_err();
        assert!(
            err.to_string().contains("canonical Venus vFDUSD"),
            "{}",
            err
        );
    }

    #[test]
    fn config_debug_redacts_mnemonic() {
        let cfg = Config {
            bsc_rpc_urls: vec!["http://example".into()],
            bsc_confirmation_blocks: 15,
            allowed_bsc_chain_ids: vec![56],
            venus_vtoken_address: CANONICAL_VENUS_VFDUSD_BSC_MAINNET.into(),
            terra_lcd_url: String::new(),
            terra_chain_id: String::new(),
            terra_mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".into(),
            oracle_contract: String::new(),
            poll_interval_secs: 0,
            max_silence_since_broadcast_secs: 28_800,
        };
        let s = format!("{cfg:?}");
        assert!(
            !s.contains("abandon"),
            "Debug must not echo mnemonic words: {s}"
        );
        assert!(s.contains("redacted"), "{s}");
    }
}
