//! LCD queries and signed `wasm` execute for Terra Classic.
//! Adapted from `cl8y-bridge-monorepo` `multichain-rs` `terra/signer.rs` (broadcast + account query).
//!
//! # Broadcast vs confirmation (INV-ORACLE-LIVENESS-001)
//!
//! `BROADCAST_MODE_SYNC` `code == 0` means **CheckTx** (mempool admission) only.
//! Callers must use [`TerraSigner::wait_for_deliver_tx_success`] and
//! [`crate::confirm::oracle_state_matches_intended_update`] before recording liveness
//! ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23), audit C-3).
//! See `skills/oracle-liveness-confirm/SKILL.md`.

use base64::{engine::general_purpose::STANDARD, Engine};
use bip39::Mnemonic;
use cosmrs::bip32::DerivationPath;
use cosmrs::cosmwasm::MsgExecuteContract;
use cosmrs::crypto::secp256k1::SigningKey;
use cosmrs::tx::{self, Fee, Msg, SignDoc, SignerInfo};
use cosmrs::{AccountId, Coin};
use eyre::{eyre, Result, WrapErr};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::Serialize;
use std::time::{Duration, Instant};
use tracing::{info, warn};

pub const TERRA_DERIVATION_PATH: &str = "m/44'/330'/0'/0/0";
pub const DEFAULT_GAS_LIMIT: u64 = 500_000u64;
pub const DEFAULT_GAS_PRICE: f64 = 0.015_f64;

/// Bounded DeliverTx confirmation polling (env: `ORACLE_TX_CONFIRM_*`).
#[derive(Debug, Clone, Copy)]
pub struct ConfirmConfig {
    pub timeout: Duration,
    pub poll_interval: Duration,
}

impl ConfirmConfig {
    pub fn from_secs_and_ms(timeout_secs: u64, poll_interval_ms: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
            poll_interval: Duration::from_millis(poll_interval_ms.max(1)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerraSignerConfig {
    pub lcd_url: String,
    pub chain_id: String,
    pub mnemonic: SecretString,
    pub gas_limit: Option<u64>,
}

pub struct TerraSigner {
    signing_key: SigningKey,
    address: AccountId,
    lcd_url: String,
    chain_id: String,
    client: Client,
    gas_limit: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfo {
    pub sequence: u64,
    pub account_number: u64,
}

/// Parsed `tx_response` from LCD `GET /cosmos/tx/v1beta1/txs/{hash}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxResponseSummary {
    pub txhash: String,
    pub code: u64,
    pub raw_log: String,
}

/// True when a broadcast / CheckTx error indicates account sequence mismatch.
pub fn is_account_sequence_mismatch(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("account sequence mismatch")
        || lower.contains("incorrect account sequence")
        || (lower.contains("sequence") && lower.contains("mismatch"))
}

/// True for LCD/HTTP failures that are worth a bounded retry during confirmation polling.
pub fn is_transient_lcd_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection")
        || lower.contains("temporarily")
        || lower.contains("429")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
}

/// Strip userinfo and query string so API keys in LCD URLs never appear in logs/errors.
pub fn redact_url_credentials(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) => {
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.set_query(None);
            u.set_fragment(None);
            u.to_string()
        }
        Err(_) => {
            if let Some((base, _)) = url.split_once('?') {
                base.to_string()
            } else {
                "<invalid-url>".to_string()
            }
        }
    }
}

fn http_status_is_not_found(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND
}

fn tx_body_looks_not_found(body: &serde_json::Value) -> bool {
    let blob = body.to_string().to_ascii_lowercase();
    blob.contains("not found") || blob.contains("tx not found") || blob.contains("not_found")
}

/// Parse LCD get-tx JSON into a summary, or `None` if the tx is not yet indexed.
pub fn parse_tx_query_body(
    status: reqwest::StatusCode,
    body: &serde_json::Value,
) -> Result<Option<TxResponseSummary>> {
    if http_status_is_not_found(status) || tx_body_looks_not_found(body) {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(eyre!(
            "tx query http {}: {}",
            status.as_u16(),
            truncate_for_error(body)
        ));
    }
    let tx_response = body
        .get("tx_response")
        .cloned()
        .ok_or_else(|| eyre!("tx query missing tx_response"))?;
    let code = tx_response
        .get("code")
        .and_then(|c| c.as_u64())
        .unwrap_or(1);
    let txhash = tx_response
        .get("txhash")
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_string();
    let raw_log = tx_response
        .get("raw_log")
        .and_then(|l| l.as_str())
        .unwrap_or("")
        .to_string();
    if txhash.is_empty() {
        return Err(eyre!("tx query missing txhash"));
    }
    Ok(Some(TxResponseSummary {
        txhash,
        code,
        raw_log,
    }))
}

