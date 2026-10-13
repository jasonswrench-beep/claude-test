// SPDX-License-Identifier: Apache-2.0
//! Turning captured pictures into an MP4 file, as a test of this computer's video tools.
//!
//! The screen gives a new picture only when something changes, but a video needs a steady
//! number of pictures every second. [`Pacer`] decides, on a fixed beat, when the next picture of
//! the video is due; the newest screen picture (repeated if nothing changed) fills each beat.
//!
//! As with the screen test, the real Windows parts sit behind a trait ([`VideoPlatform`]) so the
//! whole flow, and every message a person will read, is tested here with a fake.

use crate::{check, Check, Frame, Platform, ScreenError};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A video encoder the computer offers for H.264.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderInfo {
    pub name: String,
    /// Runs on the graphics card (cheaper for the processor) rather than on the processor.
    pub hardware: bool,
}

/// Something an MP4 file is written into.
pub trait VideoSink {
    /// Something worth telling the person about how this file is being made, if anything.
    fn note(&self) -> Option<String> {
        None
    }
    /// Add one picture, shown from `pts_100ns` for `duration_100ns` (in units of 100 nanoseconds).
    fn write(&mut self, frame: &Frame, pts_100ns: i64, duration_100ns: i64) -> Result<(), ScreenError>;
    /// Complete the file so that it can be played.
    fn finish(self: Box<Self>) -> Result<(), ScreenError>;
}

/// What the video checks need from the operating system.
pub trait VideoPlatform {
    /// Start Windows' video tools (Media Foundation).
    fn start_media(&self) -> Result<(), ScreenError>;
    fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError>;
    fn create_sink(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
    ) -> Result<Box<dyn VideoSink>, ScreenError>;
    /// Processor time this program has used so far, if the system can say.
    fn process_cpu_time(&self) -> Option<Duration>;
    /// How many logical processors the computer has.
    fn logical_cores(&self) -> u32;
}

/// The real thing on Windows; on any other system a stand-in that says so.
pub fn system_video_platform() -> Box<dyn VideoPlatform> {
    #[cfg(windows)]
    {
        Box::new(crate::win_video::WindowsVideo)
    }
    #[cfg(not(windows))]
    {
        Box::new(NoVideo)
    }
}

#[cfg(not(windows))]
struct NoVideo;

#[cfg(not(windows))]
impl VideoPlatform for NoVideo {
    fn start_media(&self) -> Result<(), ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn create_sink(&self, _: &Path, _: u32, _: u32, _: u32, _: u32) -> Result<Box<dyn VideoSink>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn process_cpu_time(&self) -> Option<Duration> {
        None
    }
    fn logical_cores(&self) -> u32 {
        1
    }
}

/// Decides when each picture of a steady-rate video is due.
///
/// Beat `n` falls `n / fps` seconds after the start. If the program falls behind and several beats
/// are due at once, only the latest is filled and the others are counted as skipped, so lateness
/// never snowballs.
#[derive(Debug, Clone)]
pub struct Pacer {
    fps: u32,
    next: u64,
    skipped: u64,
}

impl Pacer {
    pub fn new(fps: u32) -> Self {
        Self { fps: fps.max(1), next: 0, skipped: 0 }
    }

    /// The beat to fill now, if one is due `elapsed` after the start.
    pub fn due(&mut self, elapsed: Duration) -> Option<u64> {
        let latest = (elapsed.as_micros() * self.fps as u128 / 1_000_000) as u64;
        if latest >= self.next {
            self.skipped += latest - self.next;
            self.next = latest + 1;
            Some(latest)
        } else {
            None
        }
    }

    /// Beats that came and went without a picture.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// When beat `n` is shown, in units of 100 nanoseconds.
    pub fn pts_100ns(&self, beat: u64) -> i64 {
        (beat as u128 * 10_000_000 / self.fps as u128) as i64
    }

    /// How long beat `n` lasts, in units of 100 nanoseconds.
    pub fn duration_100ns(&self, beat: u64) -> i64 {
        self.pts_100ns(beat + 1) - self.pts_100ns(beat)
    }
}

