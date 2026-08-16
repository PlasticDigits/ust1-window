//! Poll BSC Venus vToken rate and update `ust1-oracle` on Terra Classic when policy allows.
//!
//! Same-rate Venus readings still submit `UpdateRate` once **INV-ORACLE-THROTTLE-001**
//! allows (heartbeat) so `last_update_sec` stays inside window `max_oracle_age_sec`.
//! Liveness success is gated by **INV-ORACLE-LIVENESS-001** (DeliverTx wasm events, with
//! LCD `State` fallback when events are stripped); see `confirm`, `liveness`,
//! `skills/oracle-liveness-confirm/SKILL.md`, and `skills/oracle-ops-poll-silence/SKILL.md`
//! ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
//! [#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32)).

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
use ust1_common::oracle_policy::{check_rate_update, OraclePolicyError};
use ust1_oracle::msg::{ExecuteMsg, QueryMsg, StateResponse};

/// Outcome of the pure tick decision step (C-3 / **INV-ORACLE-LIVENESS-001**).
///
/// Liveness is recorded only on [`TickAction::Submit`] after DeliverTx + event (or State
/// fallback) confirm ([GitLab #28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28),
/// [#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32),
/// `skills/oracle-liveness-confirm/SKILL.md`, `skills/oracle-ops-poll-silence/SKILL.md`).
#[derive(Debug, PartialEq, Eq)]
enum TickAction {
    SkipPaused,
    /// Equal Venus vs on-chain rate, but **INV-ORACLE-THROTTLE-001** still blocks.
    SkipEqualRate,
    SkipPolicy(OraclePolicyError),
    Submit {
        day_id: u64,
        baseline: Uint128,
        /// `true` when `proposed == state.rate` (same-rate freshness refresh).
        heartbeat: bool,
    },
}

fn decide_tick_action(proposed: Uint128, state: &StateResponse, now: u64) -> TickAction {
    if state.paused {
        return TickAction::SkipPaused;
    }
    match check_rate_update(
        now,
        state.last_update_sec,
        state.rate,
        proposed,
        state.utc_day_id,
        state.day_baseline_rate,
    ) {
        Ok((day_id, baseline)) => TickAction::Submit {
            day_id,
            baseline,
            heartbeat: proposed == state.rate,
        },
        Err(e) => {
            if proposed == state.rate && matches!(e, OraclePolicyError::UpdateTooSoon { .. }) {
                TickAction::SkipEqualRate
            } else {
                TickAction::SkipPolicy(e)
            }
        }
    }
}

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
    for msg in
        config::ops_timing_warnings(cfg.poll_interval_secs, cfg.max_silence_since_broadcast_secs)
    {
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
         heartbeat same-rate UpdateRate after 4h throttle — #32; \
         liveness = confirmed DeliverTx wasm events or State fallback — C-3/#23/#32)"
    );

    operator_loop(&cfg, &signer, &liveness, Box::pin(shutdown_signal())).await
}

