//! In-process liveness tracking for **confirmed** Terra oracle updates (no Prometheus).
//!
//! # Invariants
//!
//! **INV-ORACLE-LIVENESS-001** ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
//! audit C-3): [`LivenessTracker::record_successful_broadcast`] may be called only after
//! DeliverTx `code == 0` **and** oracle `State` reflects the intended update. See
//! [`crate::confirm`] and [`crate::terra_tx::TerraSigner::wait_for_deliver_tx_success`].
//! Operator skill: `skills/oracle-liveness-confirm/SKILL.md`.
//!
//! Silence alerts (`ORACLE_MAX_SILENCE_SECS`) therefore mean “no confirmed on-chain update”,
//! not “no mempool CheckTx acceptance”. Timing defaults are **INV-ORACLE-OPS-SILENCE-001**
//! (H-3 / glab #24) in `config.rs`.

use std::time::{Duration, Instant};

/// Tracks time since the last confirmed on-chain oracle update (INV-ORACLE-LIVENESS-001).
#[derive(Debug)]
pub struct LivenessTracker {
    started: Instant,
    last_successful_broadcast: Option<Instant>,
}

impl LivenessTracker {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            last_successful_broadcast: None,
        }
    }

    /// Record a **confirmed** oracle update (DeliverTx + matching `State` only).
    pub fn record_successful_broadcast(&mut self) {
        self.last_successful_broadcast = Some(Instant::now());
    }

    /// Duration since the last confirmed update, or since process start if none yet.
    pub fn silence_since_last_broadcast(&self) -> Duration {
        match self.last_successful_broadcast {
            Some(t) => t.elapsed(),
            None => self.started.elapsed(),
        }
    }

    pub fn should_alert(&self, max_silence: Duration) -> bool {
        self.silence_since_last_broadcast() > max_silence
    }

    /// Test helper: whether any confirmed success has been recorded.
    #[cfg(test)]
    pub fn has_recorded_success(&self) -> bool {
        self.last_successful_broadcast.is_some()
    }
}

impl Default for LivenessTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_ORACLE_MAX_SILENCE_SECS;

    #[test]
    fn fresh_tracker_does_not_alert_at_default_silence() {
        let t = LivenessTracker::new();
        assert!(!t.should_alert(Duration::from_secs(DEFAULT_ORACLE_MAX_SILENCE_SECS)));
    }

    #[test]
    fn should_alert_when_silence_exceeds_threshold() {
        let t = LivenessTracker::new();
        std::thread::sleep(Duration::from_millis(5));
        assert!(t.should_alert(Duration::ZERO));
        assert!(!t.should_alert(Duration::from_secs(DEFAULT_ORACLE_MAX_SILENCE_SECS)));
    }

    #[test]
    fn record_successful_broadcast_resets_silence_for_alert() {
        let mut t = LivenessTracker::new();
        std::thread::sleep(Duration::from_millis(5));
        assert!(t.should_alert(Duration::ZERO));
        t.record_successful_broadcast();
        assert!(!t.should_alert(Duration::from_millis(50)));
    }

    #[test]
    fn default_silence_threshold_is_six_hours_not_eight() {
        // Guards against regressing to the pre-H-3 28800s default at the alert boundary.
        assert_eq!(DEFAULT_ORACLE_MAX_SILENCE_SECS, 21_600);
        let t = LivenessTracker::new();
        assert!(!t.should_alert(Duration::from_secs(21_600)));
    }

    #[test]
    fn no_success_until_recorded() {
        let t = LivenessTracker::new();
        assert!(!t.has_recorded_success());
        assert!(t.should_alert(Duration::from_secs(0)));
    }

    #[test]
    fn record_clears_alert_for_large_threshold() {
        let mut t = LivenessTracker::new();
        t.record_successful_broadcast();
        assert!(t.has_recorded_success());
        assert!(!t.should_alert(Duration::from_secs(3600)));
    }
}
