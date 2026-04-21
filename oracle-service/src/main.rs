//! Poll BSC Venus vToken rate and update `ust1-oracle` on Terra Classic when policy allows.

mod bsc;
mod config;
mod evm_rpc;
mod terra_tx;

use cosmwasm_std::Uint128;
use eyre::Result;
use tracing::{info, warn};
use ust1_common::oracle_policy::check_rate_update;
use ust1_oracle::msg::{ExecuteMsg, QueryMsg, StateResponse};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ust1_oracle_service=info".parse()?),
        )
        .init();

    let cfg = config::Config::from_env()?;
    let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
        lcd_url: cfg.terra_lcd_url.clone(),
        chain_id: cfg.terra_chain_id.clone(),
        mnemonic: cfg.terra_mnemonic.clone(),
        gas_limit: None,
    })?;

    info!(address = %signer.address_str(), "oracle operator");

    loop {
        if let Err(e) = run_once(&cfg, &signer).await {
            warn!(error = %e, "tick failed");
        }
        tokio::time::sleep(std::time::Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

async fn run_once(cfg: &config::Config, signer: &terra_tx::TerraSigner) -> Result<()> {
    let urls = &cfg.bsc_rpc_urls;
    let proposed: Uint128 = evm_rpc::run_with_evm_rpc_url_fallback(urls, |url| {
        let v = cfg.venus_vtoken_address.clone();
        let confirm = cfg.bsc_confirmation_blocks;
        async move { bsc::read_exchange_rate_stored(url, &v, confirm).await }
    })
    .await?;

    let state: StateResponse = signer
        .query_wasm_smart(&cfg.oracle_contract, &QueryMsg::State {})
        .await?;

    if proposed == state.rate {
        info!("no rate change from BSC");
        return Ok(());
    }

    let check = check_rate_update(
        now_unix(),
        state.last_update_sec,
        state.rate,
        proposed,
        state.utc_day_id,
        state.day_baseline_rate,
    );

    match check {
        Ok(_) => {
            let msg = ExecuteMsg::UpdateRate { new_rate: proposed };
            let txh = signer
                .sign_and_broadcast_execute(&cfg.oracle_contract, &msg)
                .await?;
            info!(tx_hash = %txh, new_rate = %proposed, "submitted oracle update");
        }
        Err(e) => {
            info!(?e, proposed = %proposed, "skip update (policy or no change)");
        }
    }
    Ok(())
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
