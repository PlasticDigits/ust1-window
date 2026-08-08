//! Poll BSC Venus vToken rate and update `ust1-oracle` on Terra Classic when policy allows.
//!
//! Liveness success is gated by **INV-ORACLE-LIVENESS-001** (DeliverTx + oracle state);
//! see `confirm`, `liveness`, and `skills/oracle-liveness-confirm/SKILL.md`
//! ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)).

mod bsc;
mod config;
mod confirm;
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
    for msg in config::ops_timing_warnings(
        cfg.poll_interval_secs,
        cfg.max_silence_since_broadcast_secs,
    ) {
        warn!(target: "ust1_oracle_service", alert = "ORACLE_OPS_TIMING_MISCONFIG", "{msg}");
    }
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
        window_default_max_oracle_age_secs = ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS,
        tx_confirm_timeout_secs = cfg.tx_confirm_timeout_secs,
        tx_confirm_poll_interval_ms = cfg.tx_confirm_poll_interval_ms,
        healthz_bind = %cfg.healthz_bind,
        "oracle operator (poll ≪ max_oracle_age; silence ≤ max_oracle_age — H-3/#24; \
         liveness = confirmed DeliverTx + State — C-3/#23)"
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
                            "LIVENESS ALERT: no confirmed on-chain Terra oracle update within the configured silence window \
                             (default aligns with window max oracle age — swaps may already be rejected); \
                             investigate LCD, BSC RPC, keys, policy, and DeliverTx confirmation"
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

    // INV-ORACLE-PAUSE-001: governance circuit breaker — do not burn fees on UpdateRate.
    // Service does not auto-unpause; ops use DEPLOYMENT emergency pause runbook (#22).
    if state.paused {
        warn!(
            event = "oracle_paused",
            "on-chain oracle is paused (circuit breaker); skipping UpdateRate"
        );
        return Ok(());
    }

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
            let txh = submit_and_confirm_oracle_update(cfg, signer, proposed, &state).await?;
            liveness::lock_liveness_arc(liveness).record_successful_broadcast();
            info!(
                tx_hash = %txh,
                new_rate = %proposed,
                "confirmed on-chain oracle update"
            );
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

