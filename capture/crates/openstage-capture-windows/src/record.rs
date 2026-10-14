// SPDX-License-Identifier: Apache-2.0
//! Recording a real screen into a take, as one crash-safe MP4 file.
//!
//! [`ScreenMedia`] plugs into the session engine (`openstage-capture`) the way the test source
//! does, so countdown, pause, resume, start again, markers, and stop all work through the same
//! protocol. Each take holds one file, `screen.mp4`, written by Windows' own video tools as a
//! fragmented MP4: a setup box and then a short, self-contained fragment about every second, so a
//! recording cut off by a crash still plays up to the last whole fragment.
//!
//! Recording time does not advance while paused, so the video simply has no gap where the pause was.
//! The screen sends a picture only when something changes; the newest picture fills each beat of a
//! steady frame rate (see [`Pacer`]).

use crate::audio::{AudioPlatform, CHANNELS, SAMPLE_RATE};
use crate::region::{CroppedSource, Region};
use crate::report::{LookStats, RecordingReport, SlowMoment};
use crate::sound::{Sound, SoundPlan, SoundTake, MIXED_BITRATE_KBPS};
use crate::video::{AudioSpec, Pacer};
use crate::{Frame, FrameSource, Platform, SinkSpec, VideoPlatform, VideoSink};
use openstage_capture::{MediaFactory, SessionConfig, SessionError, TakeMedia};
use openstage_capture_project::{MediaFile, RegionInfo};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The most recording a crash is declared to cost: a fragment (about a second) plus the encoder's delay.
/// A real test on Windows 11 lost 1.1 seconds.
pub const DECLARED_LOSS_MS: u64 = 2000;

/// The file each take's picture goes into.
pub const SCREEN_FILE: &str = "screen.mp4";

fn media_error(what: impl std::fmt::Display) -> SessionError {
    SessionError::Media(what.to_string())
}

/// Starts a screen recording for each take.
pub struct ScreenMedia {
    screen: Box<dyn Platform>,
    video: Rc<dyn VideoPlatform>,
    display: u32,
    fps: u32,
    bitrate_kbps: u32,
    /// Record only this part of the display, if set.
    region: Option<Region>,
    sound: Option<Sound>,
    /// What the most recent take did, filled in when it finishes.
    report: Rc<RefCell<Option<RecordingReport>>>,
    /// What was started during the countdown, waiting for the take to begin.
    prepared: RefCell<Option<Prepared>>,
    /// Whether the video encoder has been woken up yet in this run.
    encoder_warmed: std::cell::Cell<bool>,
}

impl ScreenMedia {
    pub fn new(
        screen: Box<dyn Platform>,
        video: Box<dyn VideoPlatform>,
        display: u32,
        fps: u32,
        bitrate_kbps: u32,
    ) -> Self {
        Self {
            screen,
            video: Rc::from(video),
            display,
            fps: fps.max(1),
            bitrate_kbps,
            region: None,
            sound: None,
            report: Rc::new(RefCell::new(None)),
            prepared: RefCell::new(None),
            encoder_warmed: std::cell::Cell::new(false),
        }
    }

    /// Where the numbers about the most recent take appear once it has finished.
    pub fn report_handle(&self) -> Rc<RefCell<Option<RecordingReport>>> {
        Rc::clone(&self.report)
    }

    /// Record only this part of the display instead of all of it. The region is checked against the
    /// display when the countdown starts, and an odd width or height is made one pixel smaller.
    pub fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Record sound too: the microphone and/or the computer's own sound, as already chosen in `plan`.
    /// An empty plan records no sound.
    pub fn with_sound(mut self, platform: Box<dyn AudioPlatform>, plan: SoundPlan) -> Self {
        self.sound = (!plan.is_empty()).then(|| Sound::new(platform, plan));
        self
    }

    /// Recording this computer's real screen.
    pub fn for_this_computer(display: u32, fps: u32, bitrate_kbps: u32) -> Self {
        let screen = crate::system_platform();
        let video = crate::system_video_platform();
        Self::new(screen, video, display, fps, bitrate_kbps)
    }
}

impl MediaFactory for ScreenMedia {
    fn source_id(&self) -> String {
        format!("screen:{}", self.display)
    }

    fn media_files(&self) -> Vec<MediaFile> {
        let mut files = vec![MediaFile {
            file: SCREEN_FILE.into(),
            kind: "screen".into(),
            container: "fragmented-mp4".into(),
            mix: self.sound.as_ref().map(|s| s.plan().mix_info()),
            region: self.region.map(|r| RegionInfo {
                display: self.display,
                x: r.x,
                y: r.y,
                width: r.width - r.width % 2,
                height: r.height - r.height % 2,
            }),
        }];
        if let Some(sound) = &self.sound {
            files.extend(sound.plan().media_files());
        }
        files
    }

    fn declared_flush_ms(&self, _cfg: &SessionConfig) -> u64 {
        DECLARED_LOSS_MS
    }

    fn warm_up(&self, _cfg: &SessionConfig) -> Result<(), SessionError> {
        let started = Instant::now();
        let prepared = self.prepare()?;
        // The first video encoder of a run can take most of a second to wake up (loading the graphics
        // card's encoder). Make a throwaway file first, so the real one starts quickly.
        if !self.encoder_warmed.replace(true) {
            self.warm_encoder(&prepared);
        }
        eprintln!("warm-up during the countdown took {} ms in all", started.elapsed().as_millis());
        *self.prepared.borrow_mut() = Some(prepared);
        Ok(())
    }

