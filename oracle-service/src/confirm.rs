//! Post-broadcast confirmation: DeliverTx wasm events, with LCD `State` as fallback.
//!
//! # Invariant
//!
//! **INV-ORACLE-LIVENESS-001** ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
//! [#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32), audit C-3): liveness success
//! may be recorded only after:
//! 1. the broadcast txhash is included on-chain with DeliverTx `code == 0`, **and**
//! 2. **either** wasm events on **that same tx** prove `action=update_rate` on
//!    `ORACLE_CONTRACT` with `rate` equal to this tick's proposed value, **or**
//!    (when structured wasm attrs are absent) retried oracle `State` shows
//!    `last_update_sec` advanced and `rate` equal to the proposed update.
//!
//! CheckTx / `BROADCAST_MODE_SYNC` acceptance alone is **not** success. Uncertain inclusion,
//! DeliverTx failure, hash mismatch, wrong contract, missing/wrong wasm attrs, or (when
//! events are stripped) state mismatch is fail-closed (no liveness record).
//!
//! LCD `State` lag after a proven wasm write is **observability only** — it must not undo
//! event-confirmed success (`oracle_state_lag_after_event_ok`).
//!
//! Crosslinks: [`crate::liveness`], [`crate::terra_tx`], `docs/DEPLOYMENT.md`,
//! `skills/oracle-liveness-confirm/SKILL.md`, `skills/oracle-ops-poll-silence/SKILL.md`.

use std::str::FromStr;

use cosmwasm_std::Uint128;
use eyre::{eyre, Result};
use ust1_oracle::msg::StateResponse;

use crate::terra_tx::TxEvent;

/// Bounded LCD `State` retries when wasm events are stripped (fallback path only).
pub const STATE_CONFIRM_RETRY_ATTEMPTS: u32 = 3;
/// Delay between State fallback retries.
pub const STATE_CONFIRM_RETRY_INTERVAL_MS: u64 = 100;

/// Result of inspecting DeliverTx wasm events for this operator's `UpdateRate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleTxEventMatch {
    /// Wasm `action=update_rate` on `ORACLE_CONTRACT` with `rate` == proposed.
    Matched,
    /// No structured wasm attributes to evaluate (stripped / unparseable LCD).
    /// Caller may fall back to retried `State`, then fail-closed.
    EventsAbsent,
}

/// Fail-closed check that on-chain oracle state reflects this tick's intended update.
///
/// Used as the **fallback** when DeliverTx wasm events are absent, and as an optional
/// consistency check after event success (lag must not fail-close — see callers).
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

/// Bind DeliverTx wasm events to **this** oracle contract and **this** tick's proposed rate.
///
/// Other contracts' wasm events in the same tx are ignored (confused-deputy / A10).
/// Events that exist but do not prove this write are `Err` (no State fallback).
/// An empty or non-wasm event list is [`OracleTxEventMatch::EventsAbsent`].
pub fn oracle_tx_events_match_update(
    contract: &str,
    proposed_rate: Uint128,
    events: &[TxEvent],
) -> Result<OracleTxEventMatch> {
    let groups = wasm_attr_groups(events);
    if groups.is_empty() {
        return Ok(OracleTxEventMatch::EventsAbsent);
    }

    let mut saw_our_contract = false;
    let mut saw_update_rate = false;
    let mut rate_mismatch = false;
    let mut rate_unparseable = false;

    for group in &groups {
        let Some(addr) = group
            .iter()
            .find(|(k, _)| k == "_contract_address")
            .map(|(_, v)| v.as_str())
        else {
            continue;
        };
        if !addr.eq_ignore_ascii_case(contract) {
            continue;
        }
        saw_our_contract = true;

        let actions: Vec<&str> = group
            .iter()
            .filter(|(k, _)| k == "action")
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
            .collect();
        if !actions.contains(&"update_rate") {
            continue;
        }
        saw_update_rate = true;

        let rates: Vec<&str> = group
            .iter()
            .filter(|(k, _)| k == "rate")
            .map(|(_, v)| v.as_str())
            .collect();
        if rates.is_empty() || rates.iter().any(|r| r.is_empty()) {
            rate_unparseable = true;
            continue;
        }
        let parsed: Result<Vec<Uint128>, _> = rates
            .iter()
            .map(|r| Uint128::from_str(r.trim()))
            .collect();
        let Ok(parsed) = parsed else {
            rate_unparseable = true;
            continue;
        };
        if parsed.iter().any(|r| *r != parsed[0]) {
            rate_unparseable = true;
            continue;
        }
        if parsed[0] == proposed_rate {
            return Ok(OracleTxEventMatch::Matched);
        }
        rate_mismatch = true;
    }

    if !saw_our_contract {
        return Err(eyre!(
            "DeliverTx wasm events did not include _contract_address={} (fail-closed)",
            contract
        ));
    }
    if !saw_update_rate {
        return Err(eyre!(
            "DeliverTx wasm events for oracle contract missing action=update_rate (fail-closed)"
        ));
    }
    if rate_mismatch {
        return Err(eyre!(
            "DeliverTx wasm event rate does not match proposed {} (fail-closed)",
            proposed_rate
        ));
    }
    if rate_unparseable {
        return Err(eyre!(
            "DeliverTx wasm event rate missing, empty, or unparseable (fail-closed)"
        ));
    }
    Err(eyre!(
        "DeliverTx wasm events did not prove oracle update_rate (fail-closed)"
    ))
}

