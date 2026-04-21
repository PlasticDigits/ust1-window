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
    pub mnemonic: String,
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

impl TerraSigner {
    pub fn new(config: TerraSignerConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .wrap_err("HTTP client")?;
        let mnemonic = Mnemonic::parse(&config.mnemonic).map_err(|e| eyre!("mnemonic: {}", e))?;
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
