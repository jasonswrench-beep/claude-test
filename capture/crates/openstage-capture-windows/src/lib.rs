// SPDX-License-Identifier: Apache-2.0
//! Screen capture for Windows, and the checks that prove it works on a given computer.
//!
//! The real capture uses Windows Graphics Capture (Windows' own screen capture service, the same
//! one the Windows Snipping Tool uses) and exists only when built for Windows. Everything around
//! it (the checks, the statistics, the picture files) is ordinary code that is tested anywhere,
//! with a fake [`Platform`] standing in for Windows.
//!
//! This crate does not record to a capture project yet. [`run_screen_test`] only proves that
//! pictures arrive, how steadily, and that they show the screen.

mod audio;
mod balance;
mod bmp;
mod crash;
mod cue_player;
mod echo;
mod queue;
mod record;
mod report;
mod sound;
mod stats;
mod timing;
mod video;
#[cfg(windows)]
mod win;
#[cfg(windows)]
mod win_audio;
#[cfg(windows)]
mod win_cues;
#[cfg(windows)]
mod win_video;

pub use audio::{
    choose_device, describe_devices, mix, peak, run_audio_test, run_audio_test_with, run_find_output,
    run_find_output_with, system_audio_platform, write_wav, AudioAligner, AudioChoice, AudioDevice, AudioInfo,
    AudioPacket, AudioPlatform, AudioSource, AudioTestOptions, DeviceKind, FindOutputOptions,
    CHANNELS as AUDIO_CHANNELS, SAMPLE_RATE as AUDIO_SAMPLE_RATE,
};
pub use balance::{
    balance_of_samples, check_balance_in_files, describe_balance, judge_balance, typical_level, BalanceFinding,
    BalanceVerdict, Mix,
};
pub use bmp::write_bmp;
pub use crash::{run_recorder_crash_report, MAX_LOSS_MS as RECORDER_CRASH_MAX_LOSS_MS};
pub use cue_player::{run_cue_test, run_cue_test_with, system_cue_player, CueTestOptions};
pub use echo::{check_echo_in_files, describe_echo, detect_echo, envelope_of, EchoFinding, EchoVerdict, Envelope};
pub use openstage_capture_project::fmp4::{repair_file, scan_file, Mp4Scan, Repair, TopBox};
pub use queue::FrameQueue;
pub use record::{ScreenMedia, DECLARED_LOSS_MS, SCREEN_FILE};
pub use report::{summarize, OutputMove, RecordingReport, TrackReport};
pub use sound::{
    check_sound_files, output_may_have_changed, plan_sound, Sound, SoundFileReport, SoundPlan, MAX_GAIN_DB, MIC_FILE,
    MIN_GAIN_DB, MIXED_BITRATE_KBPS, SYSTEM_FILE,
};
pub use stats::{FrameStats, FrameSummary};
pub use timing::{now_100ns, run_timing_test, run_timing_test_with, Spread, TimingTestOptions, MAX_OFFSET_MS};
pub use video::{
    compare_audio_readback, compare_readback, plain_name, run_crash_report, run_video_test, run_video_test_with,
    system_video_platform, AudioInspection, AudioSpec, EncoderInfo, Pacer, SinkSpec, VideoInspection, VideoPlatform,
    VideoSink, VideoTestOptions, CPU_TARGET_PERCENT,
};

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Why something could not be done, in words for a person.
#[derive(Debug, Error)]
pub enum ScreenError {
    #[error("Screen capture only works on Windows.")]
    NotWindows,
    #[error("{0}")]
    Failed(String),
}

/// One display (monitor) attached to the computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    /// 1, 2, 3 ... The main display is always 1; the others follow left to right.
    pub number: u32,
    /// Where the display's top left corner sits on the whole desktop, in pixels.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub main: bool,
    /// What the system calls it, such as `\\.\DISPLAY1`. Used to pick the same display again.
    pub system_name: String,
    /// A handle the platform needs to start capturing this display.
    pub(crate) handle: isize,
}

impl DisplayInfo {
    /// For example "Display 1, 1920 by 1080, main display".
    pub fn describe(&self) -> String {
        if self.main {
            format!("Display {}, {} by {}, main display", self.number, self.width, self.height)
        } else {
            format!("Display {}, {} by {}, at position {}, {}", self.number, self.width, self.height, self.x, self.y)
        }
    }
}

