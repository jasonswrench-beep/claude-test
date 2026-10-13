// SPDX-License-Identifier: Apache-2.0
//! What a recording did, in numbers a person can read.
//!
//! After a long recording the questions are: did the computer keep up, did the program grow without
//! limit, and did sound and picture stay together? The recorder keeps a few counters while it works
//! ([`RecordingReport`]) and [`summarize`] turns them into findings in plain words.

use crate::timing::MAX_OFFSET_MS;
use crate::{check, Check, CPU_TARGET_PERCENT};

/// How one sound track behaved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackReport {
    /// True for the microphone, false for the computer's own sound.
    pub microphone: bool,
    /// How much sound the sound card really delivered, in milliseconds.
    pub heard_ms: u64,
    /// Silence filled in where nothing arrived, in milliseconds.
    pub padded_ms: u64,
    /// Sound dropped because the sound card ran ahead of the computer, in milliseconds.
    pub trimmed_ms: u64,
    /// At worst, the sound was held back this long, so it played this much later than its picture.
    pub worst_later_ms: u64,
    /// At worst, the sound was this far short when it was due, so it played this much earlier than its picture.
    pub worst_earlier_ms: u64,
    /// How many times Windows reported lost sound.
    pub lost_moments: u64,
}

/// Counters from one recording.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordingReport {
    pub recorded_ms: u64,
    pub pictures_written: u64,
    /// Beats of the steady frame rate that came and went without a fresh picture because the program was late.
    pub beats_skipped: u64,
    /// Pictures that Windows' capture service had to throw away because the program was not taking them fast enough.
    pub pictures_replaced: u64,
    /// Real time the recording took, in seconds, and the processor time the program used meanwhile.
    pub wall_seconds: f64,
    pub cpu_seconds: Option<f64>,
    pub logical_cores: u32,
    pub memory_first_mb: Option<f64>,
    pub memory_peak_mb: Option<f64>,
    pub tracks: Vec<TrackReport>,
}