fn truncate_for_error(body: &serde_json::Value) -> String {
    let s = body.to_string();
    const MAX: usize = 256;
    if s.len() <= MAX {
        s
    } else {
        format!("{}…", &s[..MAX])
    }
}

impl TerraSigner {
    pub fn new(config: TerraSignerConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .wrap_err("HTTP client")?;
        let mnemonic = Mnemonic::parse(config.mnemonic.expose_secret())
            .map_err(|e| eyre!("mnemonic: {}", e))?;
        let seed = mnemonic.to_seed("");
        let path: DerivationPath = TERRA_DERIVATION_PATH
            .parse()
            .map_err(|e| eyre!("derivation path: {:?}", e))?;
        let signing_key =
            SigningKey::derive_from_path(seed, &path).map_err(|e| eyre!("derive key: {}", e))?;
        let address = signing_key
            .public_key()
            .account_id("terra")
            .map_err(|e| eyre!("account id: {}", e))?;
        Ok(Self {
            signing_key,
            address,
            lcd_url: config.lcd_url.trim_end_matches('/').to_string(),
            chain_id: config.chain_id,
            client,
            gas_limit: config.gas_limit.unwrap_or(DEFAULT_GAS_LIMIT),
        })
    }

    pub fn address_str(&self) -> String {
        self.address.to_string()
    }

