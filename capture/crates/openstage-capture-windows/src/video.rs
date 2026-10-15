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
use openstage_capture_project::fmp4::{repair_file, scan_file};
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
    /// Where the time of handing pictures over went so far, if the sink measures it.
    fn work(&self) -> SinkWork {
        SinkWork::default()
    }
    /// Add one picture, shown from `pts_100ns` for `duration_100ns` (in units of 100 nanoseconds).
    fn write(&mut self, frame: &Frame, pts_100ns: i64, duration_100ns: i64) -> Result<(), ScreenError>;
    /// Add sound to the file's sound track: interleaved 16-bit samples that start at `pts_100ns`.
    /// Only files made with [`SinkSpec::audio`] have a sound track.
    fn write_audio(&mut self, _samples: &[i16], _pts_100ns: i64) -> Result<(), ScreenError> {
        Err(ScreenError::Failed("This video file has no sound track.".into()))
    }
    /// Complete the file so that it can be played.
    fn finish(self: Box<Self>) -> Result<(), ScreenError>;
}

/// Where the time of handing pictures to the video writer went, summed over the pictures (wall-clock time).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SinkWork {
    /// Pictures handed over.
    pub pictures: u64,
    /// Copying each picture into memory of the writer's own.
    pub copy: Duration,
    /// The writer's own work when given a picture: converting its colours and starting the encoder.
    pub call: Duration,
}

impl SinkWork {
    /// What was done since `earlier` (a reading taken before).
    pub fn since(&self, earlier: &SinkWork) -> SinkWork {
        SinkWork {
            pictures: self.pictures.saturating_sub(earlier.pictures),
            copy: self.copy.saturating_sub(earlier.copy),
            call: self.call.saturating_sub(earlier.call),
        }
    }
}

/// A sound track for the video file: AAC, the sound that every player plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: u16,
    /// Quality in kilobits per second. Windows' AAC encoder accepts 96, 128, 160, and 192 for stereo.
    pub bitrate_kbps: u32,
}

/// How to make the video file.
#[derive(Debug, Clone)]
pub struct SinkSpec {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    /// Write a fragmented MP4: a setup box and then many short, self-contained fragments, so that a
    /// file cut off by a crash still plays up to the last whole fragment.
    pub fragmented: bool,
    /// Give the file a sound track too (its sound is added with [`VideoSink::write_audio`]).
    pub audio: Option<AudioSpec>,
}

/// What Windows finds when it reads a video file back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoInspection {
    pub frames: u64,
    pub duration_ms: u64,
    /// The sound track, if the file has one.
    pub audio: Option<AudioInspection>,
    /// The width and height of the pictures, if the file says.
    pub size: Option<(u32, u32)>,
}

/// What Windows finds when it reads a file's sound track back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInspection {
    /// Pieces of sound (each a fraction of a second of compressed sound).
    pub samples: u64,
    pub duration_ms: u64,
}

