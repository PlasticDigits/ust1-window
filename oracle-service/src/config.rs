//! Environment configuration for `ust1-oracle-service`.

use eyre::{eyre, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub bsc_rpc_urls: Vec<String>,
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
        if bsc_rpc_urls.is_empty() {
            return Err(eyre!("BSC_RPC_URLS must list at least one URL"));
        }
        Ok(Config {
            bsc_rpc_urls,
            venus_vtoken_address: std::env::var("VENUS_VTOKEN_ADDRESS")
                .map_err(|_| eyre!("VENUS_VTOKEN_ADDRESS is required"))?,
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
