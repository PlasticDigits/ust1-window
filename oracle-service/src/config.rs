//! Environment configuration for `ust1-oracle-service`.
//!
//! Deployment checklist and env table: `docs/DEPLOYMENT.md` (repo root). Invariants mirrored
//! off-chain: **INV-ORACLE-THROTTLE-001**, **INV-ORACLE-DAILY-001**, **INV-ORACLE-MONO-001** in
//! `ust1-common::oracle_policy`.
//!
//! Ops timing (H-3 / [glab #24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24)):
//! - **INV-ORACLE-OPS-POLL-001** — default `POLL_INTERVAL_SECS` ≪ window
//!   [`ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS`] so one missed tick cannot exhaust staleness.
//! - **INV-ORACLE-OPS-SILENCE-001** — default `ORACLE_MAX_SILENCE_SECS` ≤ window max age so
//!   liveness pages at or before swap halt.
//!
//! Liveness semantics (C-3 / [glab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)):
//! - **INV-ORACLE-LIVENESS-001** — silence keys off **confirmed** on-chain updates (DeliverTx +
//!   matching oracle `State`), not CheckTx-only. See `confirm` / `liveness`.
//!
//! Agent skills: `skills/oracle-ops-poll-silence/SKILL.md`, `skills/oracle-liveness-confirm/SKILL.md`.

use alloy::primitives::Address;
use eyre::{eyre, Result};
use secrecy::SecretString;
use std::str::FromStr;
use url::Url;
use ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS;

/// Canonical Venus vFDUSD vToken on BSC mainnet (EVM-03 / glab #9).
const CANONICAL_VENUS_VFDUSD_BSC_MAINNET: &str = "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba";

/// Default off-chain poll interval (1 hour).
///
/// **INV-ORACLE-OPS-POLL-001:** must stay **strictly below**
/// [`DEFAULT_MAX_ORACLE_AGE_SECS`] (6h). On-chain throttle
/// ([`ust1_common::MIN_ORACLE_UPDATE_INTERVAL_SECS`], 4h) still makes most ticks no-ops when
/// within band — lowering the poll default does not imply a tx every hour.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 3_600;

/// Default liveness silence threshold (equal to window max oracle age = 6h).
///
/// **INV-ORACLE-OPS-SILENCE-001:** must not exceed
/// [`DEFAULT_MAX_ORACLE_AGE_SECS`] (+ small grace). Prefer ≤ max age so ops are paged at or
/// before users are bricked. **INV-ORACLE-LIVENESS-001:** "successful" means DeliverTx +
/// matching oracle `State`, not CheckTx / `BROADCAST_MODE_SYNC` alone.
pub const DEFAULT_ORACLE_MAX_SILENCE_SECS: u64 = DEFAULT_MAX_ORACLE_AGE_SECS;

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

/// Resolve `POLL_INTERVAL_SECS` from an optional env string (invalid/missing → default).
pub(crate) fn resolve_poll_interval_secs(env_val: Option<&str>) -> u64 {
    env_val
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
}

/// Resolve `ORACLE_MAX_SILENCE_SECS` from an optional env string (invalid/missing → default).
pub(crate) fn resolve_max_silence_secs(env_val: Option<&str>) -> u64 {
    env_val
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ORACLE_MAX_SILENCE_SECS)
}

/// Operator advisories for poll/silence vs the **default** window max oracle age.
///
/// The service does not read on-chain `max_oracle_age_sec`; if governance raised that knob,
/// operators must keep `POLL_INTERVAL_SECS` ≪ and `ORACLE_MAX_SILENCE_SECS` ≤ the live value
/// (see `docs/DEPLOYMENT.md`). Warnings never fail config load.
pub fn ops_timing_warnings(poll_interval_secs: u64, max_silence_secs: u64) -> Vec<String> {
    let max_age = DEFAULT_MAX_ORACLE_AGE_SECS;
    let mut warnings = Vec::new();

    if poll_interval_secs == 0 {
        warnings.push(
            "POLL_INTERVAL_SECS=0 tight-loops LCD/RPC; use a positive interval (default 3600)"
                .to_string(),
        );
    } else if poll_interval_secs >= max_age {
        warnings.push(format!(
            "POLL_INTERVAL_SECS ({poll_interval_secs}) >= DEFAULT_MAX_ORACLE_AGE_SECS ({max_age}): \
             one missed tick can halt window swaps (INV-ORACLE-OPS-POLL-001)"
        ));
    }

    if max_silence_secs > max_age {
        warnings.push(format!(
            "ORACLE_MAX_SILENCE_SECS ({max_silence_secs}) > DEFAULT_MAX_ORACLE_AGE_SECS ({max_age}): \
             liveness alert fires after the window already rejects swaps (INV-ORACLE-OPS-SILENCE-001)"
        ));
    }

    // Footgun: silence far looser than poll cadence (e.g. poll=6h, silence=24h).
    if poll_interval_secs > 0 && max_silence_secs > poll_interval_secs.saturating_mul(8) {
        warnings.push(format!(
            "ORACLE_MAX_SILENCE_SECS ({max_silence_secs}) > 8× POLL_INTERVAL_SECS ({poll_interval_secs}): \
             silence threshold is unusually loose relative to poll cadence"
        ));
    }

    warnings
}

