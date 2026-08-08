//! In-process liveness tracking for successful Terra broadcasts (no Prometheus).

use std::sync::{Arc, Mutex};
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

/// Recover from a poisoned liveness mutex instead of panicking (L-9).
pub fn lock_liveness(
    mutex: &Mutex<LivenessTracker>,
) -> std::sync::MutexGuard<'_, LivenessTracker> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Convenience for `Arc<Mutex<LivenessTracker>>` call sites.
pub fn lock_liveness_arc(
    mutex: &Arc<Mutex<LivenessTracker>>,
) -> std::sync::MutexGuard<'_, LivenessTracker> {
    lock_liveness(mutex.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn poisoned_mutex_recovers_via_into_inner() {
        let mutex = Arc::new(Mutex::new(LivenessTracker::new()));
        let mutex2 = Arc::clone(&mutex);
        let join = thread::spawn(move || {
            let _guard = mutex2.lock().unwrap();
            panic!("force poison");
        });
        assert!(join.join().is_err());
        assert!(mutex.is_poisoned());
        let guard = lock_liveness_arc(&mutex);
        assert!(guard.silence_since_last_broadcast() < Duration::from_secs(5));
    }

    #[test]
    fn should_alert_after_silence() {
        let tracker = LivenessTracker::new();
        assert!(!tracker.should_alert(Duration::from_secs(3600)));
    }
}