    fn begin(&self, take_dir: &Path, _cfg: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
        let started = Instant::now();
        // What was prepared during the countdown, or, with no countdown, prepared now.
        let (prepared, reused) = match self.prepared.borrow_mut().take() {
            Some(p) => (p, true),
            None => (self.prepare()?, false),
        };
        let prepared_at = started.elapsed();
        let Prepared { source, first, listening, .. } = prepared;
        let dropped_at_start = source.dropped();
        let received_at_start = source.received();

        let spec = SinkSpec {
            path: take_dir.join(SCREEN_FILE),
            width: first.width,
            height: first.height,
            fps: self.fps,
            bitrate_kbps: self.bitrate_kbps,
            fragmented: true,
            audio: listening.as_ref().map(|_| AudioSpec {
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                bitrate_kbps: MIXED_BITRATE_KBPS,
            }),
        };
        let sink =
            self.video.create_sink(&spec).map_err(|e| media_error(format!("Could not start the video file: {e}")))?;
        let memory_now = self.video.process_memory_mb();
        let mut sound = match listening {
            Some(open) => Some(open.begin(take_dir)?),
            None => None,
        };
        // Recording time starts now. Whatever the microphone heard while everything was starting is not part of it.
        if let Some(sound) = sound.as_mut() {
            sound.discard_backlog();
        }
        // Where the start-up time goes, for diagnosis. Standard error is for diagnostics only.
        eprintln!(
            "start-up: {} {} ms, then the video file was ready after {} ms in all (this is the pause after the countdown)",
            if reused { "preparing had been done during the countdown; collecting it took" } else { "preparing took" },
            prepared_at.as_millis(),
            started.elapsed().as_millis()
        );
        Ok(Box::new(ScreenTake {
            source: Some(source),
            sink: Some(sink),
            pacer: Pacer::new(self.fps),
            latest: first,
            written: 0,
            sound,
            video: Rc::clone(&self.video),
            report: Rc::clone(&self.report),
            started: Instant::now(),
            cpu_start: self.video.process_cpu_time(),
            memory_first_mb: memory_now,
            memory_peak_mb: memory_now,
            next_sample_ms: SAMPLE_EVERY_MS,
            pictures_replaced: 0,
            dropped_at_start,
            received_at_start,
            pictures_received: 0,
            most_waiting: 0,
            looks: LookTracker::default(),
        }))
    }
}

/// Everything that can be started before the take's folder is known.
struct Prepared {
    source: Box<dyn FrameSource>,
    first: Frame,
    listening: Option<crate::sound::OpenSound>,
}

impl ScreenMedia {
    /// Start Windows' video tools, the screen capture, and the sound. Waits for the first picture.
    fn prepare(&self) -> Result<Prepared, SessionError> {
        let started = Instant::now();
        self.video.start_media().map_err(|e| media_error(format!("Windows' video tools would not start: {e}")))?;
        let tools_ready = started.elapsed();
        let displays = self.screen.list_displays().map_err(media_error)?;
        let display = displays.iter().find(|d| d.number == self.display).ok_or_else(|| {
            media_error(format!("There is no Display {}. Choose a number from 1 to {}.", self.display, displays.len()))
        })?;
        // A region is checked against the display before anything is started.
        let region = match self.region {
            Some(r) => Some(r.fit(display.width, display.height).map_err(media_error)?.0),
            None => None,
        };
        let mut source = self
            .screen
            .start_display_capture(display, false)
            .map_err(|e| media_error(format!("Could not start capturing {}: {e}", display.describe())))?;
        if let Some(region) = region {
            source = Box::new(CroppedSource::new(source, region));
        }
        let capture_started = started.elapsed();
        let first = source.next_frame(Duration::from_secs(3)).ok_or_else(|| {
            media_error(
                "No picture arrived from the screen within 3 seconds. Windows only sends a picture when something changes: try again while moving a window.",
            )
        })?;
        let first_picture = started.elapsed();
        if first.width % 2 != 0 || first.height % 2 != 0 {
            return Err(media_error(format!(
                "The display is {} by {} pixels. H.264 video needs an even width and height, and trimming is not built yet.",
                first.width, first.height
            )));
        }
        // Start listening before the video file exists, so a microphone that cannot be opened leaves nothing behind.
        let listening = match &self.sound {
            Some(sound) => Some(sound.open()?),
            None => None,
        };
        eprintln!(
            "preparing: video tools {} ms, screen capture started {} ms, first picture {} ms, sound listening {} ms (counted from the start of preparing)",
            tools_ready.as_millis(),
            capture_started.as_millis(),
            first_picture.as_millis(),
            started.elapsed().as_millis()
        );
        Ok(Prepared { source, first, listening })
    }

    /// Make and throw away a small video file, so the encoder is awake when the real one is made. Any
    /// trouble is ignored: this only saves time, and the real file reports its own problems.
    fn warm_encoder(&self, prepared: &Prepared) {
        let started = Instant::now();
        let path = std::env::temp_dir().join(format!("openstage-capture-warmup-{}.mp4", std::process::id()));
        let spec = SinkSpec {
            path: path.clone(),
            width: prepared.first.width,
            height: prepared.first.height,
            fps: self.fps,
            bitrate_kbps: self.bitrate_kbps,
            fragmented: true,
            audio: prepared.listening.as_ref().map(|_| AudioSpec {
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                bitrate_kbps: MIXED_BITRATE_KBPS,
            }),
        };
        match self.video.create_sink(&spec) {
            Ok(mut sink) => {
                let step = Pacer::new(self.fps);
                let _ = sink.write(&prepared.first, 0, step.duration_100ns(0));
                if spec.audio.is_some() {
                    let _ = sink.write_audio(&vec![0i16; (SAMPLE_RATE as usize / 50) * CHANNELS as usize], 0);
                }
                let _ = sink.finish();
            }
            Err(e) => eprintln!("warming up the video encoder did not work (this is not a problem by itself): {e}"),
        }
        let _ = std::fs::remove_file(&path);
        eprintln!("video encoder warmed up in {} ms", started.elapsed().as_millis());
    }
}

/// One take being recorded.
struct ScreenTake {
    source: Option<Box<dyn FrameSource>>,
    sink: Option<Box<dyn VideoSink>>,
    pacer: Pacer,
    /// The newest picture of the screen; repeated while the screen does not change.
    latest: Frame,
    written: u64,
    sound: Option<SoundTake>,
    // What the report needs.
    video: Rc<dyn VideoPlatform>,
    report: Rc<RefCell<Option<RecordingReport>>>,
    started: Instant,
    cpu_start: Option<Duration>,
    memory_first_mb: Option<f64>,
    memory_peak_mb: Option<f64>,
    next_sample_ms: u64,
    pictures_replaced: u64,
    /// Pictures Windows had already thrown away before recording began (while the countdown ran, nobody was taking them).
    dropped_at_start: u64,
    /// Pictures delivered before recording began, for the same reason.
    received_at_start: u64,
    pictures_received: u64,
    /// The most pictures that were waiting at one look.
    most_waiting: usize,
    looks: LookTracker,
}

