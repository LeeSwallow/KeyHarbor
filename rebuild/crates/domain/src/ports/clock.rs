//! Time: the one place use cases learn what time it is and wait between polls. The binary's
//! composition root plugs in the system clock; tests use [`ManualClock`].

use crate::date::Date;
use std::{sync::Mutex, time::Duration};

pub trait Clock: Send + Sync {
    /// Time since the Unix epoch (UTC).
    fn now(&self) -> Duration;
    /// Waits `duration` before a use case polls again.
    fn sleep(&self, duration: Duration);

    /// Whole seconds since the Unix epoch.
    fn unix_seconds(&self) -> u64 {
        self.now().as_secs()
    }
    /// Today (UTC), for expiry (R-37).
    fn today(&self) -> Date {
        Date::from_unix_seconds(self.unix_seconds())
    }
    /// Now as RFC 3339 UTC with seconds, e.g. `2026-10-28T12:34:56Z`.
    fn now_rfc3339(&self) -> String {
        let secs = self.unix_seconds();
        let rem = secs % 86_400;
        format!(
            "{}T{:02}:{:02}:{:02}Z",
            Date::from_unix_seconds(secs),
            rem / 3600,
            rem % 3600 / 60,
            rem % 60
        )
    }
}

/// A clock that moves only when told: [`Clock::sleep`] and [`ManualClock::advance`] add to it.
/// Tests use it so waits finish at once and dates are fixed.
#[derive(Debug)]
pub struct ManualClock(Mutex<Duration>);

impl ManualClock {
    pub fn at_unix_seconds(seconds: u64) -> Self {
        Self(Mutex::new(Duration::from_secs(seconds)))
    }
    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn sleep(&self, duration: Duration) {
        self.advance(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_told() {
        let clock = ManualClock::at_unix_seconds(1_793_203_129);
        assert_eq!(clock.today().to_string(), "2026-10-28");
        assert_eq!(clock.now_rfc3339(), "2026-10-28T15:58:49Z");
        clock.sleep(Duration::from_secs(3600 * 9));
        assert_eq!(clock.today().to_string(), "2026-10-29");
    }
}
