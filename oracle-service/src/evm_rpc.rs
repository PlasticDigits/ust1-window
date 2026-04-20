//! Comma-separated BSC RPC URLs with sequential fallback.
//! Pattern adapted from `cl8y-bridge-monorepo` `multichain-rs` `evm/rpc_fallback.rs`.

use eyre::{eyre, Result};
use std::future::Future;

pub fn parse_comma_separated_rpc_urls(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub async fn run_with_evm_rpc_url_fallback<T, F, Fut>(urls: &[String], mut op: F) -> Result<T>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if urls.is_empty() {
        return Err(eyre!("no EVM RPC URLs configured"));
    }
    let mut last_err: Option<eyre::Report> = None;
    for (i, url) in urls.iter().enumerate() {
        match op(url.clone()).await {
            Ok(v) => {
                if i > 0 {
                    tracing::info!(rpc_index = i, "EVM RPC fallback endpoint succeeded");
                }
                return Ok(v);
            }
            Err(e) => {
                if urls.len() > 1 {
                    tracing::warn!(rpc_index = i, error = %e, "EVM RPC attempt failed");
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.expect("non-empty urls implies at least one attempt"))
}