/// Keeps track of how regularly the program looks at the screen and the sound, and what it was busy
/// with when it went a long time without.
#[derive(Default)]
struct LookTracker {
    last: Option<Instant>,
    count: u64,
    total_gap: Duration,
    longest: Duration,
    over_50: u64,
    over_100: u64,
    slow: Vec<SlowMoment>,
    /// How long the previous look took for the picture and for the sound.
    last_picture: Duration,
    last_sound: Duration,
    /// Of that, how long writing sound into the video file took.
    last_sound_video: Duration,
    /// How long saving to disk has taken since the previous look.
    saving_since: Duration,
}

/// How many of the longest waits are remembered.
const SLOW_MOMENTS_KEPT: usize = 6;

impl LookTracker {
    /// The program looks now, at recording time `media_ms`.
    fn looked_at(&mut self, now: Instant, media_ms: u64) {
        if let Some(previous) = self.last {
            let gap = now.saturating_duration_since(previous);
            self.count += 1;
            self.total_gap += gap;
            self.longest = self.longest.max(gap);
            self.over_50 += (gap > Duration::from_millis(50)) as u64;
            if gap > Duration::from_millis(100) {
                self.over_100 += 1;
                self.slow.push(SlowMoment {
                    at_ms: media_ms,
                    gap_ms: gap.as_millis() as u64,
                    picture_ms: self.last_picture.as_millis() as u64,
                    sound_ms: self.last_sound.as_millis() as u64,
                    sound_video_ms: self.last_sound_video.as_millis() as u64,
                    saving_ms: self.saving_since.as_millis() as u64,
                });
                self.slow.sort_by_key(|m| std::cmp::Reverse(m.gap_ms));
                self.slow.truncate(SLOW_MOMENTS_KEPT);
            }
        }
        self.last = Some(now);
        self.saving_since = Duration::ZERO;
    }

    /// Recording was paused: the long wait until the next look is the pause, not a stall.
    fn forget(&mut self) {
        self.last = None;
    }

    fn stats(self) -> LookStats {
        LookStats {
            looks: self.count,
            average_gap_ms: if self.count == 0 {
                0.0
            } else {
                self.total_gap.as_secs_f64() * 1000.0 / self.count as f64
            },
            longest_gap_ms: self.longest.as_millis() as u64,
            gaps_over_50_ms: self.over_50,
            gaps_over_100_ms: self.over_100,
            slow_moments: self.slow,
        }
    }
}

/// How often memory use is looked at, in recording time.
const SAMPLE_EVERY_MS: u64 = 10_000;
/// How often a line about resource use is written for the log, in recording time.
const LOG_EVERY_MS: u64 = 60_000;

impl ScreenTake {
    /// Look at how much memory the program uses, and now and then say so in the log.
    fn sample_resources(&mut self, media_ms: u64) {
        if let Some(mb) = self.video.process_memory_mb() {
            self.memory_first_mb.get_or_insert(mb);
            self.memory_peak_mb = Some(self.memory_peak_mb.map_or(mb, |p| p.max(mb)));
        }
        if media_ms % LOG_EVERY_MS < SAMPLE_EVERY_MS {
            let cpu =
                self.cpu_start.zip(self.video.process_cpu_time()).map(|(a, b)| (b.saturating_sub(a)).as_secs_f64());
            let wall = self.started.elapsed().as_secs_f64().max(0.001);
            eprintln!(
                "resources at {}: memory {} MB, processor {}",
                crate::report::clock(media_ms),
                self.memory_peak_mb.map_or("unknown".to_string(), |m| format!("{m:.0}")),
                cpu.map_or("unknown".to_string(), |c| format!("{:.0} percent of one core so far", c * 100.0 / wall)),
            );
        }
    }

    fn write_beat(&mut self, beat: u64) -> Result<(), SessionError> {
        let sink = self.sink.as_mut().expect("the video file is open while recording");
        sink.write(&self.latest, self.pacer.pts_100ns(beat), self.pacer.duration_100ns(beat))
            .map_err(|e| media_error(format!("Writing the video stopped with a problem: {e}")))?;
        self.written += 1;
        Ok(())
    }
}

