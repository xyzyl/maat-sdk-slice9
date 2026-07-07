//! Clock abstraction.
//!
//! [`Clock`] allows tests to inject deterministic time. Production code uses
//! [`SystemClock`]; tests use a fixed clock that returns a stable timestamp.
//!
//! This mirrors the protocol library's pattern where
//! `verify_action_request(req, now)` takes the current time as a parameter
//! rather than reading the wall clock — except here it's a trait so that
//! resource-side validators can be injected with a clock during testing
//! without changing their public API.

use std::time::{SystemTime, UNIX_EPOCH};

/// Source of the current time as Unix seconds.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

/// Wall-clock time. The default in production.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_secs()
    }
}

/// A clock that always returns the same timestamp. For tests.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_recent_time() {
        let now = SystemClock.now();
        // Sanity check: must be after some date in 2020.
        assert!(now > 1_577_836_800);
    }

    #[test]
    fn fixed_clock_is_stable() {
        let c = FixedClock(1_700_000_000);
        assert_eq!(c.now(), 1_700_000_000);
        assert_eq!(c.now(), 1_700_000_000);
    }
}
