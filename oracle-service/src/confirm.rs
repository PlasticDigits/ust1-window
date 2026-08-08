//! Post-broadcast confirmation: DeliverTx inclusion + oracle `State` reflection.
//!
//! # Invariant
//!
//! **INV-ORACLE-LIVENESS-001** ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
//! audit C-3): liveness success may be recorded only after:
//! 1. the broadcast txhash is included on-chain with DeliverTx `code == 0`, and
//! 2. oracle `State` shows `last_update_sec` advanced and `rate` equal to the proposed update.
//!
//! CheckTx / `BROADCAST_MODE_SYNC` acceptance alone is **not** success. Uncertain inclusion
//! or state mismatch is fail-closed (no liveness record).
//!
//! Crosslinks: [`crate::liveness`], [`crate::terra_tx`], `docs/DEPLOYMENT.md`,
//! `skills/oracle-liveness-confirm/SKILL.md`.

use cosmwasm_std::Uint128;
use eyre::{eyre, Result};
use ust1_oracle::msg::StateResponse;

/// Fail-closed check that on-chain oracle state reflects this tick's intended update.
///
/// Another operator racing with a different rate, a no-op, or a wrong contract address
/// must not count as success for this tick (INV-ORACLE-LIVENESS-001).
pub fn oracle_state_matches_intended_update(
    prior_last_update_sec: u64,
    state: &StateResponse,
    proposed_rate: Uint128,
) -> Result<()> {
    if state.rate != proposed_rate {
        return Err(eyre!(
            "oracle state rate mismatch after inclusion: expected {}, got {} (fail-closed)",
            proposed_rate,
            state.rate
        ));
    }
    if state.last_update_sec <= prior_last_update_sec {
        return Err(eyre!(
            "oracle last_update_sec did not advance after inclusion: prior {}, got {} (fail-closed)",
            prior_last_update_sec,
            state.last_update_sec
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(rate: u128, last_update_sec: u64) -> StateResponse {
        StateResponse {
            rate: Uint128::new(rate),
            last_update_sec,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(rate),
        }
    }

    #[test]
    fn accepts_advanced_update_with_matching_rate() {
        oracle_state_matches_intended_update(100, &state(2_000, 200), Uint128::new(2_000)).unwrap();
    }

    #[test]
    fn rejects_rate_mismatch_even_if_timestamp_advanced() {
        let err =
            oracle_state_matches_intended_update(100, &state(1_999, 200), Uint128::new(2_000))
                .unwrap_err();
        assert!(err.to_string().contains("rate mismatch"), "{err}");
    }

    #[test]
    fn rejects_unchanged_last_update_sec() {
        let err =
            oracle_state_matches_intended_update(100, &state(2_000, 100), Uint128::new(2_000))
                .unwrap_err();
        assert!(err.to_string().contains("did not advance"), "{err}");
    }

    #[test]
    fn rejects_stale_last_update_sec() {
        let err = oracle_state_matches_intended_update(100, &state(2_000, 50), Uint128::new(2_000))
            .unwrap_err();
        assert!(err.to_string().contains("did not advance"), "{err}");
    }
}