/// Main operator loop: poll BSC, update oracle, honor graceful shutdown (L-10 / [GitLab #28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28)).
///
/// When the shutdown future completes, `select!` breaks out immediately with `Ok(())`.
/// An in-flight tick body may still run to completion depending on which branch wins the race;
/// after shutdown fires we do not start another tick.
async fn operator_loop<F>(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    liveness: &Arc<Mutex<liveness::LivenessTracker>>,
    mut shutdown: F,
) -> Result<()>
where
    F: std::future::Future<Output = ()> + Unpin,
{
    let max_silence = Duration::from_secs(cfg.max_silence_since_broadcast_secs);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received; exiting gracefully");
                break;
            }
            _ = async {
                {
                    let tracker = liveness::lock_liveness_arc(liveness);
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
                    run_once(cfg, signer, liveness),
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
    shutdown_signal_with_hook(std::future::pending::<()>()).await
}

/// Injectable shutdown hook for unit tests; production passes `pending()` and relies on OS signals.
async fn shutdown_signal_with_hook<H>(mut hook: H)
where
    H: std::future::Future<Output = ()> + Unpin,
{
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT handler");
        tokio::select! {
            _ = signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
            _ = &mut hook => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = signal::ctrl_c() => {}
            _ = &mut hook => {}
        }
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

    match decide_tick_action(proposed, &state, now_unix()) {
        TickAction::SkipPaused => {
            // INV-ORACLE-PAUSE-001: governance circuit breaker — do not burn fees on UpdateRate.
            // Service does not auto-unpause; ops use DEPLOYMENT emergency pause runbook (#22).
            warn!(
                event = "oracle_paused",
                "on-chain oracle is paused (circuit breaker); skipping UpdateRate"
            );
        }
        TickAction::SkipEqualRate => {
            info!("no rate change from BSC");
        }
        TickAction::SkipPolicy(e) => {
            info!(
                event = "check_rate_update",
                outcome = "failed",
                error = ?e,
                proposed = %proposed,
                current = %state.rate,
                "skip update (policy rejection)"
            );
        }
        TickAction::Submit {
            day_id,
            baseline,
            heartbeat,
        } => {
            if heartbeat {
                info!(
                    event = "oracle_heartbeat",
                    tick_kind = "heartbeat",
                    outcome = "ok",
                    day_id,
                    baseline = %baseline,
                    proposed = %proposed,
                    "policy allows same-rate heartbeat UpdateRate"
                );
            } else {
                info!(
                    event = "check_rate_update",
                    tick_kind = "rate_change",
                    outcome = "ok",
                    day_id,
                    baseline = %baseline,
                    proposed = %proposed,
                    "policy allows oracle update"
                );
            }
            let txh = submit_and_confirm_oracle_update(cfg, signer, proposed, &state).await?;
            // INV-ORACLE-LIVENESS-001: DeliverTx + wasm events (or State fallback) — #23/#32.
            liveness::lock_liveness_arc(liveness).record_successful_broadcast();
            info!(
                tx_hash = %txh,
                new_rate = %proposed,
                tick_kind = if heartbeat { "heartbeat" } else { "rate_change" },
                "confirmed on-chain oracle update"
            );
        }
    }
    Ok(())
}

/// Broadcast `UpdateRate`, wait for DeliverTx success, then confirm via wasm events
/// (preferred) or retried LCD `State` (fallback when events are stripped).
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
    let summary = signer
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
    match confirm::oracle_tx_events_match_update(&cfg.oracle_contract, proposed, &summary.events) {
        Ok(confirm::OracleTxEventMatch::Matched) => {
            info!(
                event = "oracle_event_confirm",
                outcome = "ok",
                tx_hash = %txh,
                proposed = %proposed,
                "DeliverTx wasm events prove update_rate; recording liveness"
            );
            observe_state_after_event_ok(cfg, signer, prior, proposed, &txh).await;
            Ok(txh)
        }
        Ok(confirm::OracleTxEventMatch::EventsAbsent) => {
            confirm_state_with_retry(cfg, signer, prior, proposed, &txh).await?;
            Ok(txh)
        }
        Err(e) => {
            warn!(
                event = "oracle_event_confirm",
                outcome = "failed",
                tx_hash = %txh,
                error = %e,
                "DeliverTx wasm events did not prove this update; not recording liveness success"
            );
            Err(e)
        }
    }
}

/// After event-proven include, LCD `State` is observability only (INV-ORACLE-LIVENESS-001 / #32).
async fn observe_state_after_event_ok(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    prior: &StateResponse,
    proposed: Uint128,
    txh: &str,
) {
    match signer
        .query_wasm_smart(&cfg.oracle_contract, &QueryMsg::State {})
        .await
    {
        Ok(state) => {
            if let Err(e) =
                confirm::oracle_state_matches_intended_update(prior.last_update_sec, &state, proposed)
            {
                warn!(
                    event = "oracle_state_lag_after_event_ok",
                    outcome = "lag",
                    tx_hash = %txh,
                    error = %e,
                    "LCD State lagged after event-proven include; not fail-closing"
                );
            } else {
                info!(
                    event = "oracle_state_confirm",
                    outcome = "ok",
                    tx_hash = %txh,
                    "LCD State matches intended update"
                );
            }
        }
        Err(e) => {
            warn!(
                event = "oracle_state_lag_after_event_ok",
                outcome = "query_failed",
                tx_hash = %txh,
                error = %e,
                "LCD State query failed after event-proven include; not fail-closing"
            );
        }
    }
}