impl TakeMedia for ScreenTake {
    fn pump(&mut self, media_ms: u64) -> Result<(), SessionError> {
        self.looks.looked_at(Instant::now(), media_ms);
        let picture_started = Instant::now();
        if let Some(source) = self.source.as_mut() {
            let mut waiting = 0;
            while let Some(newer) = source.next_frame(Duration::ZERO) {
                self.latest = newer;
                waiting += 1;
            }
            self.most_waiting = self.most_waiting.max(waiting);
        }
        if let Some(beat) = self.pacer.due(Duration::from_millis(media_ms)) {
            self.write_beat(beat)?;
        }
        self.looks.last_picture = picture_started.elapsed();
        let sound_started = Instant::now();
        if let (Some(sound), Some(sink)) = (self.sound.as_mut(), self.sink.as_mut()) {
            sound.pump(media_ms, sink.as_mut())?;
            self.looks.last_sound_video = sound.take_video_cost();
        }
        self.looks.last_sound = sound_started.elapsed();
        if media_ms >= self.next_sample_ms {
            self.next_sample_ms = media_ms - media_ms % SAMPLE_EVERY_MS + SAMPLE_EVERY_MS;
            self.sample_resources(media_ms);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SessionError> {
        // Windows' writer closes a fragment about once a second by itself; the separate sound files are
        // pushed to the disk here, with their lengths recorded, so a crash costs about a second of them.
        let started = Instant::now();
        let result = match self.sound.as_mut() {
            Some(sound) => sound.sync(),
            None => Ok(()),
        };
        self.looks.saving_since += started.elapsed();
        result
    }

    fn levels(&self, _media_ms: u64) -> Vec<(&'static str, f32)> {
        self.sound.as_ref().map(|s| s.levels()).unwrap_or_default()
    }

    fn paused(&mut self) {
        self.looks.forget();
        // The microphone keeps running while recording time stands still; what it hears is not kept.
        if let Some(sound) = self.sound.as_mut() {
            sound.pause();
        }
    }

    fn resumed(&mut self) {
        self.looks.forget();
        if let Some(sound) = self.sound.as_mut() {
            sound.resume();
        }
    }

    fn finish(mut self: Box<Self>, media_ms: u64) -> Result<u32, SessionError> {
        // Stop taking pictures first, so nothing arrives while the file is being completed.
        self.pictures_replaced = self.source.as_ref().map_or(0, |s| s.dropped()).saturating_sub(self.dropped_at_start);
        self.pictures_received =
            self.source.as_ref().map_or(0, |s| s.received()).saturating_sub(self.received_at_start);
        self.source = None;
        if self.written == 0 {
            // A recording stopped at once still gets one picture, so the file is a valid video.
            self.write_beat(0)?;
        }
        let mut sink = self.sink.take().expect("the video file is open until it is finished");
        let mut files = 1;
        let mut tracks = Vec::new();
        if let Some(sound) = self.sound.take() {
            files += sound.track_count();
            tracks = sound.finish(media_ms, sink.as_mut())?;
        }
        self.sample_resources(media_ms);
        *self.report.borrow_mut() = Some(RecordingReport {
            recorded_ms: media_ms,
            pictures_written: self.written,
            beats_skipped: self.pacer.skipped(),
            pictures_replaced: self.pictures_replaced,
            pictures_received: self.pictures_received,
            most_waiting: self.most_waiting,
            wall_seconds: self.started.elapsed().as_secs_f64(),
            cpu_seconds: self
                .cpu_start
                .zip(self.video.process_cpu_time())
                .map(|(a, b)| b.saturating_sub(a).as_secs_f64()),
            logical_cores: self.video.logical_cores(),
            memory_first_mb: self.memory_first_mb,
            memory_peak_mb: self.memory_peak_mb,
            tracks,
            looks: std::mem::take(&mut self.looks).stats(),
        });
        sink.finish().map_err(|e| media_error(format!("The video file could not be completed: {e}")))?;
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayInfo, EncoderInfo, ScreenError, VideoInspection, WindowsVersion};
    use openstage_capture::{run_session, Input, ManualClock, ScriptedInput, SessionConfig, VecSink};
    use openstage_capture_project::{read_take, take_dir, TakeState};
    use openstage_capture_protocol::{Command, Event};
    use std::cell::RefCell;
    use std::rc::Rc;

    // ---- fakes ----

    struct StillSource {
        picture: Option<Frame>,
    }
    impl FrameSource for StillSource {
        fn next_frame(&mut self, _timeout: Duration) -> Option<Frame> {
            self.picture.take()
        }
        fn dropped(&self) -> u64 {
            0
        }
    }

    struct FakeScreen {
        width: u32,
        height: u32,
        gives_picture: bool,
        /// Make every pixel say where it is (blue is the column, green the row), instead of all the same.
        positional: bool,
        /// How many times a capture of the screen was started.
        captures: Rc<std::cell::Cell<u32>>,
    }
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
                width: self.width,
                height: self.height,
                main: true,
                system_name: "D1".into(),
                handle: 1,
            }])
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            self.captures.set(self.captures.get() + 1);
            let picture = self.gives_picture.then(|| Frame {
                width: self.width,
                height: self.height,
                t_ms: 0,
                captured_100ns: None,
                arrived_100ns: None,
                bgra: if self.positional {
                    (0..self.height)
                        .flat_map(|r| (0..self.width).flat_map(move |c| [c as u8, r as u8, 7, 255]))
                        .collect()
                } else {
                    vec![9; self.width as usize * self.height as usize * 4]
                },
            });
            Ok(Box::new(StillSource { picture }))
        }
    }

    /// What each fake video file received: (pts, duration) of every beat, and whether it was completed.
    #[derive(Default)]
    struct File {
        beats: Vec<(i64, i64)>,
        finished: bool,
        fragmented: bool,
        path: std::path::PathBuf,
        /// Whether the file was asked to have a sound track, and what was written to it: (start, frames).
        sound_track: bool,
        sound: Vec<(i64, usize)>,
        /// The size the file was made for, and the size and first pixel of every picture written to it.
        size: (u32, u32),
        pictures: Vec<(u32, u32, [u8; 4])>,
    }

    #[derive(Default)]
    struct Files(Rc<RefCell<Vec<Rc<RefCell<File>>>>>);

    struct FakeSink(Rc<RefCell<File>>);
    impl VideoSink for FakeSink {
        fn write(&mut self, frame: &Frame, pts: i64, dur: i64) -> Result<(), ScreenError> {
            let mut file = self.0.borrow_mut();
            file.beats.push((pts, dur));
            file.pictures.push((
                frame.width,
                frame.height,
                [frame.bgra[0], frame.bgra[1], frame.bgra[2], frame.bgra[3]],
            ));
            Ok(())
        }
        fn write_audio(&mut self, samples: &[i16], pts: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().sound.push((pts, samples.len() / 2));
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            self.0.borrow_mut().finished = true;
            Ok(())
        }
    }

    struct FakeVideo {
        files: Rc<RefCell<Vec<Rc<RefCell<File>>>>>,
        /// Memory in use; grows by 10 megabytes every time it is looked at.
        memory: std::cell::Cell<f64>,
    }
    impl VideoPlatform for FakeVideo {
        fn start_media(&self) -> Result<(), ScreenError> {
            Ok(())
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(vec![])
        }
        fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            let file = Rc::new(RefCell::new(File {
                fragmented: spec.fragmented,
                path: spec.path.clone(),
                sound_track: spec.audio.is_some(),
                size: (spec.width, spec.height),
                ..File::default()
            }));
            self.files.borrow_mut().push(Rc::clone(&file));
            Ok(Box::new(FakeSink(file)))
        }
        fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
            Err(ScreenError::Failed("not used here".into()))
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            None
        }
        fn logical_cores(&self) -> u32 {
            1
        }
        fn process_memory_mb(&self) -> Option<f64> {
            let m = self.memory.get();
            self.memory.set(m + 10.0);
            Some(m)
        }
    }

    fn factory(screen: FakeScreen) -> (ScreenMedia, Files) {
        let files = Files::default();
        let video = FakeVideo { files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) };
        (ScreenMedia::new(Box::new(screen), Box::new(video), 1, 30, 8000), files)
    }

    fn good_screen() -> FakeScreen {
        FakeScreen {
            width: 8,
            height: 4,
            gives_picture: true,
            positional: false,
            captures: Rc::new(std::cell::Cell::new(0)),
        }
    }

    fn config(dir: &tempfile::TempDir) -> SessionConfig {
        let mut cfg = SessionConfig::new(dir.path().join("lesson.capture"));
        cfg.countdown_ms = 0;
        cfg
    }

    // ---- tests ----

    // ---- a region of the display ----

    fn wide_screen() -> FakeScreen {
        FakeScreen { width: 200, height: 100, positional: true, ..good_screen() }
    }

    #[test]
    fn a_region_records_only_that_part_of_the_display_and_says_so_in_take_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(1000);
        let (media, files) = factory(wide_screen());
        let media = media.with_region(Region { x: 30, y: 20, width: 64, height: 64 });
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new())
            .unwrap();
        assert!(out.ok);

        let files = files.0.borrow();
        let f = files[0].borrow();
        assert_eq!(f.size, (64, 64), "the video is the size of the region");
        assert!(!f.pictures.is_empty());
        assert!(
            f.pictures.iter().all(|p| *p == (64, 64, [30, 20, 7, 255])),
            "every picture starts at the region's corner of the display"
        );

        let region = read_take(&cfg.project, 1).unwrap().media[0].region.expect("the region is written down");
        assert_eq!(region, RegionInfo { display: 1, x: 30, y: 20, width: 64, height: 64 });
    }

    #[test]
    fn a_region_with_an_odd_size_is_recorded_one_pixel_smaller_and_written_down_as_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(500);
        let (media, files) = factory(wide_screen());
        let media = media.with_region(Region { x: 0, y: 0, width: 65, height: 67 });
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(files.0.borrow()[0].borrow().size, (64, 66));
        let region = read_take(&cfg.project, 1).unwrap().media[0].region.unwrap();
        assert_eq!((region.width, region.height), (64, 66), "the file says what the video really shows");
    }

    #[test]
    fn a_region_past_the_edge_ends_the_session_with_a_plain_message_and_makes_no_video() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = factory(wide_screen());
        let media = media.with_region(Region { x: 150, y: 0, width: 100, height: 64 });
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        let message = sink
            .events
            .iter()
            .find_map(|e| match e {
                Event::Error { message, .. } => Some(message.clone()),
                _ => None,
            })
            .expect("an error event");
        assert!(
            message.contains("reaches past the edge of the display") && message.contains("200 by 100"),
            "{message}"
        );
        assert!(files.0.borrow().is_empty(), "no video file was made");
    }

    #[test]
    fn without_a_region_the_whole_display_is_recorded_and_no_region_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(500);
        let (media, files) = factory(wide_screen());
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(files.0.borrow()[0].borrow().size, (200, 100));
        assert_eq!(read_take(&cfg.project, 1).unwrap().media[0].region, None);
        let text = std::fs::read_to_string(take_dir(&cfg.project, 1).join("take.json")).unwrap();
        assert!(!text.contains("region"), "{text}");
    }

    #[test]
    fn a_screen_take_is_one_fragmented_mp4_listed_in_take_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(3000);
        let (media, files) = factory(good_screen());
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new())
            .unwrap();
        assert_eq!(
            (out.ok, out.take, out.state.as_str(), out.duration_ms, out.segments),
            (true, 1, "complete", 3000, 1)
        );

        let take = read_take(&cfg.project, 1).unwrap();
        assert_eq!(take.state, TakeState::Complete);
        assert_eq!(take.sources, vec!["screen:1".to_string()]);
        assert_eq!(take.flush_ms, DECLARED_LOSS_MS, "the take declares how much a crash can cost");
        assert_eq!(take.media.len(), 1);
        assert_eq!((take.media[0].file.as_str(), take.media[0].container.as_str()), ("screen.mp4", "fragmented-mp4"));
        assert!(!take_dir(&cfg.project, 1).join("segments").exists());

        let files = files.0.borrow();
        assert_eq!(files.len(), 1);
        let f = files[0].borrow();
        assert!(f.finished && f.fragmented, "completed, and written as a crash-safe fragmented file");
        assert_eq!(f.path, take_dir(&cfg.project, 1).join("screen.mp4"));
        assert!((88..=91).contains(&f.beats.len()), "30 pictures a second for 3 seconds, got {}", f.beats.len());
        assert!(f.beats.windows(2).all(|p| p[1].0 > p[0].0), "time only moves forward");
    }

    #[test]
    fn pausing_leaves_no_gap_in_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = factory(good_screen());
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (2000, Input::Command(Command::Resume)),
            (3000, Input::Command(Command::Stop)),
        ]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!((1980..=2040).contains(&out.duration_ms), "2 of 3 seconds were recorded, got {}", out.duration_ms);

        let files = files.0.borrow();
        let f = files[0].borrow();
        let last_pts = f.beats.last().unwrap().0;
        assert!((19_500_000..=20_500_000).contains(&last_pts), "the video is about 2 seconds long, got {last_pts}");
        let biggest_gap = f.beats.windows(2).map(|p| p[1].0 - p[0].0).max().unwrap();
        assert!(biggest_gap < 700_000, "no jump where the pause was (largest step {biggest_gap})");
    }

    #[test]
    fn start_again_completes_the_first_video_and_starts_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = factory(good_screen());
        let mut input =
            ScriptedInput::new(vec![(1500, Input::Command(Command::Restart)), (2500, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert_eq!((out.take, out.ok), (2, true));

        assert_eq!(read_take(&cfg.project, 1).unwrap().state, TakeState::SetAside);
        assert_eq!(read_take(&cfg.project, 2).unwrap().state, TakeState::Complete);
        let files = files.0.borrow();
        assert_eq!(files.len(), 2, "each take has its own video file");
        assert!(files.iter().all(|f| f.borrow().finished), "the set-aside take's video was completed, not abandoned");
        assert_ne!(files[0].borrow().path, files[1].borrow().path);
        assert!(sink.events.contains(&Event::TakeSetAside { take: 1, next_take: 2 }));
    }

    #[test]
    fn a_recording_stopped_at_once_still_gets_a_valid_video() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = factory(good_screen());
        let mut input = ScriptedInput::new(vec![(0, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(out.ok);
        let files = files.0.borrow();
        let f = files[0].borrow();
        assert!(f.finished);
        assert!(!f.beats.is_empty(), "at least one picture, so the file can be played");
    }

    #[test]
    fn a_screen_that_sends_no_picture_ends_the_session_with_a_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, _files) = factory(FakeScreen { gives_picture: false, ..good_screen() });
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        let error = sink.events.iter().find_map(|e| match e {
            Event::Error { code, message, fatal } => Some((code.clone(), message.clone(), *fatal)),
            _ => None,
        });
        let (code, message, fatal) = error.expect("an error event");
        assert_eq!(code, "media-failed");
        assert!(fatal && message.contains("No picture arrived from the screen within 3 seconds"), "{message}");
    }

    #[test]
    fn an_odd_sized_display_is_explained() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, _files) = factory(FakeScreen { width: 7, ..good_screen() });
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        assert!(sink
            .events
            .iter()
            .any(|e| matches!(e, Event::Error { message, .. } if message.contains("even width and height"))));
    }

    #[test]
    fn a_display_that_does_not_exist_is_explained() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let files = Files::default();
        let video = FakeVideo { files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) };
        let media = ScreenMedia::new(Box::new(good_screen()), Box::new(video), 4, 30, 8000);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        assert!(sink
            .events
            .iter()
            .any(|e| matches!(e, Event::Error { message, .. } if message.contains("There is no Display 4"))));
    }

    #[test]
    fn the_recording_clock_starts_when_the_video_is_ready_not_before() {
        // Starting capture takes real time on a real computer. That time must not become video.
        struct SlowStart {
            inner: ScreenMedia,
            clock: Rc<ManualClock>,
        }
        impl MediaFactory for SlowStart {
            fn source_id(&self) -> String {
                self.inner.source_id()
            }
            fn media_files(&self) -> Vec<MediaFile> {
                self.inner.media_files()
            }
            fn begin(&self, d: &Path, c: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                use openstage_capture::Clock;
                self.clock.sleep_ms(1500); // the encoder taking its time to start
                self.inner.begin(d, c)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (inner, _files) = factory(good_screen());
        let clock = Rc::new(ManualClock::new());
        let media = SlowStart { inner, clock: Rc::clone(&clock) };
        // Stop one second after the recording began. (No maximum length is set: a maximum would hide the problem.)
        let mut input = ScriptedInput::new(vec![(1000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!(
            (980..=1040).contains(&out.duration_ms),
            "about the one second recorded, not the 1.5 seconds spent starting too: got {}",
            out.duration_ms
        );
    }

    // ---- sound ----

    use crate::audio::{AudioChoice, AudioPlatform};
    use crate::sound::fakes::FakePlatform;
    use crate::sound::plan_sound;
    use openstage_capture_project::wav_info;

    /// A screen with sound: the fake computer has a microphone and a sound output, both following the clock.
    fn with_sound(
        clock: &Rc<ManualClock>,
        tune: impl FnOnce(&mut FakePlatform),
        microphone: Option<&str>,
        output: Option<&str>,
    ) -> (ScreenMedia, Files) {
        let mut platform = FakePlatform::new(clock);
        tune(&mut platform);
        let plan =
            plan_sound(&platform, microphone.map(AudioChoice::parse).as_ref(), output.map(AudioChoice::parse).as_ref())
                .unwrap();
        let (media, files) = factory(good_screen());
        (media.with_sound(Box::new(platform) as Box<dyn AudioPlatform>, plan), files)
    }

    #[test]
    fn a_take_with_sound_holds_the_video_and_two_sound_files_and_says_so_in_take_json() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_sound(&clock, |_| {}, Some("default"), Some("default"));
        let mut input = ScriptedInput::new(vec![(3000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!(out.ok);
        assert_eq!(out.segments, 3, "the video and the two sound files");

        let take = read_take(&cfg.project, 1).unwrap();
        let listed: Vec<(&str, &str, &str)> =
            take.media.iter().map(|m| (m.file.as_str(), m.kind.as_str(), m.container.as_str())).collect();
        assert_eq!(
            listed,
            vec![
                ("screen.mp4", "screen", "fragmented-mp4"),
                ("mic.wav", "microphone", "wav"),
                ("system.wav", "system-audio", "wav")
            ]
        );
        let dir1 = take_dir(&cfg.project, 1);
        for name in ["mic.wav", "system.wav"] {
            let info = wav_info(&dir1.join(name)).unwrap();
            assert!((141_000..=144_500).contains(&info.frames), "{name}: about 3 seconds, got {} frames", info.frames);
        }

        let files = files.0.borrow();
        let f = files[0].borrow();
        assert!(f.sound_track, "the video file was made with a sound track");
        let frames: usize = f.sound.iter().map(|c| c.1).sum();
        assert!((141_000..=144_500).contains(&frames), "the video holds about 3 seconds of sound, got {frames}");
        assert_eq!(f.sound[0].0, 0, "the sound starts with the picture");
    }

    #[test]
    fn a_take_without_sound_has_no_sound_track_and_no_sound_files() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = factory(good_screen());
        let mut input = ScriptedInput::new(vec![(1000, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(!files.0.borrow()[0].borrow().sound_track);
        assert!(!take_dir(&cfg.project, 1).join("mic.wav").exists());
        assert_eq!(read_take(&cfg.project, 1).unwrap().media.len(), 1);
    }

    #[test]
    fn sound_heard_during_a_pause_never_reaches_the_recording() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        // Something loud happens in the room exactly while recording is paused (1 to 2 seconds in).
        let pauses = Rc::new(RefCell::new(Vec::new()));
        let (media, files) = with_sound(
            &clock,
            |p| {
                p.mic = Rc::new(|ms| if (1000..2000).contains(&ms) { 20000 } else { 1000 });
                p.pauses = Rc::clone(&pauses);
            },
            Some("default"),
            None,
        );
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (2000, Input::Command(Command::Resume)),
            (3000, Input::Command(Command::Stop)),
        ]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!((1980..=2040).contains(&out.duration_ms), "{}", out.duration_ms);

        let mic = wav_info(&take_dir(&cfg.project, 1).join("mic.wav")).unwrap();
        assert!(
            mic.frames.abs_diff(out.duration_ms * 48) <= 4_000,
            "the sound is as long as the recording ({} ms), got {} frames",
            out.duration_ms,
            mic.frames
        );
        assert!(mic.peak < 0.05, "the loud sound from the pause is not in the file (peak {})", mic.peak);
        assert_eq!(*pauses.borrow(), vec![true, false], "the source is told to stop keeping sound, then to carry on");
        let files = files.0.borrow();
        let f = files[0].borrow();
        let frames: usize = f.sound.iter().map(|c| c.1).sum();
        assert!((frames as u64).abs_diff(mic.frames) <= 100, "the video's sound matches the file");
    }

    #[test]
    fn sound_heard_while_the_recorder_was_starting_is_not_part_of_the_recording() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, _files) =
            with_sound(&clock, |p| p.mic_backlog = vec![30000; 48_000 * 2 * 2], Some("default"), None);
        let mut input = ScriptedInput::new(vec![(1000, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        let mic = wav_info(&take_dir(&cfg.project, 1).join("mic.wav")).unwrap();
        assert!(mic.frames <= 50_000, "one second, not three: {} frames", mic.frames);
        assert!(mic.peak < 0.05, "none of the start-up sound is in the file (peak {})", mic.peak);
    }

    #[test]
    fn a_microphone_that_cannot_be_opened_ends_the_session_in_words_and_makes_no_video() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_sound(
            &clock,
            |p| p.mic_error = Some("another program is using it exclusively"),
            Some("default"),
            Some("default"),
        );
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, clock.as_ref()).unwrap();
        assert!(!out.ok);
        let message = sink
            .events
            .iter()
            .find_map(|e| match e {
                Event::Error { code, message, .. } if code == "media-failed" => Some(message.clone()),
                _ => None,
            })
            .expect("a media-failed error");
        assert!(message.contains("Could not listen to the microphone") && message.contains("exclusively"), "{message}");
        assert!(files.0.borrow().is_empty(), "no video file was created for a take that could not start");
    }

    #[test]
    fn sound_levels_are_reported_while_recording_in_words() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, _files) = with_sound(&clock, |p| p.mic = Rc::new(|_| 8000), Some("default"), Some("default"));
        let mut input = ScriptedInput::new(vec![(2000, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        run_session(&cfg, &media, &mut input, &mut sink, clock.as_ref()).unwrap();
        let mic: Vec<&str> = sink
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Level { source, text, .. } if source == "mic" => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let system: Vec<&str> = sink
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Level { source, text, .. } if source == "system" => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(mic.len() >= 3 && mic.iter().all(|t| *t == "good"), "{mic:?}");
        assert!(system.len() >= 3 && system.iter().all(|t| *t == "quiet"), "{system:?}");
    }

    #[test]
    fn take_json_says_how_the_sound_inside_the_video_was_mixed() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let platform = FakePlatform::new(&clock);
        let plan = plan_sound(&platform, Some(&AudioChoice::Default), Some(&AudioChoice::Default))
            .unwrap()
            .with_gains(2, -9)
            .unwrap();
        let (media, _files) = factory(good_screen());
        let media = media.with_sound(Box::new(platform) as Box<dyn AudioPlatform>, plan);
        let mut input = ScriptedInput::new(vec![(500, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        let take = read_take(&cfg.project, 1).unwrap();
        let mix = take.media[0].mix.expect("the video says how its sound was mixed");
        assert_eq!((mix.microphone_gain_db, mix.system_gain_db), (2, -9));
        assert!(take.media[1..].iter().all(|m| m.mix.is_none()), "the separate files are untouched");
    }

    #[test]
    fn what_a_recording_did_is_collected_for_the_summary() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, _files) = with_sound(&clock, |_| {}, Some("default"), Some("default"));
        let report = media.report_handle();
        assert!(report.borrow().is_none(), "nothing before a take has finished");
        let mut input = ScriptedInput::new(vec![(25_000, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();

        let r = report.borrow().clone().expect("a report once the take has finished");
        assert!((24_900..=25_100).contains(&r.recorded_ms), "{}", r.recorded_ms);
        assert!(
            (740..=752).contains(&r.pictures_written),
            "30 pictures a second for 25 seconds: {}",
            r.pictures_written
        );
        assert_eq!(r.logical_cores, 1);
        assert_eq!(r.cpu_seconds, None, "the fake computer cannot say, so none is claimed");
        assert_eq!(r.memory_first_mb, Some(100.0));
        assert!(r.memory_peak_mb.unwrap() >= 130.0, "memory was looked at every 10 seconds: {:?}", r.memory_peak_mb);
        assert_eq!(r.tracks.len(), 2);
        assert!(r.tracks[0].microphone && !r.tracks[1].microphone);
        assert!(r.tracks.iter().all(|t| t.heard_ms.abs_diff(r.recorded_ms) < 100), "{:?}", r.tracks);

        let checks = crate::summarize(&r);
        let names: Vec<_> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(names, ["run-summary", "run-stalls", "run-memory", "run-sound-microphone", "run-sound-system"]);
    }

    #[test]
    fn a_second_take_replaces_the_report_of_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, _files) = factory(good_screen());
        let report = media.report_handle();
        let mut input =
            ScriptedInput::new(vec![(1500, Input::Command(Command::Restart)), (3500, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        let r = report.borrow().clone().unwrap();
        assert!((1900..=2100).contains(&r.recorded_ms), "the last take, about 2 seconds: {}", r.recorded_ms);
    }

    // ---- warming up during the countdown ----

    #[test]
    fn during_a_countdown_the_screen_and_encoder_are_prepared_and_the_take_uses_them() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 2000;
        let screen = good_screen();
        let captures = Rc::clone(&screen.captures);
        let (media, files) = factory(screen);
        let mut input = ScriptedInput::new(vec![(3000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(out.ok);

        assert_eq!(captures.get(), 1, "the screen capture started once, during the countdown, and the take reused it");
        let files = files.0.borrow();
        assert_eq!(files.len(), 2, "a throwaway file to wake the encoder, then the real one");
        let (warm, real) = (files[0].borrow(), files[1].borrow());
        assert!(warm.path.to_string_lossy().contains("openstage-capture-warmup") && warm.finished);
        assert!(!warm.path.starts_with(&cfg.project), "the throwaway file is never inside the project");
        assert_eq!(real.path, take_dir(&cfg.project, 1).join("screen.mp4"));
        assert!(real.finished && !real.beats.is_empty());
    }

    #[test]
    fn with_no_countdown_everything_is_prepared_at_the_start_of_the_take_and_nothing_is_thrown_away() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let screen = good_screen();
        let captures = Rc::clone(&screen.captures);
        let (media, files) = factory(screen);
        let mut input = ScriptedInput::new(vec![(500, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(captures.get(), 1);
        assert_eq!(files.0.borrow().len(), 1, "no throwaway file when there was no countdown to do it in");
    }

    #[test]
    fn start_again_gets_a_fresh_capture_for_the_new_take_but_wakes_the_encoder_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 1000;
        let screen = good_screen();
        let captures = Rc::clone(&screen.captures);
        let (media, files) = factory(screen);
        let mut input =
            ScriptedInput::new(vec![(2500, Input::Command(Command::Restart)), (6000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(out.take, 2);
        assert_eq!(captures.get(), 2, "one capture for each take");
        let files = files.0.borrow();
        assert_eq!(files.len(), 3, "one throwaway file in all, then a real file for each take");
        assert!(files[0].borrow().path.to_string_lossy().contains("warmup"));
        assert!(!files[1].borrow().path.to_string_lossy().contains("warmup"));
        assert!(!files[2].borrow().path.to_string_lossy().contains("warmup"));
    }

    #[test]
    fn a_screen_that_cannot_be_recorded_is_reported_during_the_countdown_before_any_take_exists() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 3000;
        let (media, files) = factory(FakeScreen { width: 7, ..good_screen() });
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        assert_eq!(out.take, 0, "no take was started");
        assert!(sink
            .events
            .iter()
            .any(|e| matches!(e, Event::Error { message, .. } if message.contains("even width and height"))));
        assert!(files.0.borrow().is_empty());
        assert!(!take_dir(&cfg.project, 1).exists(), "no take folder was left behind");
    }

    #[test]
    fn a_warm_up_that_cannot_make_its_throwaway_file_does_not_stop_the_recording() {
        struct NoWarmFile(FakeVideo, std::cell::Cell<u32>);
        impl VideoPlatform for NoWarmFile {
            fn start_media(&self) -> Result<(), ScreenError> {
                self.0.start_media()
            }
            fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
                self.0.h264_encoders()
            }
            fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
                self.1.set(self.1.get() + 1);
                if self.1.get() == 1 {
                    return Err(ScreenError::Failed("the temporary folder is full".into()));
                }
                self.0.create_sink(spec)
            }
            fn inspect_video(&self, p: &Path) -> Result<VideoInspection, ScreenError> {
                self.0.inspect_video(p)
            }
            fn process_cpu_time(&self) -> Option<Duration> {
                None
            }
            fn logical_cores(&self) -> u32 {
                1
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 1000;
        let files = Files::default();
        let video = NoWarmFile(
            FakeVideo { files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) },
            std::cell::Cell::new(0),
        );
        let media = ScreenMedia::new(Box::new(good_screen()), Box::new(video), 1, 30, 8000);
        let mut input = ScriptedInput::new(vec![(2000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(out.ok, "the real recording went ahead");
        assert_eq!(files.0.borrow().len(), 1);
    }

    // ---- watching how regularly the program looks ----

    #[test]
    fn steady_looks_are_counted_and_long_waits_are_remembered_with_what_filled_them() {
        let base = Instant::now();
        let at = |ms: u64| base + Duration::from_millis(ms);
        let mut t = LookTracker::default();
        t.looked_at(at(0), 0);
        t.looked_at(at(20), 20);
        t.looked_at(at(60), 60); // 40 ms: late, but not a stall
        t.last_picture = Duration::from_millis(3);
        t.last_sound = Duration::from_millis(1);
        t.saving_since = Duration::from_millis(640);
        t.looked_at(at(710), 710); // a wait of 650 ms
        t.looked_at(at(730), 730);
        let s = t.stats();
        assert_eq!(s.looks, 4);
        assert_eq!((s.gaps_over_50_ms, s.gaps_over_100_ms, s.longest_gap_ms), (1, 1, 650));
        assert!((s.average_gap_ms - 182.5).abs() < 0.1, "{}", s.average_gap_ms);
        assert_eq!(
            s.slow_moments,
            vec![SlowMoment { at_ms: 710, gap_ms: 650, picture_ms: 3, sound_ms: 1, sound_video_ms: 0, saving_ms: 640 }]
        );
    }

    #[test]
    fn the_time_spent_paused_is_not_counted_as_a_stall() {
        let base = Instant::now();
        let at = |ms: u64| base + Duration::from_millis(ms);
        let mut t = LookTracker::default();
        t.looked_at(at(0), 0);
        t.looked_at(at(20), 20);
        t.forget(); // paused
        t.looked_at(at(20_000), 20); // resumed twenty seconds later
        t.looked_at(at(20_020), 40);
        let s = t.stats();
        assert_eq!((s.looks, s.gaps_over_100_ms, s.longest_gap_ms), (2, 0, 20));
    }

    #[test]
    fn only_the_worst_few_long_waits_are_kept_longest_first() {
        let base = Instant::now();
        let mut t = LookTracker::default();
        let mut now = 0;
        t.looked_at(base, 0);
        for gap in [200u64, 900, 300, 150, 700, 400, 250, 120] {
            now += gap;
            t.looked_at(base + Duration::from_millis(now), now);
        }
        let s = t.stats();
        assert_eq!(s.gaps_over_100_ms, 8);
        let kept: Vec<u64> = s.slow_moments.iter().map(|m| m.gap_ms).collect();
        assert_eq!(kept, vec![900, 700, 400, 300, 250, 200]);
    }

    #[test]
    fn the_report_of_a_take_says_how_regularly_it_looked_and_ignores_the_countdown_drops() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 1000;
        let (media, _files) = factory(good_screen());
        let report = media.report_handle();
        let mut input = ScriptedInput::new(vec![(4000, Input::Command(Command::Stop))]);
        run_session(&cfg, &media, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        let r = report.borrow().clone().unwrap();
        assert!(r.looks.looks > 100, "the program looked at the screen over and over: {}", r.looks.looks);
        assert_eq!(r.pictures_replaced, 0, "nothing was thrown away while recording");
    }
}
