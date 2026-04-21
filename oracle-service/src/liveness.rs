//! In-process liveness tracking for successful Terra broadcasts (no Prometheus).

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
