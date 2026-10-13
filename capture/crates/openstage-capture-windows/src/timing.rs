// SPDX-License-Identifier: Apache-2.0
//! Telling the time the way Windows stamps pictures and sound, and checking that the stamps can be trusted.
//!
//! Windows stamps every captured picture and every packet of sound with the moment it was captured,
//! on the computer's high resolution clock (the "performance counter"). If the stamps are right,
//! picture and sound can be placed on one timeline exactly, however late each arrives. This module
//! gives the current time on that clock, and [`run_timing_test_with`] measures, on a real computer,
//! how late each kind of data arrives and whether the picture and sound clocks agree.
//!
//! In the measurements, "lateness" is how long after the moment of capture the data reached this
//! program. If picture and sound arrive with different lateness, a recording that places them by
//! arrival (as the recorder does today) puts them that much out of step. The difference is the
//! number this test reports.

use crate::audio::{AudioChoice, AudioPlatform, AudioSource, CHANNELS, SAMPLE_RATE};
use crate::{check, Check, FrameSource, Platform};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The time now, in units of 100 nanoseconds, on the clock Windows uses to stamp pictures and sound.
#[cfg(windows)]
pub fn now_100ns() -> i64 {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    static FREQUENCY: OnceLock<i64> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut f = 0i64;
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    });
    let mut counter = 0i64;
    let _ = unsafe { QueryPerformanceCounter(&mut counter) };
    (counter as i128 * 10_000_000 / frequency as i128) as i64
}

/// On other systems there is no such clock; time since the program started stands in for it, so the
/// rest of the code can be tested anywhere.
#[cfg(not(windows))]
pub fn now_100ns() -> i64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    (START.get_or_init(Instant::now).elapsed().as_nanos() / 100) as i64
}

/// A few numbers that describe a set of measurements, in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Spread {
    pub count: usize,
    pub min: f64,
    pub median: f64,
    /// 95 out of 100 measurements were at or below this.
    pub p95: f64,
    pub max: f64,
}

impl Spread {
    pub fn of(values: &[f64]) -> Option<Spread> {
        if values.is_empty() {
            return None;
        }
        let mut v = values.to_vec();
        v.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        Some(Spread { count: v.len(), min: v[0], median: at(0.5), p95: at(0.95), max: v[v.len() - 1] })
    }
}

/// What to measure.
#[derive(Debug, Clone)]
pub struct TimingTestOptions {
    pub display: u32,
    pub seconds: f64,
    pub microphone: AudioChoice,
    pub output: AudioChoice,
}

/// How far from sensible a lateness can be before the stamps are not believed: pictures and sound
/// reach this program within a fraction of a second of being captured.
const SENSIBLE_MIN_MS: f64 = -20.0;
const SENSIBLE_MAX_MS: f64 = 1000.0;
/// The most one sound may be out of step with the picture before it is called a problem.
pub const MAX_OFFSET_MS: f64 = 40.0;

fn describe(what: &str, s: &Spread) -> String {
    format!(
        "{what}: {} readings, usually {:.0} ms after capture (between {:.0} and {:.0} ms; 95 out of 100 within {:.0} ms).",
        s.count, s.median, s.min, s.max, s.p95
    )
}

fn believable(s: &Spread) -> bool {
    s.min >= SENSIBLE_MIN_MS && s.median <= SENSIBLE_MAX_MS
}

/// One sound being measured.
struct Probe {
    name: &'static str,
    label: &'static str,
    source: Box<dyn AudioSource>,
    /// How late the freshest sound in each packet was when it arrived, in milliseconds.
    lateness: Vec<f64>,
    packets: usize,
    unstamped: usize,
}

/// Measure how late picture and sound arrive on this computer.
pub fn run_timing_test(opts: &TimingTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_timing_test_with(
        crate::system_platform().as_ref(),
        crate::audio::system_audio_platform().as_ref(),
        opts,
        report,
    )
}

