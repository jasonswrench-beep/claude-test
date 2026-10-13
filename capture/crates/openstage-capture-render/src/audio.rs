// SPDX-License-Identifier: Apache-2.0
//! Mixing the sound of an edit.
//!
//! Each piece of sound in the edit becomes an [`AudioSpan`]: which file, which part of it, where in
//! the finished video it sits, how loud, and how it fades. [`mix_block`] adds up everything that
//! plays during a stretch of time. The caller supplies the sound itself (the recorder's `.wav`
//! files are 48,000 samples a second, stereo, 16 bits); this file does only the arithmetic.

use openstage_capture_project::EditSourceRef;

/// Interleaved stereo samples: left, right, left, right ...
pub type StereoFrames = Vec<i16>;

/// One piece of sound placed in the finished video.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSpan {
    pub clip_id: String,
    pub source: EditSourceRef,
    /// Where it sits in the finished video, in milliseconds.
    pub out_start_ms: u64,
    pub out_end_ms: u64,
    /// Where in the source file it starts, in milliseconds.
    pub source_start_ms: u64,
    /// How much louder or quieter, in whole decibels; muted counts as silent.
    pub volume_db: i32,
    pub muted: bool,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
}

impl AudioSpan {
    pub fn length_ms(&self) -> u64 {
        self.out_end_ms - self.out_start_ms
    }
}

/// How loud a span is at a moment, as a number to multiply the sound by: 0 is silence, 1 is as recorded.
/// The fades are straight lines. A moment outside the span, or a muted span, is silent.
pub fn gain_at(span: &AudioSpan, out_us: u64) -> f32 {
    let (start, end) = (span.out_start_ms * 1000, span.out_end_ms * 1000);
    if span.muted || out_us < start || out_us >= end {
        return 0.0;
    }
    let mut gain = 10f32.powf(span.volume_db as f32 / 20.0);
    let into = out_us - start;
    let before_end = end - out_us;
    let fade_in = span.fade_in_ms * 1000;
    let fade_out = span.fade_out_ms * 1000;
    if fade_in > 0 && into < fade_in {
        gain *= into as f32 / fade_in as f32;
    }
    if fade_out > 0 && before_end < fade_out {
        gain *= before_end as f32 / fade_out as f32;
    }
    gain
}

