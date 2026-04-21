//! Poll BSC Venus vToken rate and update `ust1-oracle` on Terra Classic when policy allows.

mod bsc;
mod config;
mod evm_rpc;
mod liveness;
mod terra_tx;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cosmwasm_std::Uint128;
use eyre::Result;
use tracing::{error, info, warn};
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
    info!(allowed_chain_ids = ?cfg.allowed_bsc_chain_ids, "BSC EVM chain allowlist");
    bsc::verify_all_bsc_rpc_urls(&cfg.bsc_rpc_urls, &cfg.allowed_bsc_chain_ids).await?;
    let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
        lcd_url: cfg.terra_lcd_url.clone(),
        chain_id: cfg.terra_chain_id.clone(),
        mnemonic: cfg.terra_mnemonic.clone(),
        gas_limit: None,
    })?;

    let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));
    let max_silence = Duration::from_secs(cfg.max_silence_since_broadcast_secs);

    info!(
        address = %signer.address_str(),
        poll_interval_secs = cfg.poll_interval_secs,
        max_silence_since_broadcast_secs = cfg.max_silence_since_broadcast_secs,
        "oracle operator"
    );

    loop {
        {
            let tracker = liveness.lock().expect("liveness mutex poisoned");
            if tracker.should_alert(max_silence) {
                let silence_secs = tracker.silence_since_last_broadcast().as_secs();
                error!(
                    target: "ust1_oracle_service",
                    alert = "LIVENESS_ORACLE_NO_BROADCAST",
                    silence_secs,
                    threshold_secs = cfg.max_silence_since_broadcast_secs,
                    "LIVENESS ALERT: no successful Terra oracle broadcast within the configured silence window; \
                     on-chain rate may be stale — investigate LCD, BSC RPC, keys, and policy"
                );
            }
        }
        if let Err(e) = run_once(&cfg, &signer, &liveness).await {
            warn!(error = %e, "tick failed");
        }
        tokio::time::sleep(std::time::Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

async fn run_once(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    liveness: &Arc<Mutex<liveness::LivenessTracker>>,
) -> Result<()> {
    let urls = &cfg.bsc_rpc_urls;
    let allowed = cfg.allowed_bsc_chain_ids.clone();
    let proposed: Uint128 = evm_rpc::run_with_evm_rpc_rate_consensus(urls, |url| {
        let v = cfg.venus_vtoken_address.clone();
        let confirm = cfg.bsc_confirmation_blocks;
        let allowed = allowed.clone();
        async move { bsc::read_exchange_rate_stored(url, &v, confirm, &allowed).await }
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
        Ok((day_id, baseline)) => {
            info!(
                event = "check_rate_update",
                outcome = "ok",
                day_id,
                baseline = %baseline,
                proposed = %proposed,
                "policy allows oracle update"
            );
            let msg = ExecuteMsg::UpdateRate { new_rate: proposed };
            let txh = signer
                .sign_and_broadcast_execute(&cfg.oracle_contract, &msg)
                .await?;
            liveness
                .lock()
                .expect("liveness mutex poisoned")
                .record_successful_broadcast();
            info!(tx_hash = %txh, new_rate = %proposed, "submitted oracle update");
        }
        Err(e) => {
            info!(
                event = "check_rate_update",
                outcome = "failed",
                error = ?e,
                proposed = %proposed,
                current = %state.rate,
                "skip update (policy rejection)"
            );
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
