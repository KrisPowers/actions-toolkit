use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A simple in-memory fixed-window rate limiter, keyed by an arbitrary string (e.g. a
/// client IP). Sized for a self-hosted, single-instance tool: the key set is bounded by
/// how many distinct clients have ever attempted a login, which stays small in practice,
/// so this deliberately skips a background sweep of stale entries.
pub struct RateLimiter {
    max_attempts: u32,
    window: Duration,
    state: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self { max_attempts, window, state: Mutex::new(HashMap::new()) }
    }

    /// Records an attempt for `key` and reports whether it's still within the allowed
    /// rate. The window resets the first time `check` is called after it has elapsed.
    pub fn check(&self, key: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
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