    /// LCD base URL with credentials stripped (safe for structured logs).
    pub fn lcd_url_redacted(&self) -> String {
        redact_url_credentials(&self.lcd_url)
    }

    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.lcd_url, self.address
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(eyre!(
                "account query {} (lcd {})",
                response.status(),
                self.lcd_url_redacted()
            ));
        }
        let data: serde_json::Value = response.json().await?;
        let account = data
            .get("account")
            .ok_or_else(|| eyre!("missing account"))?;
        let sequence = account
            .get("sequence")
            .or_else(|| account.get("base_account").and_then(|b| b.get("sequence")))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let account_number = account
            .get("account_number")
            .or_else(|| {
                account
                    .get("base_account")
                    .and_then(|b| b.get("account_number"))
            })
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        Ok(AccountInfo {
            sequence,
            account_number,
        })
    }

    pub async fn query_wasm_smart<T: serde::de::DeserializeOwned>(
        &self,
        contract: &str,
        query_msg: &impl Serialize,
    ) -> Result<T> {
        let q = serde_json::to_vec(query_msg)?;
        let b64 = STANDARD.encode(&q);
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            self.lcd_url, contract, b64
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(eyre!(
                "wasm query {} (lcd {})",
                response.status(),
                self.lcd_url_redacted()
            ));
        }
        let body: serde_json::Value = response.json().await?;
        let data = body
            .get("data")
            .ok_or_else(|| eyre!("missing data field"))?;
        let data_str = data.as_str().ok_or_else(|| eyre!("data not string"))?;
        let raw = STANDARD.decode(data_str.trim())?;
        let parsed: T = serde_json::from_slice(&raw)?;
        Ok(parsed)
    }

    /// Sign + SYNC broadcast. On account sequence mismatch, re-queries account and retries **once**.
    pub async fn sign_and_broadcast_execute(
        &self,
        contract_address: &str,
        msg: &impl Serialize,
    ) -> Result<String> {
        let contract = contract_address.to_string();
        let first = self.sign_and_broadcast_execute_inner(&contract, msg).await;
        let result = match first {
            Ok(txh) => Ok(txh),
            Err(e) if is_account_sequence_mismatch(&e.to_string()) => {
                warn!(
                    event = "sign_and_broadcast_execute",
                    outcome = "sequence_mismatch_retry",
                    contract = %contract,
                    error = %e,
                    "account sequence mismatch; re-querying account and retrying once"
                );
                self.sign_and_broadcast_execute_inner(&contract, msg).await
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(txh) => {
                info!(
                    event = "sign_and_broadcast_execute",
                    outcome = "check_tx_ok",
                    contract = %contract,
                    tx_hash = %txh,
                    "broadcast CheckTx accepted (not yet DeliverTx-confirmed)"
                );
            }
            Err(e) => {
                warn!(
                    event = "sign_and_broadcast_execute",
                    outcome = "failed",
                    contract = %contract,
                    error = %e,
                    "sign_and_broadcast_execute failed"
                );
            }
        }
        result
    }

    /// Poll LCD until `expected_txhash` is included with DeliverTx `code == 0`, or timeout.
    ///
    /// Fail-closed: DeliverTx `code != 0`, hash mismatch, or timeout ⇒ `Err` (no liveness success).
    pub async fn wait_for_deliver_tx_success(
        &self,
        expected_txhash: &str,
        confirm: &ConfirmConfig,
    ) -> Result<()> {
        if expected_txhash.is_empty() {
            return Err(eyre!("empty tx hash cannot be confirmed"));
        }
        let deadline = Instant::now() + confirm.timeout;
        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            match self.query_tx_by_hash(expected_txhash).await {
                Ok(Some(summary)) => {
                    if !summary.txhash.eq_ignore_ascii_case(expected_txhash) {
                        return Err(eyre!(
                            "tx hash mismatch during confirmation: expected {}, got {} (fail-closed)",
                            expected_txhash,
                            summary.txhash
                        ));
                    }
                    if summary.code == 0 {
                        info!(
                            event = "wait_for_deliver_tx_success",
                            outcome = "ok",
                            tx_hash = %expected_txhash,
                            attempts = attempt,
                            "DeliverTx confirmed code 0"
                        );
                        return Ok(());
                    }
                    return Err(eyre!(
                        "DeliverTx failed code {}: {}",
                        summary.code,
                        summary.raw_log
                    ));
                }
                Ok(None) => {
                    // still in mempool / not indexed
                }
                Err(e) => {
                    let msg = e.to_string();
                    if is_transient_lcd_error(&msg) {
                        warn!(
                            event = "wait_for_deliver_tx_success",
                            outcome = "transient_lcd_error",
                            attempt,
                            error = %e,
                            lcd = %self.lcd_url_redacted(),
                            "transient LCD error while confirming tx; backing off"
                        );
                    } else {
                        return Err(e).wrap_err("tx confirmation query failed");
                    }
                }
            }

            if Instant::now() >= deadline {
                return Err(eyre!(
                    "tx confirmation timeout after {:?} waiting for DeliverTx of {} (fail-closed)",
                    confirm.timeout,
                    expected_txhash
                ));
            }

            // Bounded jitter on top of the configured poll interval (transient LCD / indexing lag).
            let jitter_ms = 250u64.saturating_mul((attempt % 4) as u64);
            let sleep_for = confirm.poll_interval + Duration::from_millis(jitter_ms);
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::sleep(sleep_for.min(remaining).max(Duration::from_millis(1))).await;
        }
    }

    pub async fn query_tx_by_hash(&self, txhash: &str) -> Result<Option<TxResponseSummary>> {
        let url = format!("{}/cosmos/tx/v1beta1/txs/{}", self.lcd_url, txhash);
        let response = self.client.get(&url).send().await.map_err(|e| {
            eyre!(
                "tx query transport error (lcd {}): {}",
                self.lcd_url_redacted(),
                e
            )
        })?;
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        parse_tx_query_body(status, &body)
    }

    async fn sign_and_broadcast_execute_inner(
        &self,
        contract_address: &str,
        msg: &impl Serialize,
    ) -> Result<String> {
        let account_info = self.get_account_info().await?;
        let gas_price = DEFAULT_GAS_PRICE;
        let fee_amount = ((self.gas_limit as f64) * gas_price).ceil() as u128;
        let msg_json = serde_json::to_vec(msg)?;
        let execute_msg = MsgExecuteContract {
            sender: self.address.clone(),
            contract: contract_address.parse()?,
            msg: msg_json,
            funds: vec![],
        };
        let body = tx::Body::new(
            vec![execute_msg.to_any().map_err(|e| eyre!("to_any: {}", e))?],
            "",
            0u32,
        );
        let signer_info =
            SignerInfo::single_direct(Some(self.signing_key.public_key()), account_info.sequence);
        let fee = Fee::from_amount_and_gas(
            Coin {
                denom: "uluna".parse()?,
                amount: fee_amount,
            },
            self.gas_limit,
        );
        let auth_info = signer_info.auth_info(fee);
        let chain_id = self.chain_id.parse()?;
        let sign_doc = SignDoc::new(&body, &auth_info, &chain_id, account_info.account_number)
            .map_err(|e| eyre!("sign doc: {}", e))?;
        let tx_raw = sign_doc
            .sign(&self.signing_key)
            .map_err(|e| eyre!("sign: {}", e))?;
        let tx_bytes = tx_raw.to_bytes().map_err(|e| eyre!("bytes: {}", e))?;
        let tx_b64 = STANDARD.encode(&tx_bytes);
        let broadcast_request = serde_json::json!({
            "tx_bytes": tx_b64,
            "mode": "BROADCAST_MODE_SYNC"
        });
        let broadcast_url = format!("{}/cosmos/tx/v1beta1/txs", self.lcd_url);
        let response = self
            .client
            .post(&broadcast_url)
            .json(&broadcast_request)
            .send()
            .await
            .map_err(|e| {
                eyre!(
                    "broadcast transport error (lcd {}): {}",
                    self.lcd_url_redacted(),
                    e
                )
            })?;
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        if !status.is_success() {
            return Err(eyre!(
                "broadcast http {}: {}",
                status,
                truncate_for_error(&body)
            ));
        }
        let tx_response = body.get("tx_response").cloned().unwrap_or_default();
        let code = tx_response
            .get("code")
            .and_then(|c| c.as_u64())
            .unwrap_or(1);
        let txhash = tx_response
            .get("txhash")
            .and_then(|h| h.as_str())
            .unwrap_or("")
            .to_string();
        if code != 0 {
            let raw_log = tx_response
                .get("raw_log")
                .and_then(|l| l.as_str())
                .unwrap_or("");
            return Err(eyre!("broadcast tx failed code {}: {}", code, raw_log));
        }
        if txhash.is_empty() {
            return Err(eyre!("broadcast succeeded CheckTx but missing txhash"));
        }
        Ok(txhash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    use ust1_oracle::msg::{ExecuteMsg, QueryMsg, StateResponse};
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn test_signer(lcd: &str) -> TerraSigner {
        TerraSigner::new(TerraSignerConfig {
            lcd_url: lcd.to_string(),
            chain_id: "localterra".to_string(),
            mnemonic: SecretString::new(TEST_MNEMONIC.to_string().into_boxed_str()),
            gas_limit: Some(200_000),
        })
        .unwrap()
    }

    fn account_json(sequence: u64) -> serde_json::Value {
        json!({
            "account": {
                "@type": "/cosmos.auth.v1beta1.BaseAccount",
                "address": "terra1...",
                "pub_key": null,
                "account_number": "1",
                "sequence": sequence.to_string()
            }
        })
    }

    fn state_lcd_body(rate: u128, last_update_sec: u64) -> serde_json::Value {
        let st = StateResponse {
            rate: cosmwasm_std::Uint128::new(rate),
            last_update_sec,
            utc_day_id: 1,
            day_baseline_rate: cosmwasm_std::Uint128::new(rate),
            paused: false,
        };
        let raw = serde_json::to_vec(&st).unwrap();
        json!({ "data": STANDARD.encode(raw) })
    }

    #[test]
    fn sequence_mismatch_detection() {
        assert!(is_account_sequence_mismatch(
            "broadcast tx failed code 32: account sequence mismatch, expected 5, got 4"
        ));
        assert!(is_account_sequence_mismatch("incorrect account sequence"));
        assert!(!is_account_sequence_mismatch("out of gas"));
    }

    #[test]
    fn redact_strips_query_api_key() {
        let redacted = redact_url_credentials("https://lcd.example/path?api_key=SECRET&x=1");
        assert!(!redacted.contains("SECRET"));
        assert!(!redacted.contains("api_key"));
        assert!(redacted.starts_with("https://lcd.example"));
    }

    #[test]
    fn parse_tx_not_found_variants() {
        let body = json!({"message": "tx not found"});
        assert!(parse_tx_query_body(reqwest::StatusCode::NOT_FOUND, &body)
            .unwrap()
            .is_none());
        assert!(parse_tx_query_body(reqwest::StatusCode::OK, &body)
            .unwrap()
            .is_none());
    }

    #[test]
    fn parse_tx_deliver_success_and_failure() {
        let ok = parse_tx_query_body(
            reqwest::StatusCode::OK,
            &json!({"tx_response": {"txhash": "ABC", "code": 0, "raw_log": "ok"}}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(ok.code, 0);
        assert_eq!(ok.txhash, "ABC");

        let fail = parse_tx_query_body(
            reqwest::StatusCode::OK,
            &json!({"tx_response": {"txhash": "ABC", "code": 5, "raw_log": "failed"}}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(fail.code, 5);
    }

    #[tokio::test]
    async fn confirm_success_after_inclusion() {
        let server = MockServer::start().await;
        let hash = "DEADBEEF";
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        signer
            .wait_for_deliver_tx_success(hash, &ConfirmConfig::from_secs_and_ms(5, 10))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn confirm_deliver_tx_failure_is_error() {
        let server = MockServer::start().await;
        let hash = "FAILTX01";
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": hash, "code": 7, "raw_log": "wasm error" }
            })))
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        let err = signer
            .wait_for_deliver_tx_success(hash, &ConfirmConfig::from_secs_and_ms(5, 10))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("DeliverTx failed"), "{err}");
    }

    #[tokio::test]
    async fn confirm_timeout_when_never_included() {
        let server = MockServer::start().await;
        let hash = "MISSING1";
        Mock::given(method("GET"))
            .and(path(format!("/cosmos/tx/v1beta1/txs/{hash}")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": "tx not found"
            })))
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        let err = signer
            .wait_for_deliver_tx_success(hash, &ConfirmConfig::from_secs_and_ms(1, 50))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("confirmation timeout"), "{err}");
    }

    #[tokio::test]
    async fn confirm_rejects_wrong_hash_in_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cosmos/tx/v1beta1/txs/EXPECTEDHASH"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": { "txhash": "OTHERHASH", "code": 0, "raw_log": "" }
            })))
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        let err = signer
            .wait_for_deliver_tx_success("EXPECTEDHASH", &ConfirmConfig::from_secs_and_ms(5, 10))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("hash mismatch"), "{err}");
    }

    #[tokio::test]
    async fn sequence_mismatch_retries_once_then_check_tx_ok() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmos/auth/v1beta1/accounts/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(account_json(1)))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": {
                    "txhash": "",
                    "code": 32,
                    "raw_log": "account sequence mismatch, expected 2, got 1"
                }
            })))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/cosmos/tx/v1beta1/txs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tx_response": {
                    "txhash": "RETRYHASH01",
                    "code": 0,
                    "raw_log": ""
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        // Valid bech32 contract for cosmrs AccountId parse.
        let contract = "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n";
        let msg = ExecuteMsg::UpdateRate {
            new_rate: cosmwasm_std::Uint128::new(1_000_000_000_000_000_000),
        };
        let txh = signer
            .sign_and_broadcast_execute(contract, &msg)
            .await
            .unwrap();
        assert_eq!(txh, "RETRYHASH01");
    }

    #[tokio::test]
    async fn end_to_end_check_tx_ok_then_state_confirm() {
        let server = MockServer::start().await;
        let hash = "E2EHASH01";
        let contract = "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n";

        Mock::given(method("GET"))
            .and(path_regex(r"^/cosmos/auth/v1beta1/accounts/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(account_json(3)))
            .mount(&server)
            .await;

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
            .and(path_regex(format!(
                r"^/cosmwasm/wasm/v1/contract/{}/smart/.*",
                regex::escape(contract)
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(state_lcd_body(2_000_000_000_000_000_000, 1_700_000_100)),
            )
            .mount(&server)
            .await;

        let signer = test_signer(&server.uri());
        let proposed = cosmwasm_std::Uint128::new(2_000_000_000_000_000_000);
        let msg = ExecuteMsg::UpdateRate { new_rate: proposed };
        let txh = signer
            .sign_and_broadcast_execute(contract, &msg)
            .await
            .unwrap();
        assert_eq!(txh, hash);
        signer
            .wait_for_deliver_tx_success(&txh, &ConfirmConfig::from_secs_and_ms(5, 10))
            .await
            .unwrap();
        let st: StateResponse = signer
            .query_wasm_smart(contract, &QueryMsg::State {})
            .await
            .unwrap();
        crate::confirm::oracle_state_matches_intended_update(1_700_000_000, &st, proposed).unwrap();
    }
}