/// Split wasm events into per-contract attribute groups (`_contract_address` starts a group).
fn wasm_attr_groups(events: &[TxEvent]) -> Vec<Vec<(String, String)>> {
    let mut groups = Vec::new();
    for ev in events {
        if !ev.ty.eq_ignore_ascii_case("wasm") {
            continue;
        }
        let mut current: Option<Vec<(String, String)>> = None;
        for attr in &ev.attributes {
            if attr.key == "_contract_address" {
                if let Some(g) = current.take() {
                    groups.push(g);
                }
                current = Some(vec![(attr.key.clone(), attr.value.clone())]);
            } else if let Some(g) = current.as_mut() {
                g.push((attr.key.clone(), attr.value.clone()));
            }
        }
        if let Some(g) = current {
            groups.push(g);
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terra_tx::{TxEvent, TxEventAttribute};

    fn state(rate: u128, last_update_sec: u64) -> StateResponse {
        StateResponse {
            rate: Uint128::new(rate),
            last_update_sec,
            utc_day_id: 1,
            day_baseline_rate: Uint128::new(rate),
            paused: false,
        }
    }

    fn attr(key: &str, value: &str) -> TxEventAttribute {
        TxEventAttribute {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    fn wasm_event(attrs: Vec<TxEventAttribute>) -> TxEvent {
        TxEvent {
            ty: "wasm".into(),
            attributes: attrs,
        }
    }

    const ORACLE: &str = "terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n";
    const OTHER: &str = "terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2";

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

    #[test]
    fn events_absent_when_empty() {
        assert_eq!(
            oracle_tx_events_match_update(ORACLE, Uint128::new(1), &[]).unwrap(),
            OracleTxEventMatch::EventsAbsent
        );
    }

    #[test]
    fn events_absent_when_only_non_wasm() {
        let events = [TxEvent {
            ty: "message".into(),
            attributes: vec![attr("action", "update_rate"), attr("rate", "1")],
        }];
        assert_eq!(
            oracle_tx_events_match_update(ORACLE, Uint128::new(1), &events).unwrap(),
            OracleTxEventMatch::EventsAbsent
        );
    }

    #[test]
    fn events_match_update_rate_on_oracle_contract() {
        let events = [wasm_event(vec![
            attr("_contract_address", ORACLE),
            attr("action", "update_rate"),
            attr("rate", "1225553374561294951"),
        ])];
        assert_eq!(
            oracle_tx_events_match_update(ORACLE, Uint128::new(1_225_553_374_561_294_951), &events)
                .unwrap(),
            OracleTxEventMatch::Matched
        );
    }

    #[test]
    fn events_ignore_other_contract_in_same_tx() {
        let events = [wasm_event(vec![
            attr("_contract_address", OTHER),
            attr("action", "update_rate"),
            attr("rate", "99"),
            attr("_contract_address", ORACLE),
            attr("action", "update_rate"),
            attr("rate", "2000"),
        ])];
        assert_eq!(
            oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap(),
            OracleTxEventMatch::Matched
        );
    }

    #[test]
    fn events_wrong_contract_is_error_even_if_rate_matches() {
        let events = [wasm_event(vec![
            attr("_contract_address", OTHER),
            attr("action", "update_rate"),
            attr("rate", "2000"),
        ])];
        let err = oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap_err();
        assert!(
            err.to_string().contains("_contract_address"),
            "{err}"
        );
    }

    #[test]
    fn events_missing_action_is_error() {
        let events = [wasm_event(vec![
            attr("_contract_address", ORACLE),
            attr("rate", "2000"),
        ])];
        let err = oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap_err();
        assert!(err.to_string().contains("action=update_rate"), "{err}");
    }

    #[test]
    fn events_rate_mismatch_is_error() {
        let events = [wasm_event(vec![
            attr("_contract_address", ORACLE),
            attr("action", "update_rate"),
            attr("rate", "1999"),
        ])];
        let err = oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap_err();
        assert!(err.to_string().contains("rate does not match"), "{err}");
    }

    #[test]
    fn events_empty_rate_is_error() {
        let events = [wasm_event(vec![
            attr("_contract_address", ORACLE),
            attr("action", "update_rate"),
            attr("rate", ""),
        ])];
        let err = oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap_err();
        assert!(err.to_string().contains("unparseable") || err.to_string().contains("missing"), "{err}");
    }

    #[test]
    fn events_ambiguous_conflicting_rates_in_same_group_is_error() {
        let events = [wasm_event(vec![
            attr("_contract_address", ORACLE),
            attr("action", "update_rate"),
            attr("rate", "2000"),
            attr("rate", "1999"),
        ])];
        let err = oracle_tx_events_match_update(ORACLE, Uint128::new(2_000), &events).unwrap_err();
        assert!(
            err.to_string().contains("unparseable") || err.to_string().contains("fail-closed"),
            "{err}"
        );
    }

    #[test]
    fn events_contract_match_is_case_insensitive() {
        let upper = ORACLE.to_ascii_uppercase();
        let events = [wasm_event(vec![
            attr("_contract_address", &upper),
            attr("action", "update_rate"),
            attr("rate", "7"),
        ])];
        assert_eq!(
            oracle_tx_events_match_update(ORACLE, Uint128::new(7), &events).unwrap(),
            OracleTxEventMatch::Matched
        );
    }
}
