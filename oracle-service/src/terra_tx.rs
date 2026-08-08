//! LCD queries and signed `wasm` execute for Terra Classic.
//! Adapted from `cl8y-bridge-monorepo` `multichain-rs` `terra/signer.rs` (broadcast + account query).

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
use std::time::Duration;
use tracing::{info, warn};

pub const TERRA_DERIVATION_PATH: &str = "m/44'/330'/0'/0/0";
pub const DEFAULT_GAS_LIMIT: u64 = 500_000u64;
pub const DEFAULT_GAS_PRICE: f64 = 0.015_f64;

#[derive(Debug, Clone)]
pub struct TerraSignerConfig {
    pub lcd_url: String,
    pub chain_id: String,
    pub mnemonic: SecretString,
    pub gas_limit: Option<u64>,
    /// Configured gas price floor (uluna per gas unit).
    pub gas_price: f64,
}

pub struct TerraSigner {
    signing_key: SigningKey,
    address: AccountId,
    lcd_url: String,
    chain_id: String,
    client: Client,
    gas_limit: u64,
    gas_price: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AccountInfo {
    pub sequence: u64,
    pub account_number: u64,
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
            gas_price: config.gas_price,
        })
    }

    pub fn address_str(&self) -> String {
        self.address.to_string()
    }

    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.lcd_url, self.address
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(eyre!("account query {}", response.status()));
        }
        let data: serde_json::Value = response.json().await?;
        parse_account_info(&data)
    }

    /// Query the node minimum gas price when the LCD exposes it.
    ///
    /// Terra Classic nodes often serve `/cosmos/base/node/v1beta1/config` with
    /// `minimum_gas_price` like `"0.015uluna"`. When the endpoint is missing or unparsable,
    /// returns `Ok(None)` so callers can fall back to the configured floor.
    pub async fn query_network_min_gas_price(&self) -> Result<Option<f64>> {
        let url = format!("{}/cosmos/base/node/v1beta1/config", self.lcd_url);
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(eyre!(
                "node config query {} (fallback to configured TERRA_GAS_PRICE)",
                response.status()
            ));
        }
        let data: serde_json::Value = response.json().await?;
        Ok(parse_minimum_gas_price(&data))
    }

    /// `max(configured, network_min)` when the network probe succeeds; otherwise configured.
    pub async fn resolve_gas_price(&self) -> f64 {
        match self.query_network_min_gas_price().await {
            Ok(network_min) => effective_gas_price(self.gas_price, network_min),
            Err(e) => {
                warn!(
                    error = %e,
                    configured = self.gas_price,
                    "network min gas price probe failed; using configured TERRA_GAS_PRICE floor"
                );
                self.gas_price
            }
        }
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
            return Err(eyre!("wasm query {}", response.status()));
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

    pub async fn sign_and_broadcast_execute(
        &self,
        contract_address: &str,
        msg: &impl Serialize,
    ) -> Result<String> {
        let contract = contract_address.to_string();
        let result = self.sign_and_broadcast_execute_inner(&contract, msg).await;
        match &result {
            Ok(txh) => {
                info!(
                    event = "sign_and_broadcast_execute",
                    outcome = "ok",
                    contract = %contract,
                    tx_hash = %txh,
                    "broadcast accepted"
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

    async fn sign_and_broadcast_execute_inner(
        &self,
        contract_address: &str,
        msg: &impl Serialize,
    ) -> Result<String> {
        let account_info = self.get_account_info().await?;
        let gas_price = self.resolve_gas_price().await;
        let fee_amount = compute_fee_amount(self.gas_limit, gas_price);
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
            .await?;
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        if !status.is_success() {
            return Err(eyre!("broadcast http {}: {}", status, body));
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
        Ok(txhash)
    }
}

/// Parse LCD `/cosmos/auth/v1beta1/accounts/{addr}` JSON into sequence + account_number.
pub(crate) fn parse_account_info(value: &serde_json::Value) -> Result<AccountInfo> {
    let account = value
        .get("account")
        .ok_or_else(|| eyre!("missing account field in LCD response"))?;
    let inner = resolve_account_object(account)?;
    let sequence = extract_u64_field(inner, "sequence")?;
    let account_number = extract_u64_field(inner, "account_number")?;
    Ok(AccountInfo {
        sequence,
        account_number,
    })
}

/// Walk common columbus-5 account wrappers (`BaseAccount`, vesting, etc.).
fn resolve_account_object(account: &serde_json::Value) -> Result<&serde_json::Value> {
    if account.get("sequence").is_some() || account.get("account_number").is_some() {
        return Ok(account);
    }
    for key in [
        "base_account",
        "base_vesting_account",
        "base_delayed_vesting_account",
        "base_continuous_vesting_account",
        "base_periodic_vesting_account",
    ] {
        if let Some(nested) = account.get(key) {
            return resolve_account_object(nested);
        }
    }
    Err(eyre!(
        "could not locate sequence/account_number in account JSON (type={})",
        account
            .get("@type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown")
    ))
}

fn extract_u64_field(obj: &serde_json::Value, field: &str) -> Result<u64> {
    let value = obj
        .get(field)
        .ok_or_else(|| eyre!("missing account field {field}"))?;
    parse_json_u64(value, field)
}

fn parse_json_u64(value: &serde_json::Value, field: &str) -> Result<u64> {
    match value {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| eyre!("{field} is not a valid u64 number")),
        serde_json::Value::String(s) => s
            .parse::<u64>()
            .map_err(|e| eyre!("{field} string {:?} is not a valid u64: {e}", s)),
        _ => Err(eyre!("{field} must be a string or number, got {value}")),
    }
}

/// Parse `minimum_gas_price` from `/cosmos/base/node/v1beta1/config` (e.g. `"0.015uluna"`).
pub(crate) fn parse_minimum_gas_price(value: &serde_json::Value) -> Option<f64> {
    let raw = value
        .get("config")
        .and_then(|c| c.get("minimum_gas_price"))
        .or_else(|| value.get("minimum_gas_price"))
        .and_then(|v| v.as_str())?;
    parse_gas_price_denom(raw)
}

fn parse_gas_price_denom(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (num, denom) = if let Some(idx) = trimmed.find("uluna") {
        (&trimmed[..idx], "uluna")
    } else if let Some(idx) = trimmed.find("uusd") {
        (&trimmed[..idx], "uusd")
    } else {
        (trimmed, "")
    };
    if !denom.is_empty() && num.is_empty() {
        return None;
    }
    num.parse::<f64>().ok()
}

/// Use the higher of configured floor and network minimum when the probe succeeds.
pub(crate) fn effective_gas_price(configured: f64, network_min: Option<f64>) -> f64 {
    match network_min {
        Some(network) if network > configured => network,
        _ => configured,
    }
}

/// Fee amount in uluna: `ceil(gas_limit * gas_price)`.
pub(crate) fn compute_fee_amount(gas_limit: u64, gas_price: f64) -> u128 {
    ((gas_limit as f64) * gas_price).ceil() as u128
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    use serde_json::json;

    #[test]
    fn parse_base_account_top_level() {
        let body = json!({
            "account": {
                "@type": "/cosmos.auth.v1beta1.BaseAccount",
                "account_number": "42",
                "sequence": "7"
            }
        });
        let info = parse_account_info(&body).unwrap();
        assert_eq!(
            info,
            AccountInfo {
                sequence: 7,
                account_number: 42
            }
        );
    }

    #[test]
    fn parse_base_account_numeric_fields() {
        let body = json!({
            "account": {
                "@type": "/cosmos.auth.v1beta1.BaseAccount",
                "account_number": 99,
                "sequence": 3
            }
        });
        let info = parse_account_info(&body).unwrap();
        assert_eq!(
            info,
            AccountInfo {
                sequence: 3,
                account_number: 99
            }
        );
    }

    #[test]
    fn parse_vesting_wrapper() {
        let body = json!({
            "account": {
                "@type": "/cosmos.vesting.v1beta1.ContinuousVestingAccount",
                "base_vesting_account": {
                    "base_account": {
                        "account_number": "100",
                        "sequence": "5"
                    }
                }
            }
        });
        let info = parse_account_info(&body).unwrap();
        assert_eq!(
            info,
            AccountInfo {
                sequence: 5,
                account_number: 100
            }
        );
    }

    #[test]
    fn parse_missing_sequence_errors() {
        let body = json!({
            "account": {
                "@type": "/cosmos.auth.v1beta1.BaseAccount",
                "account_number": "1"
            }
        });
        let err = parse_account_info(&body).unwrap_err();
        assert!(err.to_string().contains("sequence"));
    }

    #[test]
    fn parse_missing_account_field_errors() {
        let body = json!({ "foo": "bar" });
        let err = parse_account_info(&body).unwrap_err();
        assert!(err.to_string().contains("missing account"));
    }

    #[test]
    fn effective_gas_price_uses_max() {
        assert_eq!(effective_gas_price(0.015, Some(0.02)), 0.02);
        assert_eq!(effective_gas_price(0.015, Some(0.01)), 0.015);
        assert_eq!(effective_gas_price(0.015, None), 0.015);
    }

    #[test]
    fn compute_fee_amount_ceils() {
        assert_eq!(compute_fee_amount(500_000, 0.015), 7_500);
        assert_eq!(compute_fee_amount(1, 0.015), 1);
    }

    #[test]
    fn parse_minimum_gas_price_uluna() {
        let body = json!({
            "config": {
                "minimum_gas_price": "0.015uluna"
            }
        });
        assert_eq!(parse_minimum_gas_price(&body), Some(0.015));
    }

    #[test]
    fn parse_minimum_gas_price_missing() {
        assert_eq!(parse_minimum_gas_price(&json!({})), None);
    }

    #[tokio::test]
    async fn get_account_info_wiremock_base_account() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r"/cosmos/auth/v1beta1/accounts/terra1.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "account": {
                    "@type": "/cosmos.auth.v1beta1.BaseAccount",
                    "account_number": "12",
                    "sequence": "4"
                }
            })))
            .mount(&server)
            .await;

        let signer = TerraSigner::new(TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "columbus-5".into(),
            mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: DEFAULT_GAS_PRICE,
        })
        .unwrap();

        let info = signer.get_account_info().await.unwrap();
        assert_eq!(
            info,
            AccountInfo {
                sequence: 4,
                account_number: 12
            }
        );
    }

    #[tokio::test]
    async fn get_account_info_wiremock_missing_sequence_errors() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r"/cosmos/auth/v1beta1/accounts/terra1.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "account": {
                    "@type": "/cosmos.auth.v1beta1.BaseAccount",
                    "account_number": "1"
                }
            })))
            .mount(&server)
            .await;

        let signer = TerraSigner::new(TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "columbus-5".into(),
            mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: DEFAULT_GAS_PRICE,
        })
        .unwrap();

        let err = signer.get_account_info().await.unwrap_err();
        assert!(err.to_string().contains("sequence"));
    }

    #[tokio::test]
    async fn resolve_gas_price_uses_network_max() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cosmos/base/node/v1beta1/config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "config": { "minimum_gas_price": "0.02uluna" }
            })))
            .mount(&server)
            .await;

        let signer = TerraSigner::new(TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "columbus-5".into(),
            mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: 0.015,
        })
        .unwrap();

        assert_eq!(signer.resolve_gas_price().await, 0.02);
    }

    #[tokio::test]
    async fn resolve_gas_price_probe_failure_uses_configured() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cosmos/base/node/v1beta1/config"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let signer = TerraSigner::new(TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "columbus-5".into(),
            mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: 0.015,
        })
        .unwrap();

        assert_eq!(signer.resolve_gas_price().await, 0.015);
    }

    #[tokio::test]
    async fn query_network_min_gas_price_wiremock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cosmos/base/node/v1beta1/config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "config": { "minimum_gas_price": "0.02uluna" }
            })))
            .mount(&server)
            .await;

        let signer = TerraSigner::new(TerraSignerConfig {
            lcd_url: server.uri(),
            chain_id: "columbus-5".into(),
            mnemonic: SecretString::new(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                    .to_string()
                    .into_boxed_str(),
            ),
            gas_limit: None,
            gas_price: DEFAULT_GAS_PRICE,
        })
        .unwrap();

        assert_eq!(signer.query_network_min_gas_price().await.unwrap(), Some(0.02));
    }
}
