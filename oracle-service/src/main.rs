//! Poll BSC Venus vToken rate and update `ust1-oracle` on Terra Classic when policy allows.

mod bsc;
mod config;
mod evm_rpc;
mod healthz;
mod liveness;
mod terra_tx;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cosmwasm_std::Uint128;
use eyre::Result;
use tokio::signal;
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
    bsc::verify_all_bsc_rpc_urls(
        &cfg.bsc_rpc_urls,
        &cfg.allowed_bsc_chain_ids,
        cfg.bsc_rpc_timeout_secs,
    )
    .await?;
    let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
        lcd_url: cfg.terra_lcd_url.clone(),
        chain_id: cfg.terra_chain_id.clone(),
        mnemonic: cfg.terra_mnemonic.clone(),
        gas_limit: None,
        gas_price: cfg.terra_gas_price,
    })?;

    if !cfg.healthz_bind.is_empty() {
        healthz::spawn_healthz_server(&cfg.healthz_bind).await?;
    }

    let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));
    let max_silence = Duration::from_secs(cfg.max_silence_since_broadcast_secs);

    info!(
        address = %signer.address_str(),
        poll_interval_secs = cfg.poll_interval_secs,
        tick_timeout_secs = cfg.tick_timeout_secs,
        max_silence_since_broadcast_secs = cfg.max_silence_since_broadcast_secs,
        healthz_bind = %cfg.healthz_bind,
        "oracle operator"
    );

    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                info!("shutdown signal received; exiting gracefully");
                break;
            }
            _ = async {
                {
                    let tracker = liveness::lock_liveness_arc(&liveness);
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

                match tokio::time::timeout(
                    Duration::from_secs(cfg.tick_timeout_secs),
                    run_once(&cfg, &signer, &liveness),
                )
                .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => warn!(error = %e, "tick failed"),
                    Err(_) => warn!(
                        tick_timeout_secs = cfg.tick_timeout_secs,
                        "tick timed out"
                    ),
                }

                tokio::time::sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
            } => {}
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT handler");
        tokio::select! {
            _ = signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        signal::ctrl_c().await.expect("ctrl_c handler");
    }
}

async fn run_once(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    liveness: &Arc<Mutex<liveness::LivenessTracker>>,
) -> Result<()> {
    let urls = &cfg.bsc_rpc_urls;
    let allowed = cfg.allowed_bsc_chain_ids.clone();
    let timeout_secs = cfg.bsc_rpc_timeout_secs;
    let proposed: Uint128 = evm_rpc::run_with_evm_rpc_rate_consensus(urls, |url| {
        let v = cfg.venus_vtoken_address.clone();
        let confirm = cfg.bsc_confirmation_blocks;
        let allowed = allowed.clone();
        async move {
            bsc::read_exchange_rate_stored(url, &v, confirm, &allowed, timeout_secs).await
        }
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
            liveness::lock_liveness_arc(liveness).record_successful_broadcast();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn tick_timeout_returns_timed_out() {
        let outcome = tokio::time::timeout(Duration::from_millis(100), async {
            tokio::time::timeout(Duration::from_millis(50), sleep(Duration::from_secs(10)))
                .await
                .map_err(|_| ())
        })
        .await;
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_err());
    }

    #[tokio::test]
    async fn run_tick_with_timeout_times_out_on_slow_tick() {
        let cfg = config::Config {
            bsc_rpc_urls: vec!["http://a".into(), "http://b".into()],
            bsc_confirmation_blocks: 15,
            bsc_rpc_timeout_secs: 30,
            allowed_bsc_chain_ids: vec![56],
            venus_vtoken_address: String::new(),
            terra_lcd_url: String::new(),
            terra_chain_id: String::new(),
            terra_mnemonic: secrecy::SecretString::new("seed".to_string().into_boxed_str()),
            terra_gas_price: 0.015,
            oracle_contract: String::new(),
            poll_interval_secs: 1,
            tick_timeout_secs: 1,
            max_silence_since_broadcast_secs: 28_800,
            healthz_bind: String::new(),
        };
        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: "http://127.0.0.1:1".into(),
            chain_id: "columbus-5".into(),
            mnemonic: secrecy::SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: 0.015,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        async fn hang_forever(
            _cfg: &config::Config,
            _signer: &terra_tx::TerraSigner,
            _liveness: &Arc<Mutex<liveness::LivenessTracker>>,
        ) -> Result<()> {
            sleep(Duration::from_secs(60)).await;
            Ok(())
        }

        let timed_out = tokio::time::timeout(
            Duration::from_secs(cfg.tick_timeout_secs),
            hang_forever(&cfg, &signer, &liveness),
        )
        .await
        .is_err();
        assert!(timed_out);
    }
}
