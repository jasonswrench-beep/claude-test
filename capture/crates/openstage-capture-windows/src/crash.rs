// SPDX-License-Identifier: Apache-2.0
//! What survives when the real recorder is killed without warning.
//!
//! The recorder is started as a separate program, recorded for a while (with sound, and through a
//! pause), and stopped the way a crash or a power cut would stop it. Then the project is repaired
//! with `recover` and every file is looked at: the video, its sound track, and the separate sound
//! files. This module does the looking, so it can be tested anywhere with made-up files; starting
//! and killing the program is the command line's job.

use crate::sound::{SoundPlan, MIC_FILE, SYSTEM_FILE};
use crate::{check, Check, VideoPlatform, SCREEN_FILE};
use openstage_capture_project::{read_take, recover_project, take_dir, wav_info, TakeState};
use std::path::Path;

/// The most recording a crash may cost, for this test to call it fine. (The requirement allows 10 seconds.)
pub const MAX_LOSS_MS: u64 = 3000;
/// The most the saved files may differ in length from each other.
pub const MAX_SPREAD_MS: u64 = 2500;

/// Repair the project that the killed recorder left behind, and report what came back.
/// `recorded_ms` is how much recording time had passed when the program was killed (paused time not
/// counted). Returns whether everything was fine.
pub fn run_recorder_crash_report(
    video: &dyn VideoPlatform,
    project: &Path,
    take: u32,
    recorded_ms: u64,
    plan: &SoundPlan,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };
    let seconds = |ms: u64| ms as f64 / 1000.0;

    // 1. Repair, the way the author would.
    let repair = match recover_project(project) {
        Ok(r) => r,
        Err(e) => {
            say(check("crash-recover", false, format!("The project could not be repaired: {e}")));
            return false;
        }
    };
    match repair.recovered.iter().find(|r| r.take == take) {
        Some(r) => say(check(
            "crash-recover",
            repair.problems.is_empty(),
            format!(
                "Repair finished. Take {take} was recovered and holds {} media file(s); at most {:.1} seconds could have been lost.{}",
                r.segments,
                seconds(r.max_loss_ms),
                if repair.problems.is_empty() {
                    String::new()
                } else {
                    format!(" Repair also reported: {}", repair.problems.join(" "))
                }
            ),
        )),
        None => {
            say(check(
                "crash-recover",
                false,
                format!(
                    "Repair found nothing to recover in take {take}. Either the program finished by itself before it was killed, or it left no usable take.{}",
                    if repair.problems.is_empty() { String::new() } else { format!(" Repair said: {}", repair.problems.join(" ")) }
                ),
            ));
            return false;
        }
    }
    match read_take(project, take) {
        Ok(t) if t.state == TakeState::Recovered => {}
        Ok(t) => {
            say(check("crash-take", false, format!("After repair the take is marked {:?}, not recovered.", t.state)));
            return false;
        }
        Err(e) => {
            say(check("crash-take", false, format!("The repaired take could not be read: {e}")));
            return false;
        }
    }

    // 2. What came back, file by file.
    let dir = take_dir(project, take);
    let mut lengths: Vec<(&str, u64)> = Vec::new();
    let inspection = video.inspect_video(&dir.join(SCREEN_FILE));
    match &inspection {
        Ok(info) if info.frames > 0 => {
            say(check(
                "crash-video",
                true,
                format!(
                    "Windows reads {} pictures ({:.1} seconds) from the repaired video.",
                    info.frames,
                    seconds(info.duration_ms)
                ),
            ));
            lengths.push(("the video", info.duration_ms));
        }
        Ok(_) => say(check("crash-video", false, "The repaired video opens but holds no pictures.")),
        Err(e) => say(check("crash-video", false, format!("Windows cannot read the repaired video: {e}"))),
    }
    if !plan.is_empty() {
        if let Ok(info) = &inspection {
            match &info.audio {
                Some(a) if a.samples > 0 => {
                    say(check(
                        "crash-video-sound",
                        true,
                        format!("The repaired video has a sound track {:.1} seconds long.", seconds(a.duration_ms)),
                    ));
                    lengths.push(("the video's sound track", a.duration_ms));
                }
                _ => say(check(
                    "crash-video-sound",
                    false,
                    "The repaired video has no sound track, although sound was being recorded.",
                )),
            }
        }
    }
    for (file, wanted, label) in [
        (MIC_FILE, plan.microphone.is_some(), "the microphone file"),
        (SYSTEM_FILE, plan.system.is_some(), "the computer's sound file"),
    ] {
        if !wanted {
            continue;
        }
        let name = if file == MIC_FILE { "crash-microphone-file" } else { "crash-system-file" };
        match wav_info(&dir.join(file)) {
            Ok(info) => {
                say(check(
                    name,
                    true,
                    format!("{file} plays and holds {:.1} seconds of sound.", seconds(info.duration_ms())),
                ));
                lengths.push((label, info.duration_ms()));
            }
            Err(e) => say(check(name, false, format!("{file} could not be read after repair: {e}"))),
        }
    }
    if lengths.is_empty() {
        return false;
    }

    // 3. How much was lost, and whether the files still match each other.
    let shortest = lengths.iter().min_by_key(|l| l.1).unwrap();
    let longest = lengths.iter().max_by_key(|l| l.1).unwrap();
    let lost = recorded_ms.saturating_sub(shortest.1);
    say(check(
        "crash-loss",
        lost <= MAX_LOSS_MS,
        format!(
            "The recording ran {:.1} seconds before the program was killed. The shortest saved file ({}) kept {:.1} seconds, so about {:.1} seconds were lost. {}",
            seconds(recorded_ms),
            shortest.0,
            seconds(shortest.1),
            seconds(lost),
            if lost <= MAX_LOSS_MS {
                format!("That is within the {:.0} seconds this test allows (the product allows 10).", seconds(MAX_LOSS_MS))
            } else {
                format!("That is more than the {:.0} seconds this test allows (the product allows 10).", seconds(MAX_LOSS_MS))
            }
        ),
    ));
    let spread = longest.1 - shortest.1;
    say(check(
        "crash-in-step",
        spread <= MAX_SPREAD_MS,
        format!(
            "The longest saved file ({}) kept {:.1} seconds and the shortest ({}) {:.1}: {:.1} seconds apart. {}",
            longest.0,
            seconds(longest.1),
            shortest.0,
            seconds(shortest.1),
            seconds(spread),
            if spread <= MAX_SPREAD_MS {
                "The editor would use the part they share."
            } else {
                "That is further apart than expected, so after a crash the sound and the picture would not line up at the end."
            }
        ),
    ));
    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioChoice;
    use crate::sound::fakes::FakePlatform;
    use crate::sound::plan_sound;
    use crate::{AudioInspection, EncoderInfo, ScreenError, SinkSpec, VideoInspection, VideoSink};
    use openstage_capture::ManualClock;
    use openstage_capture_project::{create_project, create_take_with, MediaFile, WavWriter};
    use std::rc::Rc;
    use std::time::Duration;

    struct Reads(Result<VideoInspection, String>);
    impl VideoPlatform for Reads {
        fn start_media(&self) -> Result<(), ScreenError> {
            Ok(())
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(vec![])
        }
        fn create_sink(&self, _: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
        fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
            self.0.clone().map_err(ScreenError::Failed)
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            None
        }
        fn logical_cores(&self) -> u32 {
            1
        }
    }

    fn video(frames: u64, ms: u64, audio_ms: Option<u64>) -> Reads {
        Reads(Ok(VideoInspection {
            frames,
            duration_ms: ms,
            audio: audio_ms.map(|duration_ms| AudioInspection { samples: duration_ms / 21, duration_ms }),
            size: None,
        }))
    }

    fn plan() -> SoundPlan {
        let clock = Rc::new(ManualClock::new());
        let platform = FakePlatform::new(&clock);
        plan_sound(&platform, Some(&AudioChoice::Default), Some(&AudioChoice::Default)).unwrap()
    }

    /// A project as a killed recorder leaves it: take 1 still marked "recording", with a video file that
    /// ends in the middle of a fragment and sound files with a header that is behind.
    fn crashed(mic_seconds: u64, system_seconds: u64) -> tempfile::TempDir {
        fn boxed(kind: &str, payload: usize) -> Vec<u8> {
            let mut out = ((payload + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind.as_bytes());
            out.extend(std::iter::repeat_n(0u8, payload));
            out
        }
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let media = plan_media();
        create_take_with(dir.path(), 1, vec!["screen:1".into()], None, 2000, media).unwrap();
        let take = take_dir(dir.path(), 1);
        let mut mp4 = boxed("ftyp", 16);
        mp4.extend(boxed("moov", 300));
        for _ in 0..8 {
            mp4.extend(boxed("moof", 60));
            mp4.extend(boxed("mdat", 4000));
        }
        mp4.extend(boxed("moof", 60)); // the fragment that was being written
        std::fs::write(take.join("screen.mp4"), mp4).unwrap();
        for (name, seconds) in [("mic.wav", mic_seconds), ("system.wav", system_seconds)] {
            let mut w = WavWriter::create(&take.join(name), 48_000, 2).unwrap();
            w.append(&vec![100i16; seconds as usize * 48_000 * 2]).unwrap();
            w.finish().unwrap();
        }
        dir
    }

    fn plan_media() -> Vec<MediaFile> {
        let mut media = vec![MediaFile {
            file: "screen.mp4".into(),
            kind: "screen".into(),
            container: "fragmented-mp4".into(),
            mix: None,
            region: None,
            camera: None,
            interactions: None,
        }];
        media.extend(plan().media_files());
        media
    }

    fn run(dir: &Path, v: &Reads, recorded_ms: u64) -> (bool, Vec<Check>) {
        let mut checks = Vec::new();
        let ok = run_recorder_crash_report(v, dir, 1, recorded_ms, &plan(), &mut |c| checks.push(c));
        (ok, checks)
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn a_killed_recorder_comes_back_with_about_a_second_lost() {
        let dir = crashed(9, 9);
        let (ok, checks) = run(dir.path(), &video(270, 9_000, Some(8_950)), 10_200);
        assert!(ok, "{checks:#?}");
        assert!(find(&checks, "crash-recover").message.contains("Take 1 was recovered"));
        assert!(find(&checks, "crash-video").message.contains("270 pictures (9.0 seconds)"));
        assert!(find(&checks, "crash-video-sound").ok);
        assert!(find(&checks, "crash-microphone-file").message.contains("mic.wav plays and holds 9.0 seconds"));
        let loss = find(&checks, "crash-loss");
        assert!(loss.ok && loss.message.contains("about 1.2 seconds were lost"), "{}", loss.message);
        assert!(find(&checks, "crash-in-step").ok);
    }

    #[test]
    fn losing_more_than_the_limit_fails_and_names_the_file_that_lost_most() {
        let dir = crashed(5, 9);
        let (ok, checks) = run(dir.path(), &video(270, 9_000, Some(9_000)), 10_000);
        assert!(!ok);
        let loss = find(&checks, "crash-loss");
        assert!(!loss.ok && loss.message.contains("(the microphone file) kept 5.0 seconds"), "{}", loss.message);
        assert!(!find(&checks, "crash-in-step").ok, "4 seconds apart is more than expected");
    }

    #[test]
    fn a_video_that_will_not_open_after_repair_is_a_failure_in_words() {
        let dir = crashed(9, 9);
        let (ok, checks) = run(dir.path(), &Reads(Err("not a video".into())), 10_000);
        assert!(!ok);
        assert!(find(&checks, "crash-video").message.contains("cannot read the repaired video: not a video"));
        // The sound files are still looked at, and the loss is judged from what could be read.
        assert!(find(&checks, "crash-loss").ok);
    }

    #[test]
    fn a_video_without_its_sound_track_is_caught() {
        let dir = crashed(9, 9);
        let (ok, checks) = run(dir.path(), &video(270, 9_000, None), 10_000);
        assert!(!ok);
        assert!(find(&checks, "crash-video-sound").message.contains("no sound track"));
    }

    #[test]
    fn a_project_that_needs_no_repair_is_reported_not_passed_off() {
        let dir = crashed(9, 9);
        recover_project(dir.path()).unwrap(); // already repaired once
        let (ok, checks) = run(dir.path(), &video(270, 9_000, Some(9_000)), 10_000);
        assert!(!ok);
        assert!(find(&checks, "crash-recover").message.contains("found nothing to recover"));
    }

    #[test]
    fn a_missing_sound_file_is_reported_by_name() {
        let dir = crashed(9, 9);
        std::fs::remove_file(take_dir(dir.path(), 1).join("system.wav")).unwrap();
        let (ok, checks) = run(dir.path(), &video(270, 9_000, Some(9_000)), 10_000);
        assert!(!ok);
        assert!(
            find(&checks, "crash-recover").message.contains("system.wav was never created"),
            "{}",
            find(&checks, "crash-recover").message
        );
        assert!(find(&checks, "crash-system-file").message.contains("could not be read"));
    }
}
