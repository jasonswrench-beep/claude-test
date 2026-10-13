// SPDX-License-Identifier: Apache-2.0
use std::cell::Cell;
use std::thread;
use std::time::{Duration, Instant};

/// Time, as the session sees it.
pub trait Clock {
    /// Milliseconds since some fixed starting point.
    fn now_ms(&self) -> u64;
    /// Wait for this many milliseconds.
    fn sleep_ms(&self, ms: u64);
}

/// The real clock.
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    fn sleep_ms(&self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
}

/// A fake clock for tests: sleeping moves time forward instantly.
#[derive(Default)]
pub struct ManualClock {
    now: Cell<u64>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }

    fn sleep_ms(&self, ms: u64) {
        self.now.set(self.now.get() + ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manual_clock_moves_only_when_it_sleeps() {
        let clock = ManualClock::new();
        assert_eq!(clock.now_ms(), 0);
        clock.sleep_ms(250);
        clock.sleep_ms(250);
        assert_eq!(clock.now_ms(), 500);
    }

    #[test]
    fn the_system_clock_moves_forward() {
        let clock = SystemClock::new();
        let before = clock.now_ms();
        clock.sleep_ms(5);
        assert!(clock.now_ms() >= before + 4);
    }
}