/// What to try.
#[derive(Debug, Clone)]
pub struct VideoTestOptions {
    pub display: u32,
    pub seconds: f64,
    pub fps: u32,
    /// Video quality, as kilobits per second. 8000 is good for a 1080p screen.
    pub bitrate_kbps: u32,
    /// The folder for `test.mp4`.
    pub out_dir: PathBuf,
}

/// The share of the computer's total processing power this program may use at 1080p30 on a
/// mid-range laptop (requirement from the product brief).
pub const CPU_TARGET_PERCENT: f64 = 10.0;

fn percent(part: f64, whole: f64) -> f64 {
    if whole <= 0.0 {
        0.0
    } else {
        part * 100.0 / whole
    }
}

/// Run the video test on this computer.
pub fn run_video_test(opts: &VideoTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    let screen = crate::system_platform();
    let video = system_video_platform();
    run_video_test_with(screen.as_ref(), video.as_ref(), opts, report)
}

/// Run the video test against any platforms. Every check is reported as soon as it is known.
/// Returns whether all of them were fine.
pub fn run_video_test_with(
    screen: &dyn Platform,
    video: &dyn VideoPlatform,
    opts: &VideoTestOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };

    // 1. Windows' video tools.
    match video.start_media() {
        Ok(()) => say(check("media-foundation", true, "Windows' video tools (Media Foundation) started.")),
        Err(e) => {
            say(check(
                "media-foundation",
                false,
                format!(
                    "Windows' video tools would not start: {e} On the \"N\" and \"KN\" editions of Windows, the free Media Feature Pack from Microsoft must be installed."
                ),
            ));
            return false;
        }
    }

    // 2. An H.264 encoder.
    match video.h264_encoders() {
        Ok(list) if !list.is_empty() => {
            let names: Vec<String> = list
                .iter()
                .map(|e| {
                    format!("{} ({})", e.name, if e.hardware { "uses the graphics card" } else { "uses the processor" })
                })
                .collect();
            say(check(
                "h264-encoder",
                true,
                format!("Found {} H.264 video encoder(s): {}.", list.len(), names.join("; ")),
            ));
        }
        Ok(_) => {
            say(check(
                "h264-encoder",
                false,
                "No H.264 video encoder was found, so MP4 video cannot be made on this computer.",
            ));
            return false;
        }
        Err(e) => {
            say(check("h264-encoder", false, e.to_string()));
            return false;
        }
    }

    // 3. Start capturing the chosen display and wait for the first picture.
    let displays = match screen.list_displays() {
        Ok(d) if !d.is_empty() => d,
        Ok(_) => {
            say(check("video-encode", false, "No displays were found."));
            return false;
        }
        Err(e) => {
            say(check("video-encode", false, e.to_string()));
            return false;
        }
    };
    let Some(display) = displays.iter().find(|d| d.number == opts.display) else {
        say(check(
            "video-encode",
            false,
            format!("There is no Display {}. Choose a number from 1 to {}.", opts.display, displays.len()),
        ));
        return false;
    };
    let cpu_before = video.process_cpu_time();
    let wall_start = Instant::now();
    let mut source = match screen.start_display_capture(display, false) {
        Ok(s) => s,
        Err(e) => {
            say(check("video-encode", false, format!("Could not start capturing {}: {e}", display.describe())));
            return false;
        }
    };
    let Some(first) = source.next_frame(Duration::from_secs(3)) else {
        say(check(
            "video-encode",
            false,
            "No picture arrived from the screen within 3 seconds. Windows only sends a picture when something changes: try again while moving a window.",
        ));
        return false;
    };
    if first.width % 2 != 0 || first.height % 2 != 0 {
        say(check(
            "video-encode",
            false,
            format!(
                "The display is {} by {} pixels. H.264 video needs an even width and height, and trimming is not built yet.",
                first.width, first.height
            ),
        ));
        return false;
    }

    // 4. Make the file and fill it, one picture per beat.
    if let Err(e) = fs::create_dir_all(&opts.out_dir) {
        say(check("video-encode", false, format!("Could not create the folder {}: {e}", opts.out_dir.display())));
        return false;
    }
    let path = opts.out_dir.join("test.mp4");
    let _ = fs::remove_file(&path);
    let mut sink = match video.create_sink(&path, first.width, first.height, opts.fps, opts.bitrate_kbps) {
        Ok(s) => s,
        Err(e) => {
            say(check("video-encode", false, format!("Could not start the video file: {e}")));
            return false;
        }
    };

    let sink_note = sink.note().map(|n| format!(" {n}")).unwrap_or_default();
    let run_for = Duration::from_secs_f64(opts.seconds.clamp(1.0, 600.0));
    let mut pacer = Pacer::new(opts.fps);
    let mut latest = first;
    let mut written: u64 = 0;
    let mut write_error: Option<ScreenError> = None;
    let beats_start = Instant::now();
    loop {
        let elapsed = beats_start.elapsed();
        if elapsed >= run_for {
            break;
        }
        while let Some(newer) = source.next_frame(Duration::ZERO) {
            latest = newer;
        }
        match pacer.due(elapsed) {
            Some(beat) => {
                if let Err(e) = sink.write(&latest, pacer.pts_100ns(beat), pacer.duration_100ns(beat)) {
                    write_error = Some(e);
                    break;
                }
                written += 1;
            }
            None => {
                if let Some(newer) = source.next_frame(Duration::from_millis(2)) {
                    latest = newer;
                }
            }
        }
    }
    let dropped_by_capture = source.dropped();
    drop(source);
    let finished = sink.finish();
    let wall = wall_start.elapsed();
    let cpu_after = video.process_cpu_time();

    if let Some(e) = write_error {
        say(check("video-encode", false, format!("Writing the video stopped with a problem: {e}")));
        return false;
    }
    if let Err(e) = finished {
        say(check("video-encode", false, format!("The video file could not be completed: {e}")));
        return false;
    }

    let skipped = pacer.skipped();
    let beats = written + skipped;
    let skipped_share = percent(skipped as f64, beats as f64);
    let capture_note = if dropped_by_capture > 0 {
        format!(" The screen capture also threw away {dropped_by_capture} pictures.")
    } else {
        String::new()
    };
    if written > 0 && skipped_share <= 10.0 {
        say(check(
            "video-encode",
            true,
            format!(
                "Wrote {written} video pictures in {:.1} seconds at {} per second, each {} by {} pixels. {skipped} beats were skipped because the computer could not keep up.{capture_note}{sink_note}",
                run_for.as_secs_f64(),
                opts.fps,
                latest.width,
                latest.height
            ),
        ));
    } else {
        say(check(
            "video-encode",
            false,
            format!(
                "Wrote only {written} of {beats} video pictures: {skipped} beats ({skipped_share:.0} percent) were skipped because the computer could not keep up with {} per second.{capture_note}{sink_note}",
                opts.fps
            ),
        ));
    }

    // 5. How hard the computer worked.
    match (cpu_before, cpu_after) {
        (Some(before), Some(after)) if wall.as_secs_f64() > 0.0 => {
            let cpu_secs = after.saturating_sub(before).as_secs_f64();
            let cores = video.logical_cores().max(1);
            let of_one_core = percent(cpu_secs, wall.as_secs_f64());
            let of_all = of_one_core / cores as f64;
            let message = format!(
                "The program used about {of_one_core:.0} percent of one processor core, which is {of_all:.1} percent of this computer's total capacity ({cores} logical processors). The target is under {CPU_TARGET_PERCENT:.0} percent."
            );
            if of_all <= CPU_TARGET_PERCENT {
                say(check("cpu-use", true, message));
            } else {
                say(check(
                    "cpu-use",
                    false,
                    format!("{message} It is above the target. The video still works; this tells us what to speed up."),
                ));
            }
        }
        _ => say(check("cpu-use", false, "This computer would not say how much processing the program used.")),
    }

    // 6. The file.
    match fs::metadata(&path) {
        Ok(meta) if meta.len() > 0 => {
            let kbps = meta.len() as f64 * 8.0 / 1000.0 / run_for.as_secs_f64();
            say(check(
                "mp4-file",
                true,
                format!(
                    "Saved {} ({:.1} megabytes, about {kbps:.0} kilobits per second). Play it to check it shows your screen, is the right way up, and plays smoothly. It has no sound, because sound is not recorded yet.",
                    path.display(),
                    meta.len() as f64 / 1_000_000.0
                ),
            ));
        }
        Ok(_) => say(check("mp4-file", false, format!("The file {} was made but is empty.", path.display()))),
        Err(e) => say(check("mp4-file", false, format!("The file {} was not made: {e}", path.display()))),
    }

    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayInfo, FrameSource, WindowsVersion};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::thread::sleep;

    // ---- Pacer ----

    #[test]
    fn the_first_beat_is_due_at_once_and_the_next_one_a_thirtieth_of_a_second_later() {
        let mut p = Pacer::new(30);
        assert_eq!(p.due(Duration::ZERO), Some(0));
        assert_eq!(p.due(Duration::from_millis(10)), None);
        assert_eq!(p.due(Duration::from_millis(34)), Some(1));
        assert_eq!(p.skipped(), 0);
    }

    #[test]
    fn falling_behind_skips_beats_instead_of_catching_up() {
        let mut p = Pacer::new(30);
        assert_eq!(p.due(Duration::ZERO), Some(0));
        // A long pause: a third of a second passes with no check.
        assert_eq!(p.due(Duration::from_millis(340)), Some(10));
        assert_eq!(p.skipped(), 9, "beats 1 to 9 were never filled");
        assert_eq!(p.due(Duration::from_millis(341)), None);
    }

    #[test]
    fn times_are_exact_with_no_drift_over_an_hour() {
        let p = Pacer::new(30);
        let hour_of_beats = 30 * 3600;
        assert_eq!(p.pts_100ns(hour_of_beats), 3600 * 10_000_000);
        // Durations differ by at most one unit and always add up exactly.
        let total: i64 = (0..300).map(|b| p.duration_100ns(b)).sum();
        assert_eq!(total, p.pts_100ns(300));
        assert!((p.duration_100ns(0) - 333_333).abs() <= 1);
    }

    #[test]
    fn a_frame_rate_of_zero_is_treated_as_one() {
        let mut p = Pacer::new(0);
        assert_eq!(p.due(Duration::ZERO), Some(0));
    }

    // ---- The whole flow, with fakes ----

    /// A still screen: one picture straight away, then nothing.
    struct FakeSource {
        given: bool,
    }
    impl FrameSource for FakeSource {
        fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
            if !self.given {
                self.given = true;
                Some(Frame { width: 8, height: 4, t_ms: 0, bgra: vec![7; 8 * 4 * 4] })
            } else {
                sleep(timeout.min(Duration::from_millis(2)));
                None
            }
        }
        fn dropped(&self) -> u64 {
            0
        }
    }

    struct FakeScreen;
    impl Platform for FakeScreen {
        fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
            Ok(WindowsVersion { major: 10, minor: 0, build: 26200 })
        }
        fn capture_supported(&self) -> bool {
            true
        }
        fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
            Ok(vec![DisplayInfo {
                number: 1,
                x: 0,
                y: 0,
                width: 8,
                height: 4,
                main: true,
                system_name: "D1".into(),
                handle: 1,
            }])
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Ok(Box::new(FakeSource { given: false }))
        }
    }

    #[derive(Default)]
    struct Written {
        beats: Vec<(i64, i64)>,
        finished: bool,
    }

    struct FakeSink {
        path: PathBuf,
        written: Rc<RefCell<Written>>,
        write_delay: Duration,
        note: Option<String>,
    }

    impl VideoSink for FakeSink {
        fn note(&self) -> Option<String> {
            self.note.clone()
        }
        fn write(&mut self, frame: &Frame, pts: i64, dur: i64) -> Result<(), ScreenError> {
            assert_eq!((frame.width, frame.height), (8, 4));
            sleep(self.write_delay);
            self.written.borrow_mut().beats.push((pts, dur));
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            self.written.borrow_mut().finished = true;
            fs::write(&self.path, vec![1u8; 50_000]).map_err(|e| ScreenError::Failed(e.to_string()))
        }
    }

    struct FakeVideo {
        media: Result<(), String>,
        encoders: Vec<EncoderInfo>,
        written: Rc<RefCell<Written>>,
        write_delay: Duration,
        /// Processor seconds used between the two readings, as a share of one core.
        busy_cores: f64,
        cores: u32,
        reading: Cell<u32>,
        sink_note: Option<String>,
    }

    impl FakeVideo {
        fn good() -> Self {
            Self {
                media: Ok(()),
                encoders: vec![
                    EncoderInfo { name: "Fake GPU Encoder".into(), hardware: true },
                    EncoderInfo { name: "Fake Software Encoder".into(), hardware: false },
                ],
                written: Rc::new(RefCell::new(Written::default())),
                write_delay: Duration::from_millis(1),
                busy_cores: 0.2,
                cores: 8,
                reading: Cell::new(0),
                sink_note: None,
            }
        }
    }

    impl VideoPlatform for FakeVideo {
        fn start_media(&self) -> Result<(), ScreenError> {
            self.media.clone().map_err(ScreenError::Failed)
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(self.encoders.clone())
        }
        fn create_sink(&self, path: &Path, _: u32, _: u32, _: u32, _: u32) -> Result<Box<dyn VideoSink>, ScreenError> {
            Ok(Box::new(FakeSink {
                path: path.to_path_buf(),
                written: Rc::clone(&self.written),
                write_delay: self.write_delay,
                note: self.sink_note.clone(),
            }))
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            // The first reading is zero; the second is "busy_cores" worth of the time that has passed in the test.
            let n = self.reading.get();
            self.reading.set(n + 1);
            Some(if n == 0 { Duration::ZERO } else { Duration::from_secs_f64(self.busy_cores * 1.0) })
        }
        fn logical_cores(&self) -> u32 {
            self.cores
        }
    }

    fn run(video: &FakeVideo) -> (bool, Vec<Check>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let opts =
            VideoTestOptions { display: 1, seconds: 1.0, fps: 30, bitrate_kbps: 8000, out_dir: dir.path().join("v") };
        let mut checks = Vec::new();
        let ok = run_video_test_with(&FakeScreen, video, &opts, &mut |c| checks.push(c));
        (ok, checks, dir)
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn a_healthy_computer_writes_a_steady_video_and_passes_every_check() {
        let video = FakeVideo::good();
        let (ok, checks, dir) = run(&video);
        assert!(ok, "{checks:?}");
        let names: Vec<_> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(names, ["media-foundation", "h264-encoder", "video-encode", "cpu-use", "mp4-file"]);

        let w = video.written.borrow();
        assert!(w.finished, "the file must be completed");
        assert!((28..=31).contains(&w.beats.len()), "about 30 pictures in a second, got {}", w.beats.len());
        assert!(w.beats.windows(2).all(|p| p[1].0 > p[0].0), "times only go forward");
        assert_eq!(w.beats[0].0, 0);
        assert!(w.beats.iter().all(|(_, d)| (333_332..=333_334).contains(d)));

        assert!(find(&checks, "h264-encoder").message.contains("uses the graphics card"));
        assert!(find(&checks, "cpu-use")
            .message
            .contains("percent of this computer's total capacity (8 logical processors)"));
        assert!(dir.path().join("v/test.mp4").is_file());
        assert!(find(&checks, "mp4-file").message.contains("no sound"));
    }

    #[test]
    fn a_still_screen_still_makes_a_full_video_by_repeating_the_picture() {
        // FakeSource gives exactly one picture; the video must still get a picture on every beat.
        let video = FakeVideo::good();
        let (_ok, _checks, _dir) = run(&video);
        assert!(video.written.borrow().beats.len() >= 28);
    }

    #[test]
    fn a_fallback_to_the_processor_encoder_is_mentioned_in_the_result() {
        let mut video = FakeVideo::good();
        video.sink_note =
            Some("The graphics card's encoder could not be used, so the processor's encoder was used instead.".into());
        let (ok, checks, _dir) = run(&video);
        assert!(ok, "{checks:?}");
        assert!(find(&checks, "video-encode").message.contains("processor's encoder was used instead"));
    }

    #[test]
    fn missing_media_tools_explain_the_n_edition_fix() {
        let mut video = FakeVideo::good();
        video.media = Err("code 0xC00D36FA".into());
        let (ok, checks, _dir) = run(&video);
        assert!(!ok);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].message.contains("Media Feature Pack"));
    }

    #[test]
    fn no_h264_encoder_stops_with_a_clear_message() {
        let mut video = FakeVideo::good();
        video.encoders.clear();
        let (ok, checks, _dir) = run(&video);
        assert!(!ok);
        assert_eq!(checks.last().unwrap().check, "h264-encoder");
        assert!(checks.last().unwrap().message.contains("No H.264 video encoder"));
    }

    #[test]
    fn a_computer_too_slow_to_keep_up_is_reported_with_the_skipped_share() {
        let mut video = FakeVideo::good();
        video.write_delay = Duration::from_millis(120);
        let (ok, checks, _dir) = run(&video);
        assert!(!ok);
        let c = find(&checks, "video-encode");
        assert!(!c.ok);
        assert!(c.message.contains("could not keep up"), "{}", c.message);
    }

    #[test]
    fn too_much_processor_use_is_flagged_without_calling_the_video_broken() {
        let mut video = FakeVideo::good();
        video.busy_cores = 2.0; // 25 percent of an 8-core computer
        let (ok, checks, _dir) = run(&video);
        assert!(!ok);
        assert!(find(&checks, "video-encode").ok);
        let c = find(&checks, "cpu-use");
        assert!(!c.ok && c.message.contains("above the target") && c.message.contains("The video still works"));
    }

    #[test]
    fn an_odd_sized_display_is_explained_rather_than_producing_a_broken_file() {
        struct OddScreen;
        impl Platform for OddScreen {
            fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
                FakeScreen.windows_version()
            }
            fn capture_supported(&self) -> bool {
                true
            }
            fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
                FakeScreen.list_displays()
            }
            fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
                struct Odd(VecDeque<Frame>);
                impl FrameSource for Odd {
                    fn next_frame(&mut self, _: Duration) -> Option<Frame> {
                        self.0.pop_front()
                    }
                    fn dropped(&self) -> u64 {
                        0
                    }
                }
                Ok(Box::new(Odd(VecDeque::from([Frame { width: 7, height: 4, t_ms: 0, bgra: vec![0; 7 * 4 * 4] }]))))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let opts =
            VideoTestOptions { display: 1, seconds: 1.0, fps: 30, bitrate_kbps: 8000, out_dir: dir.path().join("v") };
        let mut checks = Vec::new();
        let ok = run_video_test_with(&OddScreen, &FakeVideo::good(), &opts, &mut |c| checks.push(c));
        assert!(!ok);
        assert!(find(&checks, "video-encode").message.contains("even width and height"));
    }

    #[cfg(not(windows))]
    #[test]
    fn on_other_systems_the_video_test_says_it_needs_windows() {
        let dir = tempfile::tempdir().unwrap();
        let opts =
            VideoTestOptions { display: 1, seconds: 1.0, fps: 30, bitrate_kbps: 8000, out_dir: dir.path().join("v") };
        let mut checks = Vec::new();
        assert!(!run_video_test(&opts, &mut |c| checks.push(c)));
        assert!(checks[0].message.contains("only works on Windows"));
    }
}
