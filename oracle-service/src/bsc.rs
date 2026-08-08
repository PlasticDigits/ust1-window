//! Read Venus vToken `exchangeRateStored` on BSC via Alloy.

use alloy::eips::{BlockId, BlockNumberOrTag};
use alloy::network::Ethereum;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::client::ClientBuilder;
use alloy::sol;
use alloy::transports::http::Http;
use alloy::transports::Transport;
use eyre::{eyre, Result};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

sol! {
    #[sol(rpc)]
    contract VToken {
        function exchangeRateStored() external view returns (uint256);
        function underlying() external view returns (address);
        function decimals() external view returns (uint8);
    }

    #[sol(rpc)]
    contract Erc20Decimals {
        function decimals() external view returns (uint8);
    }
}

/// Convert Venus `exchangeRateStored` mantissa → on-chain oracle `R` (`RATE_SCALE` = 1e18).
///
/// Venus: `oneVTokenInUnderlying = exchangeRate / 10^(18 + underlyingDecimals - vTokenDecimals)`.
/// On-chain: `R / 1e18 = FDUSD per 1 vFDUSD` ⇒ `R = exchangeRate / 10^(underlyingDecimals - vTokenDecimals)`.
pub fn venus_exchange_rate_to_oracle_r(
    exchange_rate_stored: alloy::primitives::U256,
    underlying_decimals: u8,
    vtoken_decimals: u8,
) -> Result<cosmwasm_std::Uint128> {
    if underlying_decimals < vtoken_decimals {
        return Err(eyre!(
            "underlying decimals ({underlying_decimals}) < vToken decimals ({vtoken_decimals})"
        ));
    }
    let shift = u32::from(underlying_decimals - vtoken_decimals);
    let divisor = alloy::primitives::U256::from(10u64).pow(alloy::primitives::U256::from(shift));
    if divisor.is_zero() {
        return Err(eyre!("internal: zero divisor normalizing Venus rate"));
    }
    let r = exchange_rate_stored / divisor;
    let s = r.to_string();
    cosmwasm_std::Uint128::from_str(&s).map_err(|_| eyre!("normalized rate does not fit Uint128: {s}"))
}