/// When wasm events are stripped, require retried `State` match before success.
async fn confirm_state_with_retry(
    cfg: &config::Config,
    signer: &terra_tx::TerraSigner,
    prior: &StateResponse,
    proposed: Uint128,
    txh: &str,
) -> Result<()> {
    let attempts = confirm::STATE_CONFIRM_RETRY_ATTEMPTS;
    let delay = Duration::from_millis(confirm::STATE_CONFIRM_RETRY_INTERVAL_MS);
    let mut last_err: Option<eyre::Report> = None;
    for attempt in 1..=attempts {
        match signer
            .query_wasm_smart(&cfg.oracle_contract, &QueryMsg::State {})
            .await
        {
            Ok(state) => {
                match confirm::oracle_state_matches_intended_update(
                    prior.last_update_sec,
                    &state,
                    proposed,
                ) {
                    Ok(()) => {
                        info!(
                            event = "oracle_state_confirm",
                            outcome = "ok",
                            tx_hash = %txh,
                            attempt,
                            "LCD State fallback matches intended update"
                        );
                        return Ok(());
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Err(e) => last_err = Some(e),
        }
        if attempt < attempts {
            tokio::time::sleep(delay).await;
        }
    }
    let e = last_err.unwrap_or_else(|| eyre::eyre!("oracle State fallback exhausted retries"));
    warn!(
        event = "oracle_state_confirm",
        outcome = "failed",
        tx_hash = %txh,
        error = %e,
        "oracle State did not reflect update after event-absent fallback; not recording liveness success"
    );
    Err(e)
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

    fn wasm_update_rate_events(contract: &str, rate: u128) -> serde_json::Value {
        json!([{
            "type": "wasm",
            "attributes": [
                {"key": "_contract_address", "value": contract},
                {"key": "action", "value": "update_rate"},
                {"key": "rate", "value": rate.to_string()}
            ]
        }])
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

    #[tokio::test]
    async fn matching_events_and_lagged_state_records_ok() {
        let server = MockServer::start().await;
        let hash = "EVENTLAG01";
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
                "tx_response": {
                    "txhash": hash,
                    "code": 0,
                    "raw_log": "",
                    "events": wasm_update_rate_events(CONTRACT, 2_000)
                }
            })))
            .mount(&server)
            .await;
        // LCD State still shows the previous rate (production lag).
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmwasm/wasm/v1/contract/.*/smart/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(1_000, 100)))
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

        let txh = submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap();
        assert_eq!(txh, hash);
        liveness::lock_liveness_arc(&liveness).record_successful_broadcast();
        assert!(liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn matching_events_and_matching_state_ok() {
        let server = MockServer::start().await;
        let hash = "EVENTOK01";
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
                "tx_response": {
                    "txhash": hash,
                    "code": 0,
                    "raw_log": "",
                    "events": wasm_update_rate_events(CONTRACT, 2_000)
                }
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

        submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn events_missing_state_match_after_retry_ok() {
        let server = MockServer::start().await;
        let hash = "STATERETRY";
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
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(1_000, 100)))
            .up_to_n_times(1)
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

        submit_and_confirm_oracle_update(&cfg, &signer, proposed, &prior)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wrong_contract_events_do_not_allow_liveness() {
        let server = MockServer::start().await;
        let hash = "WRONGCTR";
        let proposed = Uint128::new(2_000);
        let prior = StateResponse {
            rate: Uint128::new(1_000),
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(1_000),
            paused: false,
        };
        let other = "terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2";

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
                "tx_response": {
                    "txhash": hash,
                    "code": 0,
                    "raw_log": "",
                    "events": wasm_update_rate_events(other, 2_000)
                }
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
        assert!(err.to_string().contains("_contract_address"), "{err}");
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn event_rate_mismatch_does_not_allow_liveness() {
        let server = MockServer::start().await;
        let hash = "WRONGRATE";
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
                "tx_response": {
                    "txhash": hash,
                    "code": 0,
                    "raw_log": "",
                    "events": wasm_update_rate_events(CONTRACT, 1_999)
                }
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
        assert!(err.to_string().contains("rate does not match"), "{err}");
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

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use tokio::sync::oneshot;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn shutdown_test_cfg() -> config::Config {
        config::Config {
            bsc_rpc_urls: vec![
                "http://127.0.0.1:8545".into(),
                "http://127.0.0.1:8546".into(),
            ],
            bsc_confirmation_blocks: 0,
            bsc_rpc_timeout_secs: 30,
            allowed_bsc_chain_ids: vec![31337],
            venus_vtoken_address: "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba".into(),
            terra_lcd_url: "http://127.0.0.1:1".into(),
            terra_chain_id: "localterra".into(),
            terra_mnemonic: secrecy::SecretString::new(TEST_MNEMONIC.to_string().into_boxed_str()),
            terra_gas_price: terra_tx::DEFAULT_GAS_PRICE,
            oracle_contract: "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n"
                .into(),
            poll_interval_secs: 86_400,
            tick_timeout_secs: 120,
            max_silence_since_broadcast_secs: 28_800,
            tx_confirm_timeout_secs: 2,
            tx_confirm_poll_interval_ms: 20,
            healthz_bind: String::new(),
        }
    }

    /// L-10 / [GitLab #28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28): injectable hook
    /// mirrors the SIGTERM/SIGINT/ctrl_c select arm without sending signals to the test runner.
    #[tokio::test]
    async fn operator_loop_exits_on_shutdown_hook() {
        let (tx, rx) = oneshot::channel::<()>();
        let _ = tx.send(());
        let shutdown = Box::pin(async {
            let _ = rx.await;
        });

        let cfg = shutdown_test_cfg();
        let signer = terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: cfg.terra_lcd_url.clone(),
            chain_id: cfg.terra_chain_id.clone(),
            mnemonic: cfg.terra_mnemonic.clone(),
            gas_limit: None,
            gas_price: cfg.terra_gas_price,
        })
        .unwrap();
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            operator_loop(&cfg, &signer, &liveness, shutdown),
        )
        .await;
        assert!(
            result.is_ok(),
            "operator_loop should exit promptly on shutdown hook"
        );
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn shutdown_signal_with_hook_completes_when_hook_fires() {
        let (tx, rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            shutdown_signal_with_hook(Box::pin(async {
                let _ = rx.await;
            }))
            .await;
        });
        tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }
}

