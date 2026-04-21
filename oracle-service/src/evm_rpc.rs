//! BSC RPC URL parsing and multi-provider consensus for `exchangeRateStored` reads.
//!
//! Every tick queries at least two endpoints and accepts a value only when two successful
//! responses agree within **0.01%** relative (`|a−b| / max(a,b) ≤ 0.0001`).

use cosmwasm_std::Uint128;
use eyre::{eyre, Result};
use std::future::Future;

pub fn parse_comma_separated_rpc_urls(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Relative agreement: `|a − b| / max(a, b) ≤ 0.01%` (i.e. ≤ 1/10 000 of the larger value).
pub fn rates_agree_within_point_zero_one_percent(a: Uint128, b: Uint128) -> bool {
    let a = a.u128();
    let b = b.u128();
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    if hi == 0 {
        return lo == 0;
    }
    let diff = hi - lo;
    match diff.checked_mul(10_000) {
        Some(x) => x <= hi,
        None => false,
    }
}

/// Reads the rate from `urls[0]` and `urls[1]` in parallel. If both succeed and agree within
/// [`rates_agree_within_point_zero_one_percent`], returns the first endpoint’s value.
///
/// If the first two disagree and a third URL exists, queries it once and accepts the rate when
/// it matches one of the first two within tolerance. If fewer than two URLs succeed, or no two
/// agree, returns an error.
pub async fn run_with_evm_rpc_rate_consensus<F, Fut>(urls: &[String], read: F) -> Result<Uint128>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<Uint128>>,
{
    if urls.len() < 2 {
        return Err(eyre!(
            "BSC RPC consensus requires at least two URLs in BSC_RPC_URLS"
        ));
    }

    let u0 = urls[0].clone();
    let u1 = urls[1].clone();
    let (first, second) = tokio::join!(read(u0), read(u1));

    match (first, second) {
        (Ok(a), Ok(b)) => {
            if rates_agree_within_point_zero_one_percent(a, b) {
                if a != b {
                    tracing::info!(
                        rate_a = %a,
                        rate_b = %b,
                        "EVM RPC consensus: two providers agree within 0.01%"
                    );
                }
                return Ok(a);
            }
            if urls.len() < 3 {
                return Err(eyre!(
                    "EVM RPC mismatch: first two providers disagree beyond 0.01% ({} vs {}); configure a third URL to break ties",
                    a, b
                ));
            }
            let c = read(urls[2].clone()).await?;
            if rates_agree_within_point_zero_one_percent(a, c) {
                tracing::warn!(
                    rate_first = %a,
                    rate_second = %b,
                    rate_third = %c,
                    "EVM RPC consensus: third provider agreed with first"
                );
                return Ok(a);
            }
            if rates_agree_within_point_zero_one_percent(b, c) {
                tracing::warn!(
                    rate_first = %a,
                    rate_second = %b,
                    rate_third = %c,
                    "EVM RPC consensus: third provider agreed with second"
                );
                return Ok(b);
            }
            Err(eyre!(
                "EVM RPC mismatch: no two of three providers agree within 0.01% ({} vs {} vs {})",
                a,
                b,
                c
            ))
        }
        (Ok(a), Err(e1)) => {
            if urls.len() < 3 {
                return Err(e1.wrap_err(format!(
                    "EVM RPC: only one successful provider (need 2 agreeing within 0.01%); first rate was {}",
                    a
                )));
            }
            tracing::warn!(error = %e1, "EVM RPC second endpoint failed; trying third");
            match read(urls[2].clone()).await {
                Ok(c) if rates_agree_within_point_zero_one_percent(a, c) => Ok(a),
                Ok(c) => Err(eyre!(
                    "EVM RPC: first and third providers disagree beyond 0.01% ({} vs {})",
                    a,
                    c
                )),
                Err(e2) => Err(e2.wrap_err(format!(
                    "EVM RPC: first succeeded ({}) but second and third failed",
                    a
                ))),
            }
        }
        (Err(e0), Ok(b)) => {
            if urls.len() < 3 {
                return Err(e0.wrap_err(format!(
                    "EVM RPC: only one successful provider (need 2 agreeing within 0.01%); second rate was {}",
                    b
                )));
            }
            tracing::warn!(error = %e0, "EVM RPC first endpoint failed; trying third");
            match read(urls[2].clone()).await {
                Ok(c) if rates_agree_within_point_zero_one_percent(b, c) => Ok(b),
                Ok(c) => Err(eyre!(
                    "EVM RPC: second and third providers disagree beyond 0.01% ({} vs {})",
                    b,
                    c
                )),
                Err(e2) => Err(e2.wrap_err(format!(
                    "EVM RPC: second succeeded ({}) but first and third failed",
                    b
                ))),
            }
        }
        (Err(e0), Err(e1)) => {
            if urls.len() < 3 {
                return Err(e0.wrap_err(format!("EVM RPC: both first endpoints failed ({})", e1)));
            }
            tracing::warn!(error = %e0, "EVM RPC first two endpoints failed; trying third");
            match read(urls[2].clone()).await {
                Ok(_) => Err(eyre!(
                    "EVM RPC: need two successful providers agreeing within 0.01%; only the third endpoint responded"
                )),
                Err(e2) => Err(e2.wrap_err(format!(
                    "EVM RPC: all of first three endpoints failed ({})",
                    e1
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_rates_agree() {
        let x = Uint128::from(1_000_000u128);
        assert!(rates_agree_within_point_zero_one_percent(x, x));
    }

    #[test]
    fn zero_pair() {
        assert!(rates_agree_within_point_zero_one_percent(
            Uint128::zero(),
            Uint128::zero()
        ));
        assert!(!rates_agree_within_point_zero_one_percent(
            Uint128::zero(),
            Uint128::from(1u128)
        ));
    }

    #[test]
    fn within_point_zero_one_percent() {
        // 0.01% of 1_000_000 = 100; diff 100 should pass
        let a = Uint128::from(1_000_000u128);
        let b = Uint128::from(1_000_100u128);
        assert!(rates_agree_within_point_zero_one_percent(a, b));
        // diff 101 should fail
        let c = Uint128::from(1_000_101u128);
        assert!(!rates_agree_within_point_zero_one_percent(a, c));
    }

    #[test]
    fn symmetric() {
        let a = Uint128::from(100u128);
        let b = Uint128::from(100u128);
        assert!(rates_agree_within_point_zero_one_percent(a, b));
        assert!(rates_agree_within_point_zero_one_percent(b, a));
    }
}