/// The sound of everything that plays from sample `start_sample` for `len` frames of the finished
/// video, at `rate` samples a second. `read` is asked for the source's sound when needed: it is given
/// the span and the first frame and number of frames wanted from the source file, and returns them
/// interleaved (fewer, if the file ends sooner; the rest is silence). The mix saturates instead of
/// wrapping, so a loud sum is clipped, never turned into noise.
pub fn mix_block(
    spans: &[AudioSpan],
    start_sample: u64,
    len: usize,
    rate: u32,
    read: &mut dyn FnMut(&AudioSpan, u64, usize) -> StereoFrames,
) -> StereoFrames {
    let mut sum = vec![0i32; len * 2];
    let rate = rate as u64;
    for span in spans {
        let first = span.out_start_ms * rate / 1000;
        let last = span.out_end_ms * rate / 1000;
        let from = start_sample.max(first);
        let to = (start_sample + len as u64).min(last);
        if from >= to {
            continue;
        }
        let frames = (to - from) as usize;
        let source_first = span.source_start_ms * rate / 1000 + (from - first);
        let heard = read(span, source_first, frames);
        let offset = (from - start_sample) as usize;
        for i in 0..frames.min(heard.len() / 2) {
            let out_us = (from + i as u64) * 1_000_000 / rate;
            let gain = gain_at(span, out_us);
            if gain == 0.0 {
                continue;
            }
            for channel in 0..2 {
                let v = heard[i * 2 + channel] as f32 * gain;
                sum[(offset + i) * 2 + channel] += v.round() as i32;
            }
        }
    }
    sum.into_iter().map(|v| v.clamp(i16::MIN as i32, i16::MAX as i32) as i16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start_ms: u64, end_ms: u64) -> AudioSpan {
        AudioSpan {
            clip_id: "c".into(),
            source: EditSourceRef::Take { take: 1, media: "mic.wav".into() },
            out_start_ms: start_ms,
            out_end_ms: end_ms,
            source_start_ms: 0,
            volume_db: 0,
            muted: false,
            fade_in_ms: 0,
            fade_out_ms: 0,
        }
    }

    /// A source that is a steady `value` on both channels.
    fn steady(value: i16) -> impl FnMut(&AudioSpan, u64, usize) -> StereoFrames {
        move |_, _, frames| vec![value; frames * 2]
    }

    #[test]
    fn gain_is_one_as_recorded_and_follows_the_decibels() {
        let mut s = span(0, 1000);
        assert_eq!(gain_at(&s, 500_000), 1.0);
        s.volume_db = -6;
        assert!((gain_at(&s, 500_000) - 0.501).abs() < 0.001);
        s.volume_db = 6;
        assert!((gain_at(&s, 500_000) - 1.995).abs() < 0.001);
        s.volume_db = -40;
        assert!((gain_at(&s, 500_000) - 0.01).abs() < 0.0001);
    }

    #[test]
    fn a_muted_span_or_a_moment_outside_it_is_silent() {
        let mut s = span(1000, 2000);
        assert_eq!(gain_at(&s, 999_999), 0.0);
        assert_eq!(gain_at(&s, 2_000_000), 0.0);
        assert_eq!(gain_at(&s, 1_500_000), 1.0);
        s.muted = true;
        assert_eq!(gain_at(&s, 1_500_000), 0.0);
    }

    #[test]
    fn fades_are_straight_lines_from_and_to_silence() {
        let mut s = span(0, 10_000);
        s.fade_in_ms = 1000;
        s.fade_out_ms = 2000;
        assert_eq!(gain_at(&s, 0), 0.0);
        assert!((gain_at(&s, 500_000) - 0.5).abs() < 1e-6);
        assert_eq!(gain_at(&s, 1_000_000), 1.0);
        assert_eq!(gain_at(&s, 5_000_000), 1.0);
        assert!((gain_at(&s, 9_000_000) - 0.5).abs() < 1e-6, "one second before the end of a two second fade");
        assert!(gain_at(&s, 9_999_000) < 0.001);
    }

    #[test]
    fn a_block_is_the_sound_of_everything_playing_then_added_up() {
        let spans = [span(0, 1000), span(500, 1500)];
        let mut read =
            |s: &AudioSpan, _: u64, frames: usize| vec![if s.out_start_ms == 0 { 1000 } else { 2000 }; frames * 2];
        // 4 frames at 1000 samples a second, starting 2 ms before 0.5 s: only the first span until 0.5 s, then both.
        let block = mix_block(&spans, 498, 4, 1000, &mut read);
        assert_eq!(block, vec![1000, 1000, 1000, 1000, 3000, 3000, 3000, 3000]);
    }

    #[test]
    fn the_source_is_asked_for_the_right_part_of_the_file() {
        let mut s = span(2000, 4000);
        s.source_start_ms = 10_000;
        let mut asked = Vec::new();
        let mut read = |_: &AudioSpan, first: u64, frames: usize| {
            asked.push((first, frames));
            vec![0; frames * 2]
        };
        mix_block(&[s], 2500, 100, 1000, &mut read);
        assert_eq!(asked, vec![(10_500, 100)], "500 ms into the span is 10.5 seconds into the file");
    }

    #[test]
    fn spans_outside_the_block_are_not_read_at_all() {
        let mut read = |_: &AudioSpan, _: u64, _: usize| -> StereoFrames { panic!("should not be read") };
        let block = mix_block(&[span(5000, 6000)], 0, 100, 1000, &mut read);
        assert!(block.iter().all(|v| *v == 0) && block.len() == 200);
    }

    #[test]
    fn a_loud_sum_is_clipped_never_wrapped_around() {
        let spans = [span(0, 1000), span(0, 1000)];
        let mut read = steady(30_000);
        let block = mix_block(&spans, 0, 10, 1000, &mut read);
        assert!(block.iter().all(|v| *v == i16::MAX), "{block:?}");
        let mut quiet = steady(-30_000);
        let block = mix_block(&spans, 0, 10, 1000, &mut quiet);
        assert!(block.iter().all(|v| *v == i16::MIN));
    }

    #[test]
    fn a_source_that_ends_early_leaves_silence_for_the_rest() {
        let mut read = |_: &AudioSpan, _: u64, frames: usize| vec![500; (frames / 2) * 2];
        let block = mix_block(&[span(0, 1000)], 0, 10, 1000, &mut read);
        assert_eq!(&block[..10], &[500; 10]);
        assert_eq!(&block[10..], &[0; 10]);
    }

    #[test]
    fn the_fade_shapes_the_mixed_sound() {
        let mut s = span(0, 1000);
        s.fade_in_ms = 100;
        let mut read = steady(10_000);
        let block = mix_block(&[s], 0, 200, 1000, &mut read);
        assert_eq!(block[0], 0);
        assert!((block[2 * 50] as i32 - 5000).abs() <= 120, "half way through the fade: {}", block[100]);
        assert!(block[2 * 150] >= 9990 && block[2 * 150] <= 10_000);
    }

    #[test]
    fn a_muted_span_adds_nothing() {
        let mut s = span(0, 1000);
        s.muted = true;
        let mut read = steady(10_000);
        assert!(mix_block(&[s], 0, 10, 1000, &mut read).iter().all(|v| *v == 0));
    }
}
