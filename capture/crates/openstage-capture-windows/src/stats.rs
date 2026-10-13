// SPDX-License-Identifier: Apache-2.0
//! How steadily pictures arrived. Used by the screen test and, later, by the recorder to warn
//! when capture cannot keep up.

/// Collects the time each picture arrived (milliseconds from the first one).
#[derive(Debug, Default, Clone)]
pub struct FrameStats {
    times: Vec<u64>,
}

/// What the arrival times add up to.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSummary {
    pub frames: u32,
    /// From the first picture to the last.
    pub span_ms: u64,
    /// Pictures per second between the first and the last. 0 if fewer than two arrived.
    pub fps: f64,
    /// The longest wait between two pictures.
    pub longest_gap_ms: u64,
}

impl FrameStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note that a picture arrived `t_ms` milliseconds after the first one.
    pub fn add(&mut self, t_ms: u64) {
        self.times.push(t_ms);
    }

    pub fn summary(&self) -> FrameSummary {
        let frames = self.times.len() as u32;
        let (Some(&first), Some(&last)) = (self.times.first(), self.times.last()) else {
            return FrameSummary { frames: 0, span_ms: 0, fps: 0.0, longest_gap_ms: 0 };
        };
        let span_ms = last.saturating_sub(first);
        let fps = if frames >= 2 && span_ms > 0 { (frames - 1) as f64 * 1000.0 / span_ms as f64 } else { 0.0 };
        let longest_gap_ms = self.times.windows(2).map(|w| w[1].saturating_sub(w[0])).max().unwrap_or(0);
        FrameSummary { frames, span_ms, fps, longest_gap_ms }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_arriving_is_zero_not_a_crash() {
        let s = FrameStats::new().summary();
        assert_eq!((s.frames, s.span_ms, s.longest_gap_ms), (0, 0, 0));
        assert_eq!(s.fps, 0.0);
    }

    #[test]
    fn one_picture_has_no_rate() {
        let mut stats = FrameStats::new();
        stats.add(0);
        let s = stats.summary();
        assert_eq!(s.frames, 1);
        assert_eq!(s.fps, 0.0);
    }

    #[test]
    fn thirty_pictures_a_second_is_reported_as_thirty() {
        let mut stats = FrameStats::new();
        for i in 0..=90u64 {
            stats.add(i * 1000 / 30);
        }
        let s = stats.summary();
        assert_eq!(s.frames, 91);
        assert!((s.fps - 30.0).abs() < 0.2, "fps was {}", s.fps);
        assert!(s.longest_gap_ms <= 34);
    }

    #[test]
    fn the_longest_wait_is_found() {
        let mut stats = FrameStats::new();
        for t in [0, 33, 66, 566, 600] {
            stats.add(t);
        }
        assert_eq!(stats.summary().longest_gap_ms, 500);
    }

    #[test]
    fn times_that_go_backwards_do_not_overflow() {
        let mut stats = FrameStats::new();
        for t in [100, 50, 80] {
            stats.add(t);
        }
        let s = stats.summary();
        assert_eq!(s.span_ms, 0);
        assert_eq!(s.longest_gap_ms, 30);
    }
}
