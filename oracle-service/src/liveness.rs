//! In-process liveness tracking for successful Terra broadcasts (no Prometheus).
//!
//! Silence currently keys off [`LivenessTracker::record_successful_broadcast`], which today is
//! called after a SYNC broadcast accepts (CheckTx). **C-3 / glab #23** will move that to
//! confirmed DeliverTx + on-chain state before counting as success. Timing defaults for when
//! to alert are **INV-ORACLE-OPS-SILENCE-001** (H-3 / glab #24) in `config.rs`.

use std::time::{Duration, Instant};

/// Tracks time since the last successful on-chain broadcast.
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

    pub fn record_successful_broadcast(&mut self) {
        self.last_successful_broadcast = Some(Instant::now());
    }

    /// Duration since the last successful broadcast, or since process start if none yet.
    pub fn silence_since_last_broadcast(&self) -> Duration {
        match self.last_successful_broadcast {
            Some(t) => t.elapsed(),
            None => self.started.elapsed(),
        }
    }

    pub fn should_alert(&self, max_silence: Duration) -> bool {
        self.silence_since_last_broadcast() > max_silence
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
}