/// Number the displays: the main one first, then the rest left to right (then top to bottom).
#[cfg_attr(not(windows), allow(dead_code))] // only the Windows build calls it; tests run it everywhere
pub(crate) fn number_displays(mut displays: Vec<DisplayInfo>) -> Vec<DisplayInfo> {
    displays.sort_by_key(|d| (!d.main, d.x, d.y));
    for (i, d) in displays.iter_mut().enumerate() {
        d.number = i as u32 + 1;
    }
    displays
}

/// Which Windows this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl WindowsVersion {
    /// Windows 11 starts at build 22000.
    pub fn is_windows_11_or_later(&self) -> bool {
        self.major > 10 || (self.major == 10 && self.build >= 22000)
    }

    pub fn describe(&self) -> String {
        let name = if self.is_windows_11_or_later() { "Windows 11 or later" } else { "Windows 10 or earlier" };
        format!("{name} (version {}.{}, build {})", self.major, self.minor, self.build)
    }
}

/// One picture of the screen.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Milliseconds after the first picture of this capture.
    pub t_ms: u64,
    /// When Windows captured this picture, on the clock Windows also uses for sound (units of 100
    /// nanoseconds). `None` when the source cannot say.
    pub captured_100ns: Option<i64>,
    /// When the picture reached this program, on the same clock. `None` when not measured.
    pub arrived_100ns: Option<i64>,
    /// Blue, green, red, alpha for every pixel, rows top to bottom, no padding.
    pub bgra: Vec<u8>,
}

/// Something pictures come out of.
pub trait FrameSource {
    /// The next picture, or `None` if none arrived within `timeout`.
    fn next_frame(&mut self, timeout: Duration) -> Option<Frame>;
    /// How many pictures had to be thrown away because the program was too slow to take them.
    fn dropped(&self) -> u64;
    /// How many pictures arrived in all, if the source counts them.
    fn received(&self) -> u64 {
        0
    }
}

/// Everything the checks need from the operating system.
pub trait Platform {
    fn windows_version(&self) -> Result<WindowsVersion, ScreenError>;
    /// Whether Windows' screen capture service is available.
    fn capture_supported(&self) -> bool;
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError>;
    fn start_display_capture(
        &self,
        display: &DisplayInfo,
        with_cursor: bool,
    ) -> Result<Box<dyn FrameSource>, ScreenError>;
}

/// The real thing on Windows; on any other system a stand-in that says so.
pub fn system_platform() -> Box<dyn Platform> {
    #[cfg(windows)]
    {
        Box::new(win::WindowsPlatform)
    }
    #[cfg(not(windows))]
    {
        Box::new(NotWindows)
    }
}

#[cfg(not(windows))]
struct NotWindows;

#[cfg(not(windows))]
impl Platform for NotWindows {
    fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn capture_supported(&self) -> bool {
        false
    }
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
}

/// One line of the report: what was checked, whether it was fine, and what that means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub check: String,
    pub ok: bool,
    pub message: String,
}

fn check(name: &str, ok: bool, message: impl Into<String>) -> Check {
    Check { check: name.to_string(), ok, message: message.into() }
}

/// What to try.
#[derive(Debug, Clone)]
pub struct ScreenTestOptions {
    /// Which display, by number (1 is the main display).
    pub display: u32,
    /// How long to capture.
    pub seconds: f64,
    /// Where to save the first and last pictures.
    pub out_dir: PathBuf,
    /// Draw the mouse pointer into the pictures. Off by default: the pointer is recorded
    /// separately so it can be enlarged or highlighted when editing.
    pub with_cursor: bool,
}

/// Pictures gathered over a period.
pub struct Collected {
    pub stats: FrameStats,
    pub first: Option<Frame>,
    pub last: Option<Frame>,
    pub dropped: u64,
}