pub(crate) fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The findings for a recording, in plain words.
pub fn summarize(r: &RecordingReport) -> Vec<Check> {
    let mut out = Vec::new();

    // Did the program keep up with the screen?
    let beats = r.pictures_written + r.beats_skipped;
    let skipped_percent = if beats == 0 { 0.0 } else { r.beats_skipped as f64 * 100.0 / beats as f64 };
    out.push(check(
        "run-summary",
        skipped_percent <= 1.0,
        format!(
            "Recorded {}. {} pictures were written. {} steps of the steady 30 pictures a second ({skipped_percent:.1} percent) had to be skipped because the program was late, and Windows threw away {} pictures that were not taken in time.{}",
            clock(r.recorded_ms),
            r.pictures_written,
            r.beats_skipped,
            r.pictures_replaced,
            if skipped_percent > 1.0 { " More than 1 percent skipped means the video may look jerky." } else { "" }
        ),
    ));

    // How hard did the computer work?
    if let Some(cpu) = r.cpu_seconds.filter(|_| r.wall_seconds > 0.0) {
        let core_percent = cpu * 100.0 / r.wall_seconds;
        let cores = r.logical_cores.max(1) as f64;
        let machine_percent = core_percent / cores;
        let laptop_percent = core_percent / 4.0;
        out.push(check(
            "run-cpu",
            machine_percent <= CPU_TARGET_PERCENT,
            format!(
                "While recording, the program used about {core_percent:.0} percent of one processor core on average. That is {machine_percent:.1} percent of this computer's {} cores. If a computer with 4 cores had to do the same work, it would be about {laptop_percent:.0} percent, against a target of {CPU_TARGET_PERCENT:.0} percent{}",
                r.logical_cores,
                if laptop_percent > CPU_TARGET_PERCENT {
                    ". That projection is above the target, so a 4-core laptop may struggle; it needs a real measurement on one, or a faster way to move the pictures."
                } else {
                    "."
                }
            ),
        ));
    }

    // Did the program grow without limit?
    if let (Some(first), Some(peak)) = (r.memory_first_mb, r.memory_peak_mb) {
        let grew = peak - first;
        let hours = (r.recorded_ms as f64 / 3_600_000.0).max(1.0 / 3600.0);
        out.push(check(
            "run-memory",
            grew <= 300.0,
            format!(
                "The program used {first:.0} megabytes of memory at the start and {peak:.0} at most (grew {grew:.0}, about {:.0} per hour).{}",
                grew / hours,
                if grew > 300.0 { " Growth like this over a long recording is a leak and must be fixed." } else { "" }
            ),
        ));
    }

    // Did sound and picture stay together?
    for t in &r.tracks {
        let name = if t.microphone { "microphone" } else { "computer's sound" };
        let check_name = if t.microphone { "run-sound-microphone" } else { "run-sound-system" };
        let within = t.worst_later_ms as f64 <= MAX_OFFSET_MS && t.worst_earlier_ms as f64 <= MAX_OFFSET_MS;
        let lost = if t.lost_moments > 0 {
            format!(
                " Windows reported {} moment{} of lost sound.",
                t.lost_moments,
                if t.lost_moments == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };
        let message = if t.microphone {
            // A microphone sends sound all the time, so its sound card's speed against the computer's clock can be told.
            let net = t.heard_ms as i64 - r.recorded_ms as i64;
            let ppm = if r.recorded_ms == 0 { 0.0 } else { net as f64 * 1_000_000.0 / r.recorded_ms as f64 };
            format!(
                "The {name}'s sound card ran {} the computer's clock by {} ms over the recording ({:.0} parts per million). The program corrected this by dropping {} ms of sound and filling in {} ms of silence. At worst the sound played {} ms later, and {} ms earlier, than its picture{}.{lost}",
                if net >= 0 { "faster than" } else { "slower than" },
                net.abs(),
                ppm.abs(),
                t.trimmed_ms,
                t.padded_ms,
                t.worst_later_ms,
                t.worst_earlier_ms,
                if within { String::new() } else { format!(", more than the {MAX_OFFSET_MS:.0} ms allowed") },
            )
        } else {
            // The computer sends nothing while it is quiet, so silence is filled in and says nothing about the clock.
            format!(
                "The {name}: {} ms was filled in as silence while the computer was quiet, and {} ms of surplus was dropped. At worst the sound played {} ms later, and {} ms earlier, than its picture{}.{lost}",
                t.padded_ms,
                t.trimmed_ms,
                t.worst_later_ms,
                t.worst_earlier_ms,
                if within { String::new() } else { format!(", more than the {MAX_OFFSET_MS:.0} ms allowed") },
            )
        };
        out.push(check(check_name, within, message));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> RecordingReport {
        RecordingReport {
            recorded_ms: 3_600_000,
            pictures_written: 108_000,
            beats_skipped: 12,
            pictures_replaced: 40,
            wall_seconds: 3600.0,
            cpu_seconds: Some(1800.0),
            logical_cores: 32,
            memory_first_mb: Some(180.0),
            memory_peak_mb: Some(260.0),
            tracks: vec![
                TrackReport {
                    microphone: true,
                    heard_ms: 3_600_180,
                    padded_ms: 0,
                    trimmed_ms: 160,
                    worst_later_ms: 38,
                    worst_earlier_ms: 12,
                    lost_moments: 1,
                },
                TrackReport {
                    microphone: false,
                    heard_ms: 2_000_000,
                    padded_ms: 1_600_000,
                    trimmed_ms: 0,
                    worst_later_ms: 0,
                    worst_earlier_ms: 30,
                    lost_moments: 1,
                },
            ],
        }
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn an_hour_that_went_well_is_told_in_words() {
        let checks = summarize(&good());
        assert!(checks.iter().all(|c| c.ok), "{checks:?}");
        let summary = &find(&checks, "run-summary").message;
        assert!(summary.starts_with("Recorded 1:00:00. 108000 pictures were written. 12 steps"), "{summary}");
        let cpu = &find(&checks, "run-cpu").message;
        assert!(
            cpu.contains("about 50 percent of one processor core")
                && cpu.contains("1.6 percent of this computer's 32 cores"),
            "{cpu}"
        );
        assert!(
            cpu.contains("about 12 percent, against a target of 10 percent") && cpu.contains("may struggle"),
            "{cpu}"
        );
        let memory = &find(&checks, "run-memory").message;
        assert!(memory.contains("180 megabytes of memory at the start and 260 at most"), "{memory}");
        let mic = &find(&checks, "run-sound-microphone").message;
        assert!(
            mic.contains("ran faster than the computer's clock by 180 ms") && mic.contains("50 parts per million"),
            "{mic}"
        );
        assert!(mic.contains("38 ms later, and 12 ms earlier") && !mic.contains("more than"), "{mic}");
        let system = &find(&checks, "run-sound-system").message;
        assert!(system.contains("1600000 ms was filled in as silence while the computer was quiet"), "{system}");
    }

    #[test]
    fn a_program_that_fell_behind_says_the_video_may_look_jerky() {
        let r = RecordingReport { beats_skipped: 5_000, ..good() };
        let c = summarize(&r);
        let s = find(&c, "run-summary");
        assert!(!s.ok && s.message.contains("may look jerky"), "{}", s.message);
    }

    #[test]
    fn a_program_that_uses_too_much_of_the_computer_fails() {
        let r = RecordingReport { cpu_seconds: Some(3600.0 * 4.0), logical_cores: 8, ..good() };
        let c = summarize(&r);
        let cpu = find(&c, "run-cpu");
        assert!(!cpu.ok && cpu.message.contains("50.0 percent of this computer's 8 cores"), "{}", cpu.message);
    }

    #[test]
    fn memory_that_keeps_growing_is_called_a_leak() {
        let r = RecordingReport { memory_peak_mb: Some(900.0), ..good() };
        let c = summarize(&r);
        let m = find(&c, "run-memory");
        assert!(!m.ok && m.message.contains("is a leak"), "{}", m.message);
    }

    #[test]
    fn sound_that_wandered_too_far_from_its_picture_fails_with_the_numbers() {
        let mut r = good();
        r.tracks[0].worst_later_ms = 75;
        let c = summarize(&r);
        let m = find(&c, "run-sound-microphone");
        assert!(
            !m.ok && m.message.contains("75 ms later") && m.message.contains("more than the 40 ms allowed"),
            "{}",
            m.message
        );
        assert!(find(&c, "run-sound-system").ok, "the other track is judged on its own");
    }

    #[test]
    fn a_short_recording_with_nothing_measured_still_reports_what_it_can() {
        let r = RecordingReport { recorded_ms: 5_000, pictures_written: 150, ..RecordingReport::default() };
        let c = summarize(&r);
        assert_eq!(c.len(), 1, "no cpu, memory or sound numbers were measured, so none are claimed");
        assert!(c[0].message.starts_with("Recorded 0:05."));
    }

    #[test]
    fn times_are_said_as_minutes_and_seconds_or_hours() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}
