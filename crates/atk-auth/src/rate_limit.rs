use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A simple in-memory fixed-window rate limiter, keyed by an arbitrary string (e.g. a client IP,
/// or an IP+user compound key). `max_tracked_keys` bounds how many distinct keys are ever held at
/// once: `usize::MAX` (the login-flow limiter's setting, via `new`) means "never evict," fine for
/// a key set bounded by how many distinct clients have ever attempted a login, which stays small
/// in practice. A limiter guarding every request on an internet-facing tunnel doesn't get that
/// guarantee -- an attacker sprayed across many source IPs could otherwise grow the map without
/// bound -- so `with_capacity_bound` opportunistically clears the whole map once it's over the
/// limit rather than tracking per-entry recency, trading perfect fairness for a hard memory
/// ceiling under attack load.
pub struct RateLimiter {
    max_attempts: u32,
    window: Duration,
    max_tracked_keys: usize,
    state: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self::with_capacity_bound(max_attempts, window, usize::MAX)
    }

    pub fn with_capacity_bound(max_attempts: u32, window: Duration, max_tracked_keys: usize) -> Self {
        Self { max_attempts, window, max_tracked_keys, state: Mutex::new(HashMap::new()) }
    }

    /// Records an attempt for `key` and reports whether it's still within the allowed
    /// rate. The window resets the first time `check` is called after it has elapsed.
    pub fn check(&self, key: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        if !state.contains_key(key) && state.len() >= self.max_tracked_keys {
            // Correctness only needs "eventually forgets stale keys under sustained pressure,"
            // not perfect fairness -- an outright clear is enough to keep memory bounded without
            // the cost of tracking per-entry last-seen recency.
            state.clear();
        }

        let entry = state.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) > self.window {
            *entry = (0, now);
        }
        if entry.0 >= self.max_attempts {
            return false;
        }
        entry.0 += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_attempts_up_to_the_limit_then_blocks() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"), "a fourth attempt within the window must be blocked");
    }

    #[test]
    fn tracks_each_key_independently() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"));
        assert!(limiter.check("5.6.7.8"), "a different key must have its own allowance");
    }

    #[test]
    fn resets_once_the_window_elapses() {
        let limiter = RateLimiter::new(1, Duration::from_millis(20));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"));
        std::thread::sleep(Duration::from_millis(30));
        assert!(limiter.check("1.2.3.4"), "a new window must grant a fresh allowance");
    }
}
