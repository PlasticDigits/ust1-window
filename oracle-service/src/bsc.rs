//! Read Venus vToken `exchangeRateStored` on BSC via Alloy.

use alloy::eips::{BlockId, BlockNumberOrTag};
use alloy::network::Ethereum;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol;
use alloy::transports::Transport;
use eyre::{eyre, Result};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};

sol! {
    #[sol(rpc)]
    contract VToken {
        function exchangeRateStored() external view returns (uint256);
    }
}

static CHAIN_ID_CACHE: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
pub async fn verify_all_bsc_rpc_urls(urls: &[String], allowed_chain_ids: &[u64]) -> Result<()> {
    for url in urls {
        let provider = ProviderBuilder::new().on_http(url.parse()?);
        verify_bsc_rpc_chain_id(url, &provider, allowed_chain_ids).await?;
    }
    Ok(())
}

pub async fn read_exchange_rate_stored(
    rpc_url: String,
    vtoken: &str,
    confirmation_blocks: u64,
    allowed_chain_ids: &[u64],
) -> Result<cosmwasm_std::Uint128> {
    let provider = ProviderBuilder::new().on_http(rpc_url.parse()?);
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
    let c = VToken::new(addr, provider);
    let r = c
        .exchangeRateStored()
        .block(block_id)
        .call()
        .await
        .map_err(|e| eyre!("vToken call: {}", e))?;
    let v = r._0;
    let s = v.to_string();
    cosmwasm_std::Uint128::from_str(&s).map_err(|_| eyre!("rate does not fit Uint128: {}", s))
}