/// What the video checks need from the operating system.
pub trait VideoPlatform {
    /// Start Windows' video tools (Media Foundation).
    fn start_media(&self) -> Result<(), ScreenError>;
    fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError>;
    /// Whether Windows has an AAC sound encoder, which the sound inside an MP4 needs.
    fn aac_encoder_available(&self) -> Result<bool, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError>;
    /// Open a finished (or cut-off) video file the way a player would and count what can be read.
    fn inspect_video(&self, path: &Path) -> Result<VideoInspection, ScreenError>;
    /// Processor time this program has used so far, if the system can say.
    fn process_cpu_time(&self) -> Option<Duration>;
    /// Processor time the calling thread alone has used so far, if the system can say.
    fn thread_cpu_time(&self) -> Option<Duration> {
        None
    }
    /// How many logical processors the computer has.
    fn logical_cores(&self) -> u32;
    /// Memory this program is using right now, in megabytes, if the system can say.
    fn process_memory_mb(&self) -> Option<f64> {
        None
    }
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
    fn create_sink(&self, _: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn process_cpu_time(&self) -> Option<Duration> {
        None
    }
    fn logical_cores(&self) -> u32 {
        1
    }
}

/// An encoder's name made safe to read aloud and to show in any console: trademark signs become
/// words and runs of spaces collapse. (Windows' console draws the real symbols as garbage.)
pub fn plain_name(name: &str) -> String {
    let swapped = name.replace('\u{00AE}', "(R)").replace('\u{2122}', "(TM)").replace('\u{00A9}', "(C)");
    swapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "NVIDIA H.264 Encoder MFT (uses the graphics card)", with identical entries merged ("2 found").
fn describe_encoders(list: &[EncoderInfo]) -> String {
    let mut merged: Vec<(String, bool, usize)> = Vec::new();
    for e in list {
        let name = plain_name(&e.name);
        match merged.iter_mut().find(|(n, h, _)| *n == name && *h == e.hardware) {
            Some(entry) => entry.2 += 1,
            None => merged.push((name, e.hardware, 1)),
        }
    }
    merged
        .iter()
        .map(|(name, hardware, n)| {
            let how = if *hardware { "uses the graphics card" } else { "uses the processor" };
            if *n > 1 {
                format!("{name} ({how}; {n} found)")
            } else {
                format!("{name} ({how})")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
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
    /// The folder for the video (`test.mp4`, or `test_fragmented.mp4`).
    pub out_dir: PathBuf,
    /// Write a fragmented MP4 that survives a crash (see [`SinkSpec::fragmented`]).
    pub fragmented: bool,
    /// A file to create at the exact moment recording begins, so another program can time the
    /// recording precisely (the crash test uses it).
    pub start_marker: Option<PathBuf>,
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
            say(check(
                "h264-encoder",
                true,
                format!("Found {} H.264 video encoder(s): {}.", list.len(), describe_encoders(&list)),
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
    let path = opts.out_dir.join(if opts.fragmented { "test_fragmented.mp4" } else { "test.mp4" });
    let _ = fs::remove_file(&path);
    let spec = SinkSpec {
        path: path.clone(),
        width: first.width,
        height: first.height,
        fps: opts.fps,
        bitrate_kbps: opts.bitrate_kbps,
        fragmented: opts.fragmented,
        audio: None,
    };
    let mut sink = match video.create_sink(&spec) {
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
    if let Some(marker) = &opts.start_marker {
        let _ = fs::write(marker, b"recording");
    }
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
        format!(" While the screen changed faster than the video needed, the capture replaced {dropped_by_capture} older pictures with newer ones, which is harmless.")
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

    // 4b. Ask Windows to read the finished file back the way a player would.
    match video.inspect_video(&path) {
        Ok(info) if info.frames > 0 && info.frames.abs_diff(written) * 100 <= written.max(1) * 5 => say(check(
            "video-readback",
            true,
            format!(
                "Windows read the finished video back: {} pictures, {:.1} seconds long.",
                info.frames,
                info.duration_ms as f64 / 1000.0
            ),
        )),
        Ok(info) => say(check(
            "video-readback",
            false,
            format!(
                "Windows read back {} pictures ({:.1} seconds) from the finished video, but {written} were written.",
                info.frames,
                info.duration_ms as f64 / 1000.0
            ),
        )),
        Err(e) => say(check("video-readback", false, format!("Windows could not read the finished video back: {e}"))),
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

/// Compare what Windows reads back from a saved recording with how long the recording ran.
/// The length may differ a little (a video is cut into frames and fragments), so up to 0.7 seconds is fine.
pub fn compare_readback(read: Result<VideoInspection, ScreenError>, recorded_ms: u64) -> Check {
    match read {
        Ok(info) if info.frames > 0 && info.duration_ms.abs_diff(recorded_ms) <= 700 => check(
            "video-readback",
            true,
            format!(
                "Windows read the saved video back: {} pictures, {:.1} seconds long. The recording time was {:.1} seconds.",
                info.frames,
                info.duration_ms as f64 / 1000.0,
                recorded_ms as f64 / 1000.0
            ),
        ),
        Ok(info) => check(
            "video-readback",
            false,
            format!(
                "Windows read back {} pictures ({:.1} seconds) from the saved video, but the recording time was {:.1} seconds.",
                info.frames,
                info.duration_ms as f64 / 1000.0,
                recorded_ms as f64 / 1000.0
            ),
        ),
        Err(e) => check("video-readback", false, format!("Windows could not read the saved video back: {e}")),
    }
}

/// Compare what Windows reads back from the saved webcam video with how long the recording ran. A webcam that
/// stopped partway (`stopped_at_ms`) makes a shorter video, which is right: it must not be longer than the moment
/// the camera stopped, and it must not be empty. Otherwise it must be as long as the recording, to within 0.7 seconds.
pub fn compare_webcam_readback(
    read: Result<VideoInspection, ScreenError>,
    recorded_ms: u64,
    stopped_at_ms: Option<u64>,
) -> Check {
    match read {
        Ok(info) if info.frames > 0 && stopped_at_ms.is_some_and(|stopped| info.duration_ms <= stopped + 700) => check(
            "webcam-readback",
            true,
            format!(
                "Windows read the saved webcam video back: {} pictures, {:.1} seconds long{}. The camera stopped before the end of the recording, so it is shorter than the recording ({:.1} seconds), as it should be.",
                info.frames,
                info.duration_ms as f64 / 1000.0,
                size_text(&info),
                recorded_ms as f64 / 1000.0
            ),
        ),
        Ok(info) if stopped_at_ms.is_none() && info.frames > 0 && info.duration_ms.abs_diff(recorded_ms) <= 700 => check(
            "webcam-readback",
            true,
            format!(
                "Windows read the saved webcam video back: {} pictures, {:.1} seconds long{}. The recording time was {:.1} seconds.",
                info.frames,
                info.duration_ms as f64 / 1000.0,
                size_text(&info),
                recorded_ms as f64 / 1000.0
            ),
        ),
        Ok(info) => check(
            "webcam-readback",
            false,
            format!(
                "Windows read back {} pictures ({:.1} seconds) from the saved webcam video, but the recording time was {:.1} seconds.",
                info.frames,
                info.duration_ms as f64 / 1000.0,
                recorded_ms as f64 / 1000.0
            ),
        ),
        Err(e) => check("webcam-readback", false, format!("Windows could not read the saved webcam video back: {e}")),
    }
}

/// ", 1280 by 720 pixels" when the file says its size, else nothing.
fn size_text(info: &VideoInspection) -> String {
    info.size.map(|(w, h)| format!(", {w} by {h} pixels")).unwrap_or_default()
}

/// Compare the size of the pictures Windows reads back from a saved recording with the size that was asked
/// for (a region). A file that does not say its size is reported, not failed: the size is a convenience check.
pub fn compare_size_readback(read: &Result<VideoInspection, ScreenError>, wanted: (u32, u32)) -> Check {
    match read {
        Ok(VideoInspection { size: Some(got), .. }) if *got == wanted => check(
            "video-size",
            true,
            format!("The saved video is {} by {} pixels, the size of the region.", got.0, got.1),
        ),
        Ok(VideoInspection { size: Some(got), .. }) => check(
            "video-size",
            false,
            format!(
                "The saved video is {} by {} pixels, but the region is {} by {}.",
                got.0, got.1, wanted.0, wanted.1
            ),
        ),
        Ok(_) => check(
            "video-size",
            true,
            "The saved video does not say how big its pictures are, so the size was not compared.",
        ),
        Err(e) => {
            check("video-size", false, format!("Windows could not read the saved video back to check its size: {e}"))
        }
    }
}

/// Compare the sound track Windows reads back from a saved recording with how long the recording ran.
/// Reports only on the sound; [`compare_readback`] reports on the picture. A recording made with
/// sound must have a sound track about as long as the recording (up to 0.7 seconds apart is fine).
pub fn compare_audio_readback(read: &Result<VideoInspection, ScreenError>, recorded_ms: u64) -> Check {
    match read {
        Ok(VideoInspection { audio: Some(a), .. }) if a.samples > 0 && a.duration_ms.abs_diff(recorded_ms) <= 700 => {
            check(
                "audio-readback",
                true,
                format!(
                    "Windows read the sound track of the saved video back: {:.1} seconds long. The recording time was {:.1} seconds.",
                    a.duration_ms as f64 / 1000.0,
                    recorded_ms as f64 / 1000.0
                ),
            )
        }
        Ok(VideoInspection { audio: Some(a), .. }) => check(
            "audio-readback",
            false,
            format!(
                "The sound track of the saved video is {:.1} seconds long, but the recording time was {:.1} seconds. Sound and picture would not stay together.",
                a.duration_ms as f64 / 1000.0,
                recorded_ms as f64 / 1000.0
            ),
        ),
        Ok(_) => check("audio-readback", false, "The saved video has no sound track, although sound was being recorded."),
        Err(e) => check("audio-readback", false, format!("Windows could not read the sound track back: {e}")),
    }
}

/// After a recording was killed in the middle, find out what survived. Reports each finding as a
/// check; returns whether all were fine. `recorded_ms` is how long the recording ran before the kill.
pub fn run_crash_report(
    video: &dyn VideoPlatform,
    path: &Path,
    recorded_ms: u64,
    report: &mut dyn FnMut(Check),
) -> bool {
    /// The most recording a crash may cost, for this test to call it fine. (The requirement allows 10 seconds.)
    const MAX_LOSS_MS: u64 = 3000;

    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };
    let seconds = |ms: u64| ms as f64 / 1000.0;

    match fs::metadata(path) {
        Ok(m) if m.len() > 0 => {}
        Ok(_) => {
            say(check(
                "crash-file",
                false,
                "The video file exists but is empty: nothing had been saved when the program was killed.",
            ));
            return false;
        }
        Err(e) => {
            say(check(
                "crash-file",
                false,
                format!("The video file does not exist after the kill, so nothing was saved: {e}"),
            ));
            return false;
        }
    }

    let scan = match scan_file(path) {
        Ok(s) => s,
        Err(e) => {
            say(check("crash-scan", false, format!("The file could not be examined: {e}")));
            return false;
        }
    };
    let ending = if scan.unsafe_tail() == 0 {
        "ends cleanly between two fragments".to_string()
    } else {
        format!("ends inside an unfinished piece ({} bytes of it)", scan.unsafe_tail())
    };
    if scan.safe_len == 0 {
        say(check(
            "crash-scan",
            false,
            format!(
                "The file is {:.1} megabytes but has no usable start, so it cannot be recovered.",
                scan.file_len as f64 / 1e6
            ),
        ));
        return false;
    }
    say(check(
        "crash-scan",
        true,
        format!(
            "The file is {:.1} megabytes, holds {} complete fragments of video, and {ending}.",
            scan.file_len as f64 / 1e6,
            scan.fragments
        ),
    ));

    // What Windows makes of the file exactly as the crash left it.
    match video.inspect_video(path) {
        Ok(info) => say(check(
            "crash-read-as-is",
            true,
            format!(
                "Without any repair, Windows can read {} pictures ({:.1} seconds) from the file.",
                info.frames,
                seconds(info.duration_ms)
            ),
        )),
        Err(e) => say(check(
            "crash-read-as-is",
            false,
            format!("Without any repair, Windows cannot read the file: {e} (Repair is tried next.)"),
        )),
    }

    // Cut off the unfinished last piece, then read again.
    match repair_file(path) {
        Ok(r) if r.before != r.after => say(check(
            "crash-repair",
            true,
            format!(
                "Cut the file back from {} to {} bytes, removing only the unfinished last piece.",
                r.before, r.after
            ),
        )),
        Ok(_) => say(check("crash-repair", true, "The file already ended cleanly, so nothing needed cutting off.")),
        Err(e) => {
            say(check("crash-repair", false, format!("The file could not be repaired: {e}")));
            return false;
        }
    }
    let recovered_ms = match video.inspect_video(path) {
        Ok(info) if info.frames > 0 => {
            say(check(
                "crash-read-after-repair",
                true,
                format!(
                    "After repair, Windows reads {} pictures ({:.1} seconds).",
                    info.frames,
                    seconds(info.duration_ms)
                ),
            ));
            info.duration_ms
        }
        Ok(_) => {
            say(check("crash-read-after-repair", false, "After repair, the file holds no video pictures."));
            return false;
        }
        Err(e) => {
            say(check(
                "crash-read-after-repair",
                false,
                format!("Even after repair, Windows cannot read the file: {e}"),
            ));
            return false;
        }
    };

    let lost = recorded_ms.saturating_sub(recovered_ms);
    let verdict = format!(
        "The recording ran about {:.1} seconds before the program was killed. {:.1} seconds were saved, and about {:.1} seconds were lost.",
        seconds(recorded_ms),
        seconds(recovered_ms),
        seconds(lost)
    );
    if lost <= MAX_LOSS_MS {
        say(check(
            "crash-loss",
            true,
            format!("{verdict} That is within the target of {:.0} seconds.", seconds(MAX_LOSS_MS)),
        ));
    } else {
        say(check(
            "crash-loss",
            false,
            format!(
                "{verdict} That is more than the target of {:.0} seconds (the product allows at most 10).",
                seconds(MAX_LOSS_MS)
            ),
        ));
    }
    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayInfo, FrameSource, WindowsVersion};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::path::Path;
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
                Some(Frame {
                    width: 8,
                    height: 4,
                    t_ms: 0,
                    captured_100ns: None,
                    arrived_100ns: None,
                    bgra: vec![7; 8 * 4 * 4],
                })
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
        /// How reading a file back behaves.
        inspect: Inspect,
    }

    #[derive(Clone)]
    enum Inspect {
        /// Count what was written (for a normal test).
        WhatWasWritten,
        /// Count one second per complete fragment in the file, and refuse files with an unfinished tail
        /// unless they are `tolerant` (as a player might).
        Fragments {
            tolerant: bool,
        },
        Nothing,
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
                inspect: Inspect::WhatWasWritten,
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
        fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            Ok(Box::new(FakeSink {
                path: spec.path.clone(),
                written: Rc::clone(&self.written),
                write_delay: self.write_delay,
                note: self.sink_note.clone(),
            }))
        }
        fn inspect_video(&self, path: &Path) -> Result<VideoInspection, ScreenError> {
            match &self.inspect {
                Inspect::WhatWasWritten => {
                    let frames = self.written.borrow().beats.len() as u64;
                    Ok(VideoInspection { frames, duration_ms: frames * 1000 / 30, audio: None, size: None })
                }
                Inspect::Fragments { tolerant } => {
                    let scan = scan_file(path).map_err(|e| ScreenError::Failed(e.to_string()))?;
                    if scan.unsafe_tail() > 0 && !*tolerant {
                        return Err(ScreenError::Failed("the file ends in the middle of a piece".into()));
                    }
                    Ok(VideoInspection {
                        frames: scan.fragments as u64 * 30,
                        duration_ms: scan.fragments as u64 * 1000,
                        audio: None,
                        size: None,
                    })
                }
                Inspect::Nothing => Err(ScreenError::Failed("not a video file".into())),
            }
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
        let opts = VideoTestOptions {
            display: 1,
            seconds: 1.0,
            fps: 30,
            bitrate_kbps: 8000,
            out_dir: dir.path().join("v"),
            fragmented: false,
            start_marker: None,
        };
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
        assert_eq!(
            names,
            ["media-foundation", "h264-encoder", "video-encode", "video-readback", "cpu-use", "mp4-file"]
        );

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
                Ok(Box::new(Odd(VecDeque::from([Frame {
                    width: 7,
                    height: 4,
                    t_ms: 0,
                    captured_100ns: None,
                    arrived_100ns: None,
                    bgra: vec![0; 7 * 4 * 4],
                }]))))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let opts = VideoTestOptions {
            display: 1,
            seconds: 1.0,
            fps: 30,
            bitrate_kbps: 8000,
            out_dir: dir.path().join("v"),
            fragmented: false,
            start_marker: None,
        };
        let mut checks = Vec::new();
        let ok = run_video_test_with(&OddScreen, &FakeVideo::good(), &opts, &mut |c| checks.push(c));
        assert!(!ok);
        assert!(find(&checks, "video-encode").message.contains("even width and height"));
    }

    #[test]
    fn the_start_marker_appears_when_recording_begins_and_not_before_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");
        let opts = VideoTestOptions {
            display: 1,
            seconds: 1.0,
            fps: 30,
            bitrate_kbps: 8000,
            out_dir: dir.path().join("v"),
            fragmented: true,
            start_marker: Some(marker.clone()),
        };
        let mut checks = Vec::new();
        assert!(run_video_test_with(&FakeScreen, &FakeVideo::good(), &opts, &mut |c| checks.push(c)));
        assert!(marker.is_file(), "marker made once recording began");
        assert!(dir.path().join("v/test_fragmented.mp4").is_file(), "the fragmented name is used");

        // With no encoder the test stops before recording, so no marker is made.
        let marker2 = dir.path().join("never");
        let mut video = FakeVideo::good();
        video.encoders.clear();
        let opts2 = VideoTestOptions { start_marker: Some(marker2.clone()), ..opts };
        assert!(!run_video_test_with(&FakeScreen, &video, &opts2, &mut |_| {}));
        assert!(!marker2.exists());
    }

    #[test]
    fn a_saved_video_that_matches_the_recording_time_passes_and_a_mismatch_does_not() {
        let info = |frames, duration_ms| Ok(VideoInspection { frames, duration_ms, audio: None, size: None });
        let c = compare_readback(info(222, 7400), 7480);
        assert!(
            c.ok && c.message.contains("222 pictures, 7.4 seconds long")
                && c.message.contains("recording time was 7.5")
        );
        assert!(!compare_readback(info(222, 5000), 7400).ok, "the video is much shorter than the recording");
        assert!(!compare_readback(info(0, 7400), 7400).ok, "no pictures at all");
        let c = compare_readback(Err(ScreenError::Failed("not a video".into())), 1000);
        assert!(!c.ok && c.message.contains("could not read the saved video back"));
    }

    #[test]
    fn a_saved_webcam_video_is_as_long_as_the_recording_unless_the_camera_stopped() {
        let info = |frames, duration_ms, size| Ok(VideoInspection { frames, duration_ms, audio: None, size });
        let c = compare_webcam_readback(info(222, 7400, Some((1280, 720))), 7480, None);
        assert!(
            c.check == "webcam-readback"
                && c.ok
                && c.message.contains("222 pictures, 7.4 seconds long, 1280 by 720 pixels")
                && c.message.contains("recording time was 7.5"),
            "{}",
            c.message
        );
        assert!(
            !compare_webcam_readback(info(222, 5000, None), 7400, None).ok,
            "much shorter, and the camera did not stop"
        );
        assert!(!compare_webcam_readback(info(0, 7400, None), 7400, None).ok, "no pictures at all");
        // A camera that stopped at 5 seconds makes a 5 second video, and that is right.
        let c = compare_webcam_readback(info(150, 4900, None), 7400, Some(5000));
        assert!(c.ok && c.message.contains("camera stopped before the end"), "{}", c.message);
        assert!(
            !compare_webcam_readback(info(220, 7400, None), 7400, Some(5000)).ok,
            "longer than the camera was there"
        );
        let c = compare_webcam_readback(Err(ScreenError::Failed("not a video".into())), 1000, None);
        assert!(!c.ok && c.message.contains("could not read the saved webcam video back"), "{}", c.message);
    }

    #[test]
    fn trademark_signs_become_words_and_duplicates_are_merged() {
        assert_eq!(
            plain_name("Intel\u{00AE} Quick  Sync Video H.264 Encoder MFT"),
            "Intel(R) Quick Sync Video H.264 Encoder MFT"
        );
        assert_eq!(plain_name("Some\u{2122}  Thing"), "Some(TM) Thing");
        let list = vec![
            EncoderInfo { name: "NVIDIA H.264 Encoder MFT".into(), hardware: true },
            EncoderInfo { name: "Intel\u{00AE} Quick Sync Video H.264 Encoder MFT".into(), hardware: true },
            EncoderInfo { name: "Intel\u{00AE} Quick Sync Video H.264 Encoder MFT".into(), hardware: true },
            EncoderInfo { name: "H264 Encoder MFT".into(), hardware: false },
        ];
        let text = describe_encoders(&list);
        assert_eq!(
            text,
            "NVIDIA H.264 Encoder MFT (uses the graphics card); Intel(R) Quick Sync Video H.264 Encoder MFT (uses the graphics card; 2 found); H264 Encoder MFT (uses the processor)"
        );
        assert!(text.is_ascii(), "nothing a console could draw wrongly");
    }

    #[test]
    fn a_video_windows_cannot_read_back_is_a_failure_even_if_a_file_exists() {
        let mut video = FakeVideo::good();
        video.inspect = Inspect::Nothing;
        let (ok, checks, _dir) = run(&video);
        assert!(!ok);
        assert!(find(&checks, "video-encode").ok);
        let c = find(&checks, "video-readback");
        assert!(!c.ok && c.message.contains("could not read the finished video back"));
    }

    // ---- The crash report ----

    fn boxed(kind: &str, payload_len: usize) -> Vec<u8> {
        let mut out = ((payload_len + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind.as_bytes());
        out.extend(std::iter::repeat_n(0u8, payload_len));
        out
    }

    /// A fragmented file with `whole` fragments and then a torn one if `torn_bytes` > 0.
    fn killed_file(dir: &Path, whole: usize, torn_bytes: usize) -> PathBuf {
        let mut bytes = boxed("ftyp", 16);
        bytes.extend(boxed("moov", 200));
        for _ in 0..whole {
            bytes.extend(boxed("moof", 60));
            bytes.extend(boxed("mdat", 4000));
        }
        if torn_bytes > 0 {
            bytes.extend(boxed("moof", 60));
            bytes.extend_from_slice(&boxed("mdat", 4000)[..torn_bytes]);
        }
        let path = dir.join("test_fragmented.mp4");
        fs::write(&path, bytes).unwrap();
        path
    }

    fn crash(video: &FakeVideo, path: &Path, recorded_ms: u64) -> (bool, Vec<Check>) {
        let mut checks = Vec::new();
        let ok = run_crash_report(video, path, recorded_ms, &mut |c| checks.push(c));
        (ok, checks)
    }

    #[test]
    fn a_killed_recording_loses_only_the_unfinished_fragment() {
        let dir = tempfile::tempdir().unwrap();
        let path = killed_file(dir.path(), 5, 1500);
        let mut video = FakeVideo::good();
        video.inspect = Inspect::Fragments { tolerant: true };
        let (ok, checks) = crash(&video, &path, 6200);
        assert!(ok, "{checks:?}");
        let names: Vec<_> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(names, ["crash-scan", "crash-read-as-is", "crash-repair", "crash-read-after-repair", "crash-loss"]);
        assert!(find(&checks, "crash-scan").message.contains("holds 5 complete fragments"));
        assert!(find(&checks, "crash-scan").message.contains("inside an unfinished piece"));
        assert!(find(&checks, "crash-repair").message.contains("removing only the unfinished last piece"));
        assert!(find(&checks, "crash-loss")
            .message
            .contains("5.0 seconds were saved, and about 1.2 seconds were lost"));
        // The file really was cut back.
        assert_eq!(scan_file(&path).unwrap().unsafe_tail(), 0);
    }

    #[test]
    fn a_player_that_refuses_the_torn_file_is_reported_but_repair_still_saves_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = killed_file(dir.path(), 4, 900);
        let mut video = FakeVideo::good();
        video.inspect = Inspect::Fragments { tolerant: false };
        let (ok, checks) = crash(&video, &path, 5000);
        assert!(!ok, "the as-is read failed, which is reported");
        assert!(!find(&checks, "crash-read-as-is").ok);
        assert!(find(&checks, "crash-read-after-repair").ok);
        assert!(find(&checks, "crash-loss").ok);
    }

    #[test]
    fn losing_too_much_is_flagged_with_the_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let path = killed_file(dir.path(), 2, 0);
        let mut video = FakeVideo::good();
        video.inspect = Inspect::Fragments { tolerant: true };
        let (ok, checks) = crash(&video, &path, 9000);
        assert!(!ok);
        let c = find(&checks, "crash-loss");
        assert!(!c.ok && c.message.contains("2.0 seconds were saved") && c.message.contains("7.0 seconds were lost"));
    }

    #[test]
    fn a_missing_or_empty_file_after_the_kill_is_the_first_finding() {
        let dir = tempfile::tempdir().unwrap();
        let (ok, checks) = crash(&FakeVideo::good(), &dir.path().join("nope.mp4"), 5000);
        assert!(!ok);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].message.contains("does not exist"));

        let empty = dir.path().join("empty.mp4");
        fs::write(&empty, b"").unwrap();
        let (ok, checks) = crash(&FakeVideo::good(), &empty, 5000);
        assert!(!ok && checks[0].message.contains("exists but is empty"));
    }

    #[test]
    fn a_file_with_no_usable_start_cannot_be_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_fragmented.mp4");
        fs::write(&path, [9u8; 300]).unwrap();
        let (ok, checks) = crash(&FakeVideo::good(), &path, 5000);
        assert!(!ok);
        assert!(find(&checks, "crash-scan").message.contains("no usable start"));
    }

    #[cfg(not(windows))]
    #[test]
    fn on_other_systems_the_video_test_says_it_needs_windows() {
        let dir = tempfile::tempdir().unwrap();
        let opts = VideoTestOptions {
            display: 1,
            seconds: 1.0,
            fps: 30,
            bitrate_kbps: 8000,
            out_dir: dir.path().join("v"),
            fragmented: false,
            start_marker: None,
        };
        let mut checks = Vec::new();
        assert!(!run_video_test(&opts, &mut |c| checks.push(c)));
        assert!(checks[0].message.contains("only works on Windows"));
    }

    #[test]
    fn the_size_of_a_saved_video_is_compared_with_the_region() {
        let with = |size| Ok(VideoInspection { frames: 10, duration_ms: 300, audio: None, size });
        let ok = compare_size_readback(&with(Some((1280, 720))), (1280, 720));
        assert!(ok.ok && ok.message.contains("1280 by 720 pixels, the size of the region"), "{}", ok.message);
        let wrong = compare_size_readback(&with(Some((1920, 1080))), (1280, 720));
        assert!(!wrong.ok && wrong.message.contains("1920 by 1080") && wrong.message.contains("region is 1280 by 720"));
        let unknown = compare_size_readback(&with(None), (1280, 720));
        assert!(unknown.ok && unknown.message.contains("so the size was not compared"), "{}", unknown.message);
        let failed = compare_size_readback(&Err(ScreenError::Failed("nope".into())), (1280, 720));
        assert!(!failed.ok && failed.message.contains("nope"));
    }
}