/// Broadcast `UpdateRate`, wait for DeliverTx success, then verify oracle `State`.
///
/// Does **not** record liveness — caller records only after this returns `Ok`
/// (INV-ORACLE-LIVENESS-001).
async fn submit_and_confirm_oracle_update(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    proposed: Uint128,
    prior: &StateResponse,
) -> Result<String> {
    let msg = ExecuteMsg::UpdateRate { new_rate: proposed };
    let txh = signer
        .sign_and_broadcast_execute(&cfg.oracle_contract, &msg)
        .await?;
    let confirm = terra_tx::ConfirmConfig::from_secs_and_ms(
        cfg.tx_confirm_timeout_secs,
        cfg.tx_confirm_poll_interval_ms,
    );
    signer
        .wait_for_deliver_tx_success(&txh, &confirm)
        .await
        .map_err(|e| {
            warn!(
                event = "oracle_tx_confirm",
                outcome = "failed",
                tx_hash = %txh,
                error = %e,
                "oracle update not confirmed; not recording liveness success"
            );
            e
        })?;
    let state: StateResponse = signer
        .query_wasm_smart(&cfg.oracle_contract, &QueryMsg::State {})
        .await?;
    confirm::oracle_state_matches_intended_update(prior.last_update_sec, &state, proposed)
        .map_err(|e| {
            warn!(
                event = "oracle_state_confirm",
                outcome = "failed",
                tx_hash = %txh,
                error = %e,
                "oracle State did not reflect update; not recording liveness success"
            );
            e
        })?;
    Ok(txh)
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod submit_confirm_tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use secrecy::SecretString;
    use serde_json::json;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const CONTRACT: &str = "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n";

    fn test_cfg(lcd: &str) -> config::Config {
        config::Config {
            bsc_rpc_urls: vec![
                "http://127.0.0.1:8545".into(),
                "http://127.0.0.1:8546".into(),
            ],
            bsc_confirmation_blocks: 1,
            bsc_rpc_timeout_secs: 30,
            allowed_bsc_chain_ids: vec![31337],
            venus_vtoken_address: "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba".into(),
            terra_lcd_url: lcd.to_string(),
            terra_chain_id: "localterra".into(),
            terra_mnemonic: SecretString::new(TEST_MNEMONIC.to_string().into_boxed_str()),
            terra_gas_price: terra_tx::DEFAULT_GAS_PRICE,
            oracle_contract: CONTRACT.into(),
            poll_interval_secs: 60,
            tick_timeout_secs: 120,
            max_silence_since_broadcast_secs: 28_800,
            tx_confirm_timeout_secs: 2,
            tx_confirm_poll_interval_ms: 20,
            healthz_bind: String::new(),
        }
    }

    fn account_json(sequence: u64) -> serde_json::Value {
        json!({
            "account": {
                "@type": "/cosmos.auth.v1beta1.BaseAccount",
                "account_number": "1",
                "sequence": sequence.to_string()
            }
        })
    }

    fn state_lcd_body(rate: u128, last_update_sec: u64) -> serde_json::Value {
        let st = StateResponse {
            rate: Uint128::new(rate),
            last_update_sec,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(rate),
            paused: false,
        };
        json!({ "data": STANDARD.encode(serde_json::to_vec(&st).unwrap()) })
    }

    async fn mount_account(server: &MockServer, sequence: u64) {
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmos/auth/v1beta1/accounts/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(account_json(sequence)))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn happy_path_records_liveness_only_after_confirm() {
        let server = MockServer::start().await;
        let hash = "HAPPYTX01";
        let proposed = Uint128::new(2_000);
        let prior = StateResponse {
            rate: Uint128::new(1_000),
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(1_000),
            paused: false,
        };

        mount_account(&server, 1).await;
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmwasm/wasm/v1/contract/.*/smart/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(2_000, 200)))
            .mount(&server)
            .await;

        let cfg = test_cfg(&server.uri());
        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "localterra".into(),
            mnemonic: cfg.terra_mnemonic.clone(),
            gas_limit: Some(200_000),
            gas_price: cfg.terra_gas_price,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));
        assert!(!liveness.lock().unwrap().has_recorded_success());

        let txh = submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap();
        assert_eq!(txh, hash);
        liveness::lock_liveness_arc(&liveness).record_successful_broadcast();
        assert!(liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn deliver_tx_failure_does_not_allow_liveness() {
        let server = MockServer::start().await;
        let hash = "DELIVERFAIL";
        let proposed = Uint128::new(2_000);
        let prior = StateResponse {
            rate: Uint128::new(1_000),
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(1_000),
            paused: false,
        };

        mount_account(&server, 1).await;
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 9, "raw_log": "policy rejected" }
            })))
            .mount(&server)
            .await;

        let cfg = test_cfg(&server.uri());
        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "localterra".into(),
            mnemonic: cfg.terra_mnemonic.clone(),
            gas_limit: Some(200_000),
            gas_price: cfg.terra_gas_price,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        let err = submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("DeliverTx failed"), "{err}");
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn state_mismatch_does_not_allow_liveness() {
        let server = MockServer::start().await;
        let hash = "STATEMISMATCH";
        let proposed = Uint128::new(2_000);
        let prior = StateResponse {
            rate: Uint128::new(1_000),
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(1_000),
            paused: false,
        };

        mount_account(&server, 1).await;
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmwasm/wasm/v1/contract/.*/smart/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(1_500, 200)))
            .mount(&server)
            .await;

        let cfg = test_cfg(&server.uri());
        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "localterra".into(),
            mnemonic: cfg.terra_mnemonic.clone(),
            gas_limit: Some(200_000),
            gas_price: cfg.terra_gas_price,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        let err = submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("rate mismatch"), "{err}");
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn confirmation_timeout_does_not_allow_liveness() {
        let server = MockServer::start().await;
        let hash = "TIMEOUTTX";
        let proposed = Uint128::new(2_000);
        let prior = StateResponse {
            rate: Uint128::new(1_000),
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(1_000),
            paused: false,
        };

        mount_account(&server, 1).await;
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": "tx not found"
            })))
            .mount(&server)
            .await;

        let mut cfg = test_cfg(&server.uri());
        cfg.tx_confirm_poll_interval_ms = 10;
        cfg.tx_confirm_timeout_secs = 1;

        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "localterra".into(),
            mnemonic: cfg.terra_mnemonic.clone(),
            gas_limit: Some(200_000),
            gas_price: cfg.terra_gas_price,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        let err = submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("confirmation timeout"), "{err}");
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }
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
            max_silence_since_broadcast_secs: 21_600,
            tx_confirm_timeout_secs: 90,
            tx_confirm_poll_interval_ms: 2_000,
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
