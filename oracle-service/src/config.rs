//! Environment configuration for `ust1-oracle-service`.

use alloy::primitives::Address;
use eyre::{eyre, Result};
use std::str::FromStr;

/// Canonical Venus vFDUSD vToken on BSC mainnet (EVM-03 / glab #9).
const CANONICAL_VENUS_VFDUSD_BSC_MAINNET: &str = "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba";

fn validate_venus_vtoken_address(addr: &str) -> Result<()> {
    let got = Address::from_str(addr.trim())
        .map_err(|e| eyre!("VENUS_VTOKEN_ADDRESS is not a valid EVM address: {}", e))?;
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

#[derive(Debug, Clone)]
pub struct Config {
    pub bsc_rpc_urls: Vec<String>,
    /// How many blocks behind `latest` to read `exchangeRateStored` (reorg protection).
    pub bsc_confirmation_blocks: u64,
    pub venus_vtoken_address: String,
    pub terra_lcd_url: String,
    pub terra_chain_id: String,
    pub terra_mnemonic: String,
    pub oracle_contract: String,
    pub poll_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        let bsc_raw = std::env::var("BSC_RPC_URLS")
            .map_err(|_| eyre!("BSC_RPC_URLS is required (comma-separated)"))?;
        let bsc_rpc_urls = crate::evm_rpc::parse_comma_separated_rpc_urls(&bsc_raw);
        if bsc_rpc_urls.len() < 2 {
            return Err(eyre!(
                "BSC_RPC_URLS must list at least two comma-separated URLs (multi-provider consensus)"
            ));
        }
        let venus_vtoken_address = std::env::var("VENUS_VTOKEN_ADDRESS")
            .map_err(|_| eyre!("VENUS_VTOKEN_ADDRESS is required"))?;
        validate_venus_vtoken_address(&venus_vtoken_address)?;
        Ok(Config {
            bsc_rpc_urls,
            bsc_confirmation_blocks: std::env::var("BSC_CONFIRMATION_BLOCKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            venus_vtoken_address,
            terra_lcd_url: std::env::var("TERRA_LCD_URL")
                .map_err(|_| eyre!("TERRA_LCD_URL is required"))?,
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_canonical_checksummed() {
        validate_venus_vtoken_address(CANONICAL_VENUS_VFDUSD_BSC_MAINNET).unwrap();
    }

    #[test]
    fn validate_accepts_canonical_lowercase() {
        validate_venus_vtoken_address("0xc4ef4229fec74ccfe17b2bdef7715fac740ba0ba").unwrap();
    }

    #[test]
    fn validate_rejects_wrong_token() {
        let err = validate_venus_vtoken_address("0x2170Ed0880ac9A755fd29B2688956BD959F933F8")
            .unwrap_err();
        assert!(
            err.to_string().contains("canonical Venus vFDUSD"),
            "{}",
            err
        );
    }
}