/// Measure how late picture and sound arrive, against any platform.
pub fn run_timing_test_with(
    screen: &dyn Platform,
    audio: &dyn AudioPlatform,
    opts: &TimingTestOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };

    // The picture.
    let displays = match screen.list_displays() {
        Ok(d) => d,
        Err(e) => {
            say(check("timing-video", false, e.to_string()));
            return false;
        }
    };
    let Some(display) = displays.iter().find(|d| d.number == opts.display) else {
        say(check(
            "timing-video",
            false,
            format!("There is no Display {}. Choose a number from 1 to {}.", opts.display, displays.len()),
        ));
        return false;
    };
    let mut frames: Box<dyn FrameSource> = match screen.start_display_capture(display, false) {
        Ok(f) => f,
        Err(e) => {
            say(check("timing-video", false, format!("Could not start capturing {}: {e}", display.describe())));
            return false;
        }
    };

    // The sound.
    let devices = audio.list_devices();
    let pick = |kind, choice: &AudioChoice| -> Result<Option<crate::audio::AudioDevice>, String> {
        match &devices {
            Ok(list) => crate::audio::choose_device(list, kind, choice).map(|d| Some(d.clone())),
            Err(e) => Err(format!("The sound devices could not be listed: {e}")),
        }
    };
    let mut probes: Vec<Probe> = Vec::new();
    for (name, label, kind) in [
        ("timing-microphone", "The microphone", crate::audio::DeviceKind::Microphone),
        ("timing-system-audio", "The computer's sound", crate::audio::DeviceKind::SoundOutput),
    ] {
        let choice = if kind == crate::audio::DeviceKind::Microphone { &opts.microphone } else { &opts.output };
        let started: Result<Box<dyn AudioSource>, String> = pick(kind, choice).and_then(|d| {
            let r = if kind == crate::audio::DeviceKind::Microphone {
                audio.start_microphone(d.as_ref())
            } else {
                audio.start_system_audio(d.as_ref())
            };
            r.map_err(|e| e.to_string())
        });
        match started {
            Ok(source) => probes.push(Probe { name, label, source, lateness: Vec::new(), packets: 0, unstamped: 0 }),
            Err(why) => say(check(name, false, format!("{label} could not be listened to: {why}"))),
        }
    }

    // Listen to everything for a while.
    let mut video_lateness: Vec<f64> = Vec::new();
    let mut pictures = 0usize;
    let mut unstamped_pictures = 0usize;
    let run_for = Duration::from_secs_f64(opts.seconds.clamp(0.2, 600.0));
    let started = Instant::now();
    while started.elapsed() < run_for {
        if let Some(frame) = frames.next_frame(Duration::from_millis(5)) {
            pictures += 1;
            match (frame.captured_100ns, frame.arrived_100ns) {
                (Some(c), Some(a)) => video_lateness.push((a - c) as f64 / 10_000.0),
                _ => unstamped_pictures += 1,
            }
        }
        for p in &mut probes {
            for packet in p.source.read_packets() {
                p.packets += 1;
                let frames_in = packet.samples.len() / CHANNELS as usize;
                match (packet.captured_100ns, packet.arrived_100ns) {
                    (Some(c), Some(a)) => {
                        // The freshest sound in the packet was captured when the packet ended.
                        let ended = c + (frames_in as i64 * 10_000_000 / SAMPLE_RATE as i64);
                        p.lateness.push((a - ended) as f64 / 10_000.0);
                    }
                    _ => p.unstamped += 1,
                }
            }
        }
        sleep(Duration::from_millis(2));
    }

    // What was found.
    let video = Spread::of(&video_lateness);
    match &video {
        Some(s) if believable(s) => say(check(
            "timing-video",
            true,
            describe(&format!("Pictures of {}", display.describe()), s),
        )),
        Some(s) => say(check(
            "timing-video",
            false,
            format!(
                "{} The picture stamps do not look like times on the computer's clock, so picture and sound cannot be lined up by them.",
                describe("Pictures", s)
            ),
        )),
        None if pictures == 0 => say(check(
            "timing-video",
            false,
            "No picture arrived. Windows only sends a picture when something on the screen changes: keep a window moving during the test.",
        )),
        None => say(check("timing-video", false, "Windows gave no capture times for the pictures.")),
    }
    if unstamped_pictures > 0 && video.is_some() {
        say(check("timing-video-stamps", false, format!("{unstamped_pictures} pictures came without a capture time.")));
    }

    let mut offsets: Vec<(&'static str, f64)> = Vec::new();
    for p in &probes {
        let spread = Spread::of(&p.lateness);
        match &spread {
            Some(s) if believable(s) => {
                say(check(p.name, true, describe(p.label, s)));
                // Only stamps that can be believed are compared.
                if let Some(v) = video.as_ref().filter(|v| believable(v)) {
                    offsets.push((p.label, s.median - v.median));
                }
            }
            Some(s) => say(check(
                p.name,
                false,
                format!(
                    "{} The sound stamps do not look like times on the computer's clock, so sound and picture cannot be lined up by them.",
                    describe(p.label, s)
                ),
            )),
            None if p.packets == 0 => say(check(
                p.name,
                false,
                format!(
                    "{} sent no sound during the test. A quiet computer sends nothing: play something, and speak.",
                    p.label
                ),
            )),
            None => say(check(p.name, false, format!("Windows gave no capture times for {}.", p.label.to_lowercase()))),
        }
    }

    // The number that matters: how far out of step sound and picture would be.
    for (label, offset) in &offsets {
        let direction = if *offset >= 0.0 { "after" } else { "before" };
        let message = format!(
            "{label} arrives about {:.0} ms {direction} the picture it belongs with. In a recording placed by arrival, it would be heard about {:.0} ms {} the matching picture.",
            offset.abs(),
            offset.abs(),
            if *offset >= 0.0 { "after" } else { "before" },
        );
        if offset.abs() <= MAX_OFFSET_MS {
            say(check("timing-sync", true, format!("{message} That is within the {MAX_OFFSET_MS:.0} ms allowed.")));
        } else {
            say(check(
                "timing-sync",
                false,
                format!("{message} That is more than the {MAX_OFFSET_MS:.0} ms allowed, so the recorder should place picture and sound by their capture times. Please tell the author."),
            ));
        }
    }
    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioDevice, AudioInfo, AudioPacket, DeviceKind};
    use crate::{DisplayInfo, Frame, ScreenError, WindowsVersion};

    /// Pictures that always arrive `late_ms` after they were captured.
    struct Pictures {
        late_ms: i64,
        /// Added to the capture time, to pretend the stamps are on some other clock.
        wrong_clock_100ns: i64,
        stamped: bool,
        silent: bool,
    }
    impl FrameSource for Pictures {
        fn next_frame(&mut self, _: Duration) -> Option<Frame> {
            if self.silent {
                return None;
            }
            let now = now_100ns();
            Some(Frame {
                width: 4,
                height: 2,
                t_ms: 0,
                captured_100ns: self.stamped.then_some(now - self.late_ms * 10_000 + self.wrong_clock_100ns),
                arrived_100ns: self.stamped.then_some(now),
                bgra: vec![0; 32],
            })
        }
        fn dropped(&self) -> u64 {
            0
        }
    }

    struct Screen {
        late_ms: i64,
        wrong_clock_100ns: i64,
        stamped: bool,
        silent: bool,
    }
    impl Platform for Screen {
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
                width: 4,
                height: 2,
                main: true,
                system_name: "D1".into(),
                handle: 1,
            }])
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Ok(Box::new(Pictures {
                late_ms: self.late_ms,
                wrong_clock_100ns: self.wrong_clock_100ns,
                stamped: self.stamped,
                silent: self.silent,
            }))
        }
    }

    /// Ten milliseconds of sound every time it is asked, whose freshest sample is `late_ms` old on arrival.
    struct Sound {
        info: AudioInfo,
        late_ms: i64,
        stamped: bool,
        silent: bool,
    }
    impl AudioSource for Sound {
        fn info(&self) -> &AudioInfo {
            &self.info
        }
        fn read(&mut self) -> Vec<i16> {
            Vec::new()
        }
        fn read_packets(&mut self) -> Vec<AudioPacket> {
            if self.silent {
                return Vec::new();
            }
            let now = now_100ns();
            let ten_ms = 100_000;
            vec![AudioPacket {
                samples: vec![1; 480 * 2],
                captured_100ns: self.stamped.then_some(now - self.late_ms * 10_000 - ten_ms),
                arrived_100ns: self.stamped.then_some(now),
            }]
        }
        fn discontinuities(&self) -> u64 {
            0
        }
        fn failure(&self) -> Option<String> {
            None
        }
    }

    struct Audio {
        mic_late_ms: i64,
        system_late_ms: i64,
        stamped: bool,
        silent: bool,
    }
    impl AudioPlatform for Audio {
        fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
            Ok(vec![
                AudioDevice {
                    kind: DeviceKind::Microphone,
                    number: 1,
                    name: "Mic".into(),
                    is_default: true,
                    id: "m".into(),
                },
                AudioDevice {
                    kind: DeviceKind::SoundOutput,
                    number: 1,
                    name: "Speakers".into(),
                    is_default: true,
                    id: "s".into(),
                },
            ])
        }
        fn start_microphone(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            Ok(Box::new(Sound {
                info: AudioInfo { device: "Mic".into(), sample_rate: 48_000, channels: 2 },
                late_ms: self.mic_late_ms,
                stamped: self.stamped,
                silent: self.silent,
            }))
        }
        fn start_system_audio(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            Ok(Box::new(Sound {
                info: AudioInfo { device: "Speakers".into(), sample_rate: 48_000, channels: 2 },
                late_ms: self.system_late_ms,
                stamped: self.stamped,
                silent: self.silent,
            }))
        }
    }

    fn run(screen: &Screen, audio: &Audio) -> (bool, Vec<Check>) {
        let opts = TimingTestOptions {
            display: 1,
            seconds: 0.3,
            microphone: AudioChoice::Default,
            output: AudioChoice::Default,
        };
        let mut checks = Vec::new();
        let ok = run_timing_test_with(screen, audio, &opts, &mut |c| checks.push(c));
        (ok, checks)
    }

    fn screen() -> Screen {
        Screen { late_ms: 30, wrong_clock_100ns: 0, stamped: true, silent: false }
    }
    fn audio() -> Audio {
        Audio { mic_late_ms: 45, system_late_ms: 38, stamped: true, silent: false }
    }
    fn find<'a>(checks: &'a [Check], name: &str) -> Vec<&'a Check> {
        checks.iter().filter(|c| c.check == name).collect()
    }

    #[test]
    fn sound_and_picture_that_arrive_at_about_the_same_time_pass() {
        let (ok, checks) = run(&screen(), &audio());
        assert!(ok, "{checks:?}");
        let sync = find(&checks, "timing-sync");
        assert_eq!(sync.len(), 2, "one for the microphone and one for the computer's sound");
        assert!(
            sync[0].message.starts_with("The microphone arrives about 15 ms after the picture"),
            "{}",
            sync[0].message
        );
        assert!(
            sync[1].message.starts_with("The computer's sound arrives about 8 ms after the picture"),
            "{}",
            sync[1].message
        );
        assert!(sync.iter().all(|c| c.ok && c.message.contains("within the 40 ms allowed")));
        let video = &find(&checks, "timing-video")[0].message;
        assert!(video.contains("usually 30 ms after capture"), "{video}");
    }

    #[test]
    fn sound_that_arrives_much_earlier_than_the_picture_is_a_problem_with_a_way_forward() {
        let (ok, checks) =
            run(&Screen { late_ms: 90, ..screen() }, &Audio { mic_late_ms: 20, system_late_ms: 20, ..audio() });
        assert!(!ok);
        let sync = find(&checks, "timing-sync");
        assert!(sync.iter().all(|c| !c.ok));
        assert!(
            sync[0].message.contains("70 ms before the picture")
                && sync[0].message.contains("more than the 40 ms allowed"),
            "{}",
            sync[0].message
        );
        assert!(sync[0].message.contains("capture times"), "{}", sync[0].message);
    }

    #[test]
    fn stamps_that_are_not_on_the_computers_clock_are_not_believed() {
        // Picture times that are hours off cannot be lined up with anything.
        let (ok, checks) = run(&Screen { wrong_clock_100ns: -36_000_000_000, ..screen() }, &audio());
        assert!(!ok);
        let video = &find(&checks, "timing-video")[0];
        assert!(
            !video.ok && video.message.contains("do not look like times on the computer's clock"),
            "{}",
            video.message
        );
        assert!(find(&checks, "timing-sync").is_empty(), "no comparison is made with stamps that cannot be believed");
    }

    #[test]
    fn a_screen_that_sends_nothing_says_to_move_a_window() {
        let (ok, checks) = run(&Screen { silent: true, ..screen() }, &audio());
        assert!(!ok);
        assert!(find(&checks, "timing-video")[0].message.contains("keep a window moving"));
    }

    #[test]
    fn a_quiet_computer_says_to_play_something() {
        let (ok, checks) = run(&screen(), &Audio { silent: true, ..audio() });
        assert!(!ok);
        assert!(find(&checks, "timing-system-audio")[0].message.contains("play something"));
        assert!(find(&checks, "timing-microphone")[0].message.contains("sent no sound"));
    }

    #[test]
    fn data_without_capture_times_is_said_plainly() {
        let (ok, checks) = run(&screen(), &Audio { stamped: false, ..audio() });
        assert!(!ok);
        assert!(find(&checks, "timing-microphone")[0].message.contains("gave no capture times for the microphone"));
    }

    #[test]
    fn a_spread_describes_a_set_of_measurements() {
        assert_eq!(Spread::of(&[]), None);
        let s = Spread::of(&[10.0, 30.0, 20.0, 40.0, 100.0]).unwrap();
        assert_eq!((s.count, s.min, s.median, s.max), (5, 10.0, 30.0, 100.0));
        assert!(s.p95 >= 40.0);
    }

    #[test]
    fn the_clock_only_moves_forward() {
        let a = now_100ns();
        sleep(Duration::from_millis(5));
        let b = now_100ns();
        assert!(b - a >= 40_000, "at least 4 ms passed, saw {} units of 100 ns", b - a);
    }
}