#[derive(Debug, Clone)]
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
    /// Operator seed phrase; only call `ExposeSecret::expose_secret` at signing boundaries.
    pub terra_mnemonic: SecretString,
    pub oracle_contract: String,
    pub poll_interval_secs: u64,
    /// Emit a loud log if no confirmed on-chain oracle update for this many seconds
    /// (default [`DEFAULT_ORACLE_MAX_SILENCE_SECS`] = 6h).
    ///
    /// “Successful” means DeliverTx + matching oracle `State` (INV-ORACLE-LIVENESS-001), not
    /// CheckTx / `BROADCAST_MODE_SYNC` alone.
    pub max_silence_since_broadcast_secs: u64,
    /// Max time to wait for DeliverTx inclusion after SYNC broadcast (default 90s).
    pub tx_confirm_timeout_secs: u64,
    /// Poll interval while waiting for tx inclusion (default 2000 ms).
    pub tx_confirm_poll_interval_ms: u64,
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
        let terra_lcd_url =
            std::env::var("TERRA_LCD_URL").map_err(|_| eyre!("TERRA_LCD_URL is required"))?;
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
            terra_mnemonic: SecretString::new(
                std::env::var("TERRA_MNEMONIC")
                    .map_err(|_| eyre!("TERRA_MNEMONIC is required"))?
                    .into_boxed_str(),
            ),
            oracle_contract: std::env::var("ORACLE_CONTRACT")
                .map_err(|_| eyre!("ORACLE_CONTRACT is required"))?,
            poll_interval_secs: resolve_poll_interval_secs(
                std::env::var("POLL_INTERVAL_SECS").ok().as_deref(),
            ),
            max_silence_since_broadcast_secs: resolve_max_silence_secs(
                std::env::var("ORACLE_MAX_SILENCE_SECS").ok().as_deref(),
            ),
            tx_confirm_timeout_secs: std::env::var("ORACLE_TX_CONFIRM_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(90),
            tx_confirm_poll_interval_ms: std::env::var("ORACLE_TX_CONFIRM_POLL_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2_000),
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
    #[allow(clippy::assertions_on_constants)]
    fn default_poll_and_silence_aligned_with_window_budget() {
        // H-3 / #24 acceptance: poll ≤ 1h and ≪ max age; silence ≤ max age (not 8h).
        assert!(DEFAULT_POLL_INTERVAL_SECS <= 3_600);
        assert!(DEFAULT_POLL_INTERVAL_SECS < DEFAULT_MAX_ORACLE_AGE_SECS);
        assert_eq!(DEFAULT_ORACLE_MAX_SILENCE_SECS, DEFAULT_MAX_ORACLE_AGE_SECS);
        assert!(DEFAULT_ORACLE_MAX_SILENCE_SECS <= DEFAULT_MAX_ORACLE_AGE_SECS);
        assert!(DEFAULT_ORACLE_MAX_SILENCE_SECS < 28_800);
    }

    #[test]
    fn resolve_poll_defaults_and_overrides() {
        assert_eq!(resolve_poll_interval_secs(None), DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(resolve_poll_interval_secs(Some("")), DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(resolve_poll_interval_secs(Some("not-a-number")), DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(resolve_poll_interval_secs(Some("1800")), 1_800);
    }

    #[test]
    fn resolve_silence_defaults_and_overrides() {
        assert_eq!(
            resolve_max_silence_secs(None),
            DEFAULT_ORACLE_MAX_SILENCE_SECS
        );
        assert_eq!(resolve_max_silence_secs(Some("7200")), 7_200);
        assert_eq!(
            resolve_max_silence_secs(Some("bogus")),
            DEFAULT_ORACLE_MAX_SILENCE_SECS
        );
    }

    #[test]
    fn ops_timing_warnings_clean_for_defaults() {
        assert!(
            ops_timing_warnings(DEFAULT_POLL_INTERVAL_SECS, DEFAULT_ORACLE_MAX_SILENCE_SECS)
                .is_empty()
        );
    }

    #[test]
    fn ops_timing_warnings_flag_legacy_misconfig() {
        // Historic footgun: poll == max age, silence 8h.
        let w = ops_timing_warnings(21_600, 28_800);
        assert!(
            w.iter().any(|s| s.contains("INV-ORACLE-OPS-POLL-001")),
            "{w:?}"
        );
        assert!(
            w.iter().any(|s| s.contains("INV-ORACLE-OPS-SILENCE-001")),
            "{w:?}"
        );
    }

    #[test]
    fn ops_timing_warnings_flag_zero_poll() {
        let w = ops_timing_warnings(0, DEFAULT_ORACLE_MAX_SILENCE_SECS);
        assert!(w.iter().any(|s| s.contains("tight-loops")), "{w:?}");
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
            terra_mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            oracle_contract: String::new(),
            poll_interval_secs: DEFAULT_POLL_INTERVAL_SECS,
            max_silence_since_broadcast_secs: DEFAULT_ORACLE_MAX_SILENCE_SECS,
            tx_confirm_timeout_secs: 90,
            tx_confirm_poll_interval_ms: 2_000,
        };
        let s = format!("{cfg:?}");
        assert!(
            !s.contains("abandon"),
            "Debug must not echo mnemonic words: {s}"
        );
        assert!(
            s.contains("REDACTED") || s.contains("SecretBox") || s.contains("SecretString"),
            "expected secrecy-style redacted Debug, got: {s}"
        );
    }
}
