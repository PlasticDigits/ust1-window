//! Read Venus vToken `exchangeRateStored` on BSC via Alloy.

use alloy::primitives::Address;
use alloy::sol;
use eyre::{eyre, Result};
use std::str::FromStr;

sol! {
    #[sol(rpc)]
    contract VToken {
        function exchangeRateStored() external view returns (uint256);
    }
}

pub async fn read_exchange_rate_stored(
    rpc_url: String,
    vtoken: &str,
) -> Result<cosmwasm_std::Uint128> {
    let provider = alloy::providers::ProviderBuilder::new().on_http(rpc_url.parse()?);
    let addr = Address::from_str(vtoken).map_err(|e| eyre!("invalid vToken address: {}", e))?;
    let c = VToken::new(addr, provider);
    let r = c
        .exchangeRateStored()
        .call()
        .await
        .map_err(|e| eyre!("vToken call: {}", e))?;
    let v = r._0;
    let s = v.to_string();
    cosmwasm_std::Uint128::from_str(&s).map_err(|_| eyre!("rate does not fit Uint128: {}", s))
}