static CHAIN_ID_CACHE: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Build an HTTP JSON-RPC provider with an explicit reqwest transport timeout.
pub(crate) fn build_http_provider(
    rpc_url: &str,
    timeout_secs: u64,
) -> Result<impl Provider<Http<reqwest::Client>, Ethereum>> {
    let url = rpc_url
        .parse()
        .map_err(|e| eyre!("invalid BSC RPC URL {rpc_url}: {e}"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| eyre!("reqwest client: {e}"))?;
    let transport = Http::with_client(client, url);
    let is_local = transport.guess_local();
    let rpc_client = ClientBuilder::default().transport(transport, is_local);
    Ok(ProviderBuilder::new().on_client(rpc_client))
}

/// Verify `eth_chainId` for this RPC URL against `allowed_chain_ids`, caching the result per URL.
pub async fn verify_bsc_rpc_chain_id<T>(
    rpc_url: &str,
    provider: &impl Provider<T, Ethereum>,
    allowed_chain_ids: &[u64],
) -> Result<()>
where
    T: Transport + Clone,
{
    {
        let cache = CHAIN_ID_CACHE
            .lock()
            .map_err(|e| eyre!("chain id cache lock: {}", e))?;
        if let Some(&id) = cache.get(rpc_url) {
            if allowed_chain_ids.contains(&id) {
                return Ok(());
            }
            return Err(eyre!(
                "BSC RPC {} previously reported chainId {}, which is not allowed (allowed: {:?})",
                rpc_url,
                id,
                allowed_chain_ids
            ));
        }
    }

    let chain_id = provider
        .get_chain_id()
        .await
        .map_err(|e| eyre!("eth_chainId: {}", e))?;

    if !allowed_chain_ids.contains(&chain_id) {
        return Err(eyre!(
            "BSC RPC returned chainId {}, expected one of {:?} (set BSC_ALLOWED_CHAIN_IDS for testnet or Anvil; default is 56 mainnet only)",
            chain_id,
            allowed_chain_ids
        ));
    }

    {
        let mut cache = CHAIN_ID_CACHE
            .lock()
            .map_err(|e| eyre!("chain id cache lock: {}", e))?;
        cache.insert(rpc_url.to_string(), chain_id);
    }
    Ok(())
}

/// Fail fast at startup: every configured RPC URL must match the allowed chain list.
pub async fn verify_all_bsc_rpc_urls(
    urls: &[String],
    allowed_chain_ids: &[u64],
    rpc_timeout_secs: u64,
) -> Result<()> {
    for url in urls {
        let provider = build_http_provider(url, rpc_timeout_secs)?;
        verify_bsc_rpc_chain_id(url, &provider, allowed_chain_ids).await?;
    }
    Ok(())
}

pub async fn read_exchange_rate_stored(
    rpc_url: String,
    vtoken: &str,
    confirmation_blocks: u64,
    allowed_chain_ids: &[u64],
    rpc_timeout_secs: u64,
) -> Result<cosmwasm_std::Uint128> {
    let provider = build_http_provider(&rpc_url, rpc_timeout_secs)?;
    verify_bsc_rpc_chain_id(&rpc_url, &provider, allowed_chain_ids).await?;

    let latest = provider
        .get_block_number()
        .await
        .map_err(|e| eyre!("eth_blockNumber: {}", e))?;
    let at_block = latest.saturating_sub(confirmation_blocks);
    let block_id = BlockId::Number(BlockNumberOrTag::Number(at_block));
    tracing::debug!(
        latest_block = latest,
        at_block,
        confirmation_blocks,
        "BSC vToken read block"
    );

    let addr = Address::from_str(vtoken).map_err(|e| eyre!("invalid vToken address: {}", e))?;
    let c = VToken::new(addr, &provider);
    let er = c
        .exchangeRateStored()
        .block(block_id)
        .call()
        .await
        .map_err(|e| eyre!("vToken exchangeRateStored: {}", e))?;
    let underlying = c
        .underlying()
        .block(block_id)
        .call()
        .await
        .map_err(|e| eyre!("vToken underlying: {}", e))?;
    let v_dec = c
        .decimals()
        .block(block_id)
        .call()
        .await
        .map_err(|e| eyre!("vToken decimals: {}", e))?;
    let u = Erc20Decimals::new(underlying._0, &provider);
    let u_dec = u
        .decimals()
        .block(block_id)
        .call()
        .await
        .map_err(|e| eyre!("underlying decimals: {}", e))?;
    let rate = venus_exchange_rate_to_oracle_r(er._0, u_dec._0, v_dec._0)?;
    tracing::info!(
        exchange_rate_stored = %er._0,
        underlying_decimals = u_dec._0,
        vtoken_decimals = v_dec._0,
        oracle_r = %rate,
        "normalized Venus exchangeRateStored → oracle RATE_SCALE units"
    );
    Ok(rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::U256;
    use serde_json::json;
    use std::time::Instant;
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const VTOKEN: &str = "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba";

    #[test]
    fn normalizes_live_vfdusd_mantissa_to_rate_scale() {
        // exchangeRateStored snapshot ≈ 1.225 FDUSD per vFDUSD (underlying 18, vToken 8)
        let er = U256::from_str("12251045160220566270827151269").unwrap();
        let r = venus_exchange_rate_to_oracle_r(er, 18, 8).unwrap();
        assert_eq!(r.u128(), 1_225_104_516_022_056_627);
    }

    #[test]
    fn rejects_underlying_decimals_lt_vtoken() {
        let err = venus_exchange_rate_to_oracle_r(U256::from(1u64), 6, 8).unwrap_err();
        assert!(err.to_string().contains("underlying decimals"));
    }

    /// **INV-ORACLE-TICK-001** / M-19 ([GitLab #28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28)):
    /// BSC JSON-RPC must fail within the configured transport timeout, not hang the tick.
    #[tokio::test]
    async fn read_exchange_rate_stored_times_out_on_hanging_rpc() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(5))
                    .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":"0x1"})),
            )
            .mount(&server)
            .await;

        let start = Instant::now();
        let err = read_exchange_rate_stored(server.uri(), VTOKEN, 0, &[31337], 1)
            .await
            .unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(3),
            "expected fail-fast within ~1s transport timeout, took {:?}",
            elapsed
        );
        let msg = err.to_string();
        assert!(
            msg.contains("eth_chainId") || msg.contains("timeout") || msg.contains("timed out"),
            "expected transport/chainId error, got: {msg}"
        );
    }

    /// Chain-id and block reads succeed; `eth_call` hangs — still bounded by `rpc_timeout_secs`.
    #[tokio::test]
    async fn read_exchange_rate_stored_times_out_when_eth_call_hangs() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_chainId"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x7a69"
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_blockNumber"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x64"
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_call"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(5))
                    .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":"0x0"})),
            )
            .mount(&server)
            .await;

        let start = Instant::now();
        let err = read_exchange_rate_stored(server.uri(), VTOKEN, 0, &[31337], 1)
            .await
            .unwrap_err();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "eth_call hang should be bounded by transport timeout"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("vToken call") || msg.contains("timeout") || msg.contains("timed out"),
            "expected eth_call/timeout error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_all_bsc_rpc_urls_fails_fast_on_hanging_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;

        let start = Instant::now();
        let err = verify_all_bsc_rpc_urls(&[server.uri()], &[31337], 1)
            .await
            .unwrap_err();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "startup chain-id probe must not block on hanging RPC"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("eth_chainId") || msg.contains("timeout") || msg.contains("timed out"),
            "expected chainId/timeout error, got: {msg}"
        );
    }
}