#[cfg(test)]
mod run_once_tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use secrecy::SecretString;
    use serde_json::json;
    use ust1_common::MIN_ORACLE_UPDATE_INTERVAL_SECS;
    use wiremock::matchers::{body_string_contains, method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const CONTRACT: &str = "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n";
    const VTOKEN: &str = "0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba";

    fn run_once_cfg(bsc_urls: Vec<String>, lcd: &str) -> config::Config {
        config::Config {
            bsc_rpc_urls: bsc_urls,
            bsc_confirmation_blocks: 0,
            bsc_rpc_timeout_secs: 5,
            allowed_bsc_chain_ids: vec![31337],
            venus_vtoken_address: VTOKEN.into(),
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

    fn state_lcd_body(rate: u128, last_update_sec: u64, paused: bool) -> serde_json::Value {
        let st = StateResponse {
            rate: Uint128::new(rate),
            last_update_sec,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(rate),
            paused,
        };
        json!({ "data": STANDARD.encode(serde_json::to_vec(&st).unwrap()) })
    }

    async fn mount_bsc_rpc(server: &MockServer, rate: u128) {
        let rate_hex = format!("0x{:064x}", rate);
        // shift = 0 when both decimals are 18 ⇒ oracle R equals exchangeRateStored.
        let decimals_hex = format!("0x{:064x}", 18u128);
        let underlying_hex = format!("0x{:064x}", 1u128);

        Mock::given(method("POST"))
            .and(body_string_contains("eth_chainId"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x7a69"
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_blockNumber"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x64"
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_call"))
            .and(body_string_contains("182df0f5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": rate_hex
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_call"))
            .and(body_string_contains("6f307dc3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": underlying_hex
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("eth_call"))
            .and(body_string_contains("313ce567"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": decimals_hex
            })))
            .mount(server)
            .await;
    }

    async fn mount_lcd_state(server: &MockServer, rate: u128, last_update_sec: u64) {
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmwasm/wasm/v1/contract/.*/smart/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(
                rate,
                last_update_sec,
                false,
            )))
            .mount(server)
            .await;
    }

    async fn mount_no_broadcast(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(server)
            .await;
    }

    fn wasm_update_rate_events(contract: &str, rate: u128) -> serde_json::Value {
        json!([{
            "type": "wasm",
            "attributes": [
                {"key": "_contract_address", "value": contract},
                {"key": "action", "value": "update_rate"},
                {"key": "rate", "value": rate.to_string()}
            ]
        }])
    }

    async fn mount_account_and_broadcast_ok(server: &MockServer, hash: &str, rate: u128) {
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmos/auth/v1beta1/accounts/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "account": {
                    "@type": "/cosmos.auth.v1beta1.BaseAccount",
                    "account_number": "1",
                    "sequence": "1"
                }
            })))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": {
                    "txhash": hash,
                    "code": 0,
                    "raw_log": "",
                    "events": wasm_update_rate_events(CONTRACT, rate)
                }
            })))
            .mount(server)
            .await;
    }

    fn test_signer(lcd: &str, mnemonic: &SecretString) -> terra_tx::TerraSigner {
        terra_tx::TerraSigner::new(terra_tx::TerraSignerConfig {
            lcd_url: lcd.to_string(),
            chain_id: "localterra".into(),
            mnemonic: mnemonic.clone(),
            gas_limit: Some(200_000),
            gas_price: terra_tx::DEFAULT_GAS_PRICE,
        })
        .unwrap()
    }

    #[test]
    fn decide_tick_action_equal_rate_inside_throttle_skips() {
        let rate = Uint128::new(1_000_000);
        let now = 100_000u64;
        let state = StateResponse {
            rate,
            last_update_sec: now - 100,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(rate, &state, now),
            TickAction::SkipEqualRate
        );
    }

    #[test]
    fn decide_tick_action_equal_rate_one_second_before_throttle_skips() {
        let rate = Uint128::new(1_000_000);
        let last = 100_000u64;
        let now = last + MIN_ORACLE_UPDATE_INTERVAL_SECS - 1;
        let state = StateResponse {
            rate,
            last_update_sec: last,
            utc_day_id: last / 86_400,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(rate, &state, now),
            TickAction::SkipEqualRate
        );
    }

    #[test]
    fn decide_tick_action_equal_rate_after_throttle_is_heartbeat() {
        let rate = Uint128::new(1_000_000);
        let last = 100_000u64;
        let now = last + MIN_ORACLE_UPDATE_INTERVAL_SECS;
        let state = StateResponse {
            rate,
            last_update_sec: last,
            utc_day_id: last / 86_400,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(rate, &state, now),
            TickAction::Submit {
                day_id: now / 86_400,
                baseline: rate,
                heartbeat: true,
            }
        );
    }

    #[test]
    fn decide_tick_action_rate_change_after_throttle_is_not_heartbeat() {
        let rate = Uint128::new(1_000_000);
        let proposed = Uint128::new(1_000_001);
        let last = 100_000u64;
        let now = last + MIN_ORACLE_UPDATE_INTERVAL_SECS;
        let state = StateResponse {
            rate,
            last_update_sec: last,
            utc_day_id: last / 86_400,
            day_baseline_rate: rate,
            paused: false,
        };
        match decide_tick_action(proposed, &state, now) {
            TickAction::Submit { heartbeat, .. } => assert!(!heartbeat),
            other => panic!("expected Submit rate_change, got {other:?}"),
        }
    }

    #[test]
    fn decide_tick_action_first_update_equal_rate_submits_heartbeat() {
        let rate = Uint128::new(1_000_000);
        let state = StateResponse {
            rate,
            last_update_sec: 0,
            utc_day_id: 0,
            day_baseline_rate: rate,
            paused: false,
        };
        match decide_tick_action(rate, &state, 200) {
            TickAction::Submit {
                heartbeat: true, ..
            } => {}
            other => panic!("expected bootstrap heartbeat Submit, got {other:?}"),
        }
    }

    #[test]
    fn decide_tick_action_paused_equal_rate_skips() {
        let rate = Uint128::new(1_000_000);
        let now = 100_000u64;
        let state = StateResponse {
            rate,
            last_update_sec: now - MIN_ORACLE_UPDATE_INTERVAL_SECS - 10,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: true,
        };
        assert_eq!(
            decide_tick_action(rate, &state, now),
            TickAction::SkipPaused
        );
    }

    #[test]
    fn decide_tick_action_equal_rate_skips() {
        let rate = Uint128::new(1_000_000);
        let state = StateResponse {
            rate,
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(rate, &state, 200),
            TickAction::SkipEqualRate
        );
    }

    #[test]
    fn decide_tick_action_paused_skips() {
        let rate = Uint128::new(1_000_000);
        let state = StateResponse {
            rate,
            last_update_sec: 100,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: true,
        };
        assert_eq!(
            decide_tick_action(Uint128::new(2_000_000), &state, 200),
            TickAction::SkipPaused
        );
    }

    #[test]
    fn decide_tick_action_update_too_soon() {
        let rate = Uint128::new(1_000_000);
        let now = 100_000u64;
        let state = StateResponse {
            rate,
            last_update_sec: now - 100,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(Uint128::new(1_000_001), &state, now),
            TickAction::SkipPolicy(OraclePolicyError::UpdateTooSoon {
                min_interval: MIN_ORACLE_UPDATE_INTERVAL_SECS,
            })
        );
    }

    #[test]
    fn decide_tick_action_rate_decreased() {
        let rate = Uint128::new(1_000_000);
        let now = 100_000u64;
        let state = StateResponse {
            rate,
            last_update_sec: now - MIN_ORACLE_UPDATE_INTERVAL_SECS - 10,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(Uint128::new(999_000), &state, now),
            TickAction::SkipPolicy(OraclePolicyError::RateDecreased)
        );
    }

    #[test]
    fn decide_tick_action_daily_cap_exceeded() {
        let rate = Uint128::new(1_000_000);
        let now = 100_000u64;
        let state = StateResponse {
            rate,
            last_update_sec: now - MIN_ORACLE_UPDATE_INTERVAL_SECS - 10,
            utc_day_id: 1,
            day_baseline_rate: rate,
            paused: false,
        };
        assert_eq!(
            decide_tick_action(Uint128::new(1_030_000), &state, now),
            TickAction::SkipPolicy(OraclePolicyError::DailyCapExceeded)
        );
    }

    /// **INV-ORACLE-LIVENESS-001** / C-3 ([GitLab #28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28),
    /// [#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32)):
    /// equal BSC rate inside the 4h throttle must not broadcast or record liveness.
    #[tokio::test]
    async fn run_once_equal_rate_does_not_record_liveness() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;

        mount_bsc_rpc(&bsc0, rate).await;
        mount_bsc_rpc(&bsc1, rate).await;
        let now = now_unix();
        mount_lcd_state(&lcd, rate, now.saturating_sub(100)).await;
        mount_no_broadcast(&lcd).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        run_once(&cfg, &signer, &liveness).await.unwrap();
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    /// Same-rate after throttle: heartbeat `UpdateRate` + event confirm records liveness
    /// even when post-include LCD State is still lagged.
    #[tokio::test]
    async fn run_once_equal_rate_heartbeat_records_liveness_on_events() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;
        let hash = "HEARTBEAT1";

        mount_bsc_rpc(&bsc0, rate).await;
        mount_bsc_rpc(&bsc1, rate).await;
        let now = now_unix();
        let last = now.saturating_sub(MIN_ORACLE_UPDATE_INTERVAL_SECS + 10);
        mount_lcd_state(&lcd, rate, last).await;
        mount_account_and_broadcast_ok(&lcd, hash, rate).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        run_once(&cfg, &signer, &liveness).await.unwrap();
        assert!(liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn run_once_paused_equal_rate_does_not_broadcast() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;

        mount_bsc_rpc(&bsc0, rate).await;
        mount_bsc_rpc(&bsc1, rate).await;
        let now = now_unix();
        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmwasm/wasm/v1/contract/.*/smart/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(state_lcd_body(
                rate,
                now.saturating_sub(MIN_ORACLE_UPDATE_INTERVAL_SECS + 10),
                true,
            )))
            .mount(&lcd)
            .await;
        mount_no_broadcast(&lcd).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        run_once(&cfg, &signer, &liveness).await.unwrap();
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    #[tokio::test]
    async fn run_once_evm_disagreement_does_not_heartbeat() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;

        mount_bsc_rpc(&bsc0, rate).await;
        mount_bsc_rpc(&bsc1, rate + 10_000).await;
        let now = now_unix();
        mount_lcd_state(&lcd, rate, now.saturating_sub(MIN_ORACLE_UPDATE_INTERVAL_SECS + 10)).await;
        mount_no_broadcast(&lcd).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        let err = run_once(&cfg, &signer, &liveness).await.unwrap_err();
        assert!(
            err.to_string().contains("mismatch") || err.to_string().contains("disagree"),
            "{err}"
        );
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    /// Policy throttle skip must not broadcast or record liveness.
    #[tokio::test]
    async fn run_once_policy_throttle_does_not_record_liveness() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;
        let proposed = rate + 1;

        mount_bsc_rpc(&bsc0, proposed).await;
        mount_bsc_rpc(&bsc1, proposed).await;
        let now = now_unix();
        mount_lcd_state(&lcd, rate, now - 100).await;
        mount_no_broadcast(&lcd).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        run_once(&cfg, &signer, &liveness).await.unwrap();
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }

    /// Monotonic decrease skip must not broadcast or record liveness.
    #[tokio::test]
    async fn run_once_mono_decrease_does_not_record_liveness() {
        let bsc0 = MockServer::start().await;
        let bsc1 = MockServer::start().await;
        let lcd = MockServer::start().await;
        let rate = 1_000_000u128;
        let proposed = rate - 1;

        mount_bsc_rpc(&bsc0, proposed).await;
        mount_bsc_rpc(&bsc1, proposed).await;
        let now = now_unix();
        mount_lcd_state(&lcd, rate, now - MIN_ORACLE_UPDATE_INTERVAL_SECS - 10).await;
        mount_no_broadcast(&lcd).await;

        let cfg = run_once_cfg(vec![bsc0.uri(), bsc1.uri()], &lcd.uri());
        let signer = test_signer(&lcd.uri(), &cfg.terra_mnemonic);
        let liveness = Arc::new(Mutex::new(liveness::LivenessTracker::new()));

        run_once(&cfg, &signer, &liveness).await.unwrap();
        assert!(!liveness.lock().unwrap().has_recorded_success());
    }
}