/// Take pictures from `source` for `run_for`, keeping the first and the latest and noting when each came.
pub fn collect_frames(source: &mut dyn FrameSource, run_for: Duration) -> Collected {
    let started = Instant::now();
    let mut stats = FrameStats::new();
    let mut first = None;
    let mut last = None;
    while started.elapsed() < run_for {
        if let Some(frame) = source.next_frame(Duration::from_millis(100)) {
            stats.add(frame.t_ms);
            if first.is_none() {
                first = Some(frame.clone());
            }
            last = Some(frame);
        }
    }
    Collected { stats, first, last, dropped: source.dropped() }
}

fn save_bmp(dir: &Path, name: &str, frame: &Frame) -> std::io::Result<PathBuf> {
    let path = dir.join(name);
    let mut file = std::io::BufWriter::new(fs::File::create(&path)?);
    write_bmp(&mut file, frame.width, frame.height, &frame.bgra)?;
    use std::io::Write;
    file.flush()?;
    Ok(path)
}

/// The frame rate below which the screen test calls the result a problem.
pub const MIN_GOOD_FPS: f64 = 24.0;

/// Run the screen test on this computer.
pub fn run_screen_test(opts: &ScreenTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_screen_test_with(system_platform().as_ref(), opts, report)
}

/// Run the screen test against any [`Platform`]. Every check is reported as soon as it is known.
/// Returns whether all of them were fine.
pub fn run_screen_test_with(platform: &dyn Platform, opts: &ScreenTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };

    match platform.windows_version() {
        Ok(v) if v.is_windows_11_or_later() => say(check("windows-version", true, v.describe())),
        Ok(v) => say(check(
            "windows-version",
            false,
            format!(
                "{}. OpenStage Capture is designed for Windows 11 and later. Recording might still work, but this version is not supported.",
                v.describe()
            ),
        )),
        Err(e) => {
            say(check("windows-version", false, e.to_string()));
            return false;
        }
    }

    if platform.capture_supported() {
        say(check("capture-support", true, "Windows can capture the screen on this computer."));
    } else {
        say(check(
            "capture-support",
            false,
            "This computer's Windows does not offer screen capture (Windows Graphics Capture). Recording the screen will not work.",
        ));
        return false;
    }

    let displays = match platform.list_displays() {
        Ok(d) if !d.is_empty() => d,
        Ok(_) => {
            say(check("displays", false, "No displays were found."));
            return false;
        }
        Err(e) => {
            say(check("displays", false, e.to_string()));
            return false;
        }
    };
    let names: Vec<String> = displays.iter().map(DisplayInfo::describe).collect();
    let word = if displays.len() == 1 { "display" } else { "displays" };
    say(check("displays", true, format!("Found {} {word}: {}.", displays.len(), names.join("; "))));

    let Some(display) = displays.iter().find(|d| d.number == opts.display) else {
        say(check(
            "screen-capture",
            false,
            format!("There is no Display {}. Choose a number from 1 to {}.", opts.display, displays.len()),
        ));
        return false;
    };

    let mut source = match platform.start_display_capture(display, opts.with_cursor) {
        Ok(s) => s,
        Err(e) => {
            say(check("screen-capture", false, format!("Could not start capturing {}: {e}", display.describe())));
            return false;
        }
    };
    let run_for = Duration::from_secs_f64(opts.seconds.clamp(0.5, 600.0));
    let collected = collect_frames(source.as_mut(), run_for);
    drop(source);

    let summary = collected.stats.summary();
    let (Some(first), Some(last)) = (collected.first.as_ref(), collected.last.as_ref()) else {
        say(check(
            "screen-capture",
            false,
            format!(
                "No pictures arrived from {} in {:.1} seconds. Windows only sends a picture when something on the screen changes, so a completely still screen can look like this. Try again while moving a window.",
                display.describe(),
                run_for.as_secs_f64()
            ),
        ));
        return false;
    };
    let dropped_note = if collected.dropped > 0 {
        format!(" {} pictures were thrown away because the program was too slow to take them.", collected.dropped)
    } else {
        String::new()
    };
    say(check(
        "screen-capture",
        true,
        format!(
            "Received {} pictures in {:.1} seconds from {}. Each picture is {} by {} pixels.{dropped_note}",
            summary.frames,
            run_for.as_secs_f64(),
            display.describe(),
            first.width,
            first.height
        ),
    ));

    let rate_message = format!(
        "{:.1} pictures per second, with the longest wait between two pictures {} milliseconds.",
        summary.fps, summary.longest_gap_ms
    );
    if summary.fps >= MIN_GOOD_FPS {
        say(check("frame-rate", true, rate_message));
    } else {
        say(check(
            "frame-rate",
            false,
            format!(
                "{rate_message} That is below {MIN_GOOD_FPS:.0}. If the screen was mostly still this is normal, because Windows only sends a picture when something changes. If you were moving a window the whole time, this is a real problem: please tell the author."
            ),
        ));
    }

    let saved = fs::create_dir_all(&opts.out_dir)
        .and_then(|_| save_bmp(&opts.out_dir, "first.bmp", first))
        .and_then(|a| save_bmp(&opts.out_dir, "last.bmp", last).map(|b| (a, b)));
    match saved {
        Ok((a, b)) => say(check(
            "pictures-saved",
            true,
            format!(
                "Saved the first and last pictures as {} and {}. Open them to see whether they show your screen.",
                a.display(),
                b.display()
            ),
        )),
        Err(e) => say(check("pictures-saved", false, format!("Could not save the pictures: {e}"))),
    }

    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::thread::sleep;

    struct FakeSource {
        frames: VecDeque<Frame>,
        dropped: u64,
    }

    impl FrameSource for FakeSource {
        fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
            match self.frames.pop_front() {
                Some(f) => {
                    sleep(Duration::from_millis(5));
                    Some(f)
                }
                None => {
                    sleep(timeout.min(Duration::from_millis(20)));
                    None
                }
            }
        }
        fn dropped(&self) -> u64 {
            self.dropped
        }
    }

    fn picture(t_ms: u64, shade: u8) -> Frame {
        Frame { width: 4, height: 2, t_ms, captured_100ns: None, arrived_100ns: None, bgra: vec![shade; 4 * 2 * 4] }
    }

    struct FakePlatform {
        version: WindowsVersion,
        supported: bool,
        displays: Vec<DisplayInfo>,
        /// Arrival times of the pictures the fake display produces.
        arrivals: Vec<u64>,
        dropped: u64,
    }

    fn display(number: u32, main: bool, x: i32) -> DisplayInfo {
        DisplayInfo {
            number,
            x,
            y: 0,
            width: 1920,
            height: 1080,
            main,
            system_name: format!("DISPLAY{number}"),
            handle: number as isize,
        }
    }

    impl FakePlatform {
        fn good() -> Self {
            Self {
                version: WindowsVersion { major: 10, minor: 0, build: 22631 },
                supported: true,
                displays: vec![display(1, true, 0)],
                arrivals: (0..=60).map(|i| i * 1000 / 60).collect(),
                dropped: 0,
            }
        }
    }

    impl Platform for FakePlatform {
        fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
            Ok(self.version.clone())
        }
        fn capture_supported(&self) -> bool {
            self.supported
        }
        fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
            Ok(self.displays.clone())
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            let frames = self.arrivals.iter().enumerate().map(|(i, t)| picture(*t, i as u8)).collect();
            Ok(Box::new(FakeSource { frames, dropped: self.dropped }))
        }
    }

    fn run(platform: &FakePlatform, display: u32) -> (bool, Vec<Check>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let opts = ScreenTestOptions { display, seconds: 0.6, out_dir: dir.path().join("out"), with_cursor: false };
        let mut checks = Vec::new();
        let ok = run_screen_test_with(platform, &opts, &mut |c| checks.push(c));
        (ok, checks, dir)
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check named {name}: {checks:?}"))
    }

    #[test]
    fn a_healthy_computer_passes_every_check_and_leaves_two_pictures() {
        let (ok, checks, dir) = run(&FakePlatform::good(), 1);
        assert!(ok, "{checks:?}");
        let names: Vec<_> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(
            names,
            ["windows-version", "capture-support", "displays", "screen-capture", "frame-rate", "pictures-saved"]
        );
        assert!(find(&checks, "displays").message.contains("Display 1, 1920 by 1080, main display"));
        assert!(find(&checks, "screen-capture").message.contains("61 pictures"));
        for name in ["first.bmp", "last.bmp"] {
            let bytes = fs::read(dir.path().join("out").join(name)).unwrap();
            assert_eq!(&bytes[0..2], b"BM");
        }
        // The first and last pictures differ (they were made with different shades).
        let first = fs::read(dir.path().join("out/first.bmp")).unwrap();
        let last = fs::read(dir.path().join("out/last.bmp")).unwrap();
        assert_ne!(first[54], last[54]);
    }

    #[test]
    fn windows_10_is_reported_as_unsupported_but_the_test_carries_on() {
        let mut p = FakePlatform::good();
        p.version = WindowsVersion { major: 10, minor: 0, build: 19045 };
        let (ok, checks, _dir) = run(&p, 1);
        assert!(!ok);
        let v = find(&checks, "windows-version");
        assert!(!v.ok && v.message.contains("Windows 11 and later"));
        assert!(find(&checks, "screen-capture").ok, "capture is still tried");
    }

    #[test]
    fn no_screen_capture_support_stops_with_a_clear_message() {
        let mut p = FakePlatform::good();
        p.supported = false;
        let (ok, checks, _dir) = run(&p, 1);
        assert!(!ok);
        assert_eq!(checks.last().unwrap().check, "capture-support");
        assert!(checks.last().unwrap().message.contains("does not offer screen capture"));
    }

    #[test]
    fn a_display_that_does_not_exist_is_explained() {
        let (ok, checks, _dir) = run(&FakePlatform::good(), 3);
        assert!(!ok);
        let c = find(&checks, "screen-capture");
        assert!(c.message.contains("no Display 3") || c.message.contains("There is no Display 3"), "{}", c.message);
        assert!(c.message.contains("1 to 1"));
    }

    #[test]
    fn a_slow_frame_rate_is_flagged_with_advice() {
        let mut p = FakePlatform::good();
        p.arrivals = vec![0, 500, 1000];
        let (ok, checks, _dir) = run(&p, 1);
        assert!(!ok);
        let c = find(&checks, "frame-rate");
        assert!(!c.ok);
        assert!(c.message.contains("mostly still"));
        assert!(find(&checks, "screen-capture").ok);
    }

    #[test]
    fn no_pictures_at_all_is_a_failure_that_mentions_a_still_screen() {
        let mut p = FakePlatform::good();
        p.arrivals = vec![];
        let (ok, checks, _dir) = run(&p, 1);
        assert!(!ok);
        let c = find(&checks, "screen-capture");
        assert!(!c.ok && c.message.contains("completely still screen"));
        assert!(checks.iter().all(|c| c.check != "pictures-saved"));
    }

    #[test]
    fn thrown_away_pictures_are_mentioned() {
        let mut p = FakePlatform::good();
        p.dropped = 7;
        let (_ok, checks, _dir) = run(&p, 1);
        assert!(find(&checks, "screen-capture").message.contains("7 pictures were thrown away"));
    }

    #[test]
    fn displays_are_numbered_main_first_then_left_to_right() {
        let numbered = number_displays(vec![display(0, false, 3840), display(0, true, 1920), display(0, false, 0)]);
        let order: Vec<(u32, bool, i32)> = numbered.iter().map(|d| (d.number, d.main, d.x)).collect();
        assert_eq!(order, vec![(1, true, 1920), (2, false, 0), (3, false, 3840)]);
        assert_eq!(numbered[1].describe(), "Display 2, 1920 by 1080, at position 0, 0");
    }

    #[test]
    fn windows_11_starts_at_build_22000() {
        let v = |build| WindowsVersion { major: 10, minor: 0, build };
        assert!(!v(19045).is_windows_11_or_later());
        assert!(!v(21999).is_windows_11_or_later());
        assert!(v(22000).is_windows_11_or_later());
        assert!(WindowsVersion { major: 11, minor: 0, build: 1 }.is_windows_11_or_later());
    }

    #[cfg(not(windows))]
    #[test]
    fn on_other_systems_the_test_says_screen_capture_needs_windows() {
        let dir = tempfile::tempdir().unwrap();
        let opts = ScreenTestOptions { display: 1, seconds: 1.0, out_dir: dir.path().join("o"), with_cursor: false };
        let mut checks = Vec::new();
        let ok = run_screen_test(&opts, &mut |c| checks.push(c));
        assert!(!ok);
        assert!(checks[0].message.contains("only works on Windows"));
    }
}
