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
use crate::report::{LookStats, PictureWork, RecordingReport, SlowMoment};
use crate::sound::{Sound, SoundPlan, SoundTake, MIXED_BITRATE_KBPS};
use crate::video::{AudioSpec, Pacer};
use crate::webcam::{CameraInfo, CameraMode, CameraPlatform, StartedCamera, WebcamTake, WEBCAM_FILE};
use crate::window::{fit_into, recording_size, FittedSource, WindowInfo};
use crate::{CaptureWork, Frame, FrameSource, Outline, Platform, SinkSpec, VideoPlatform, VideoSink};
use openstage_capture::{MediaFactory, SessionConfig, SessionError, TakeMedia};
use openstage_capture_project::{CameraInfo as CameraRecord, MediaFile, RegionInfo};
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
    /// Whether to show a thin frame around the region on the screen while it is recorded.
    outline: bool,
    /// Record just this window instead of a display, if set.
    window: Option<WindowInfo>,
    sound: Option<Sound>,
    /// Record this webcam too, into a file of its own, if set.
    camera: Option<(Box<dyn CameraPlatform>, CameraInfo)>,
    /// The clock the webcam's picture times are measured on (Windows' own, except in tests), and how long to wait
    /// at a pause or the end for the pictures taken in the last moments to arrive.
    camera_clock: Rc<dyn Fn() -> i64>,
    camera_settle: Duration,
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
            outline: false,
            window: None,
            sound: None,
            camera: None,
            camera_clock: Rc::new(crate::now_100ns),
            camera_settle: Duration::from_millis(400),
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

    /// Record just this window instead of a display. The video has the size the window has when recording
    /// begins (made even); if the window is resized, its picture is made smaller to fit. If the window is
    /// closed, the recording stops at once and keeps everything recorded until then.
    pub fn with_window(mut self, window: WindowInfo) -> Self {
        self.window = Some(window);
        self
    }

    /// Show a thin frame around the region on the screen from the start of the countdown until the take
    /// ends, so the author can see what is being kept. It is left out of the video. Does nothing without
    /// a region.
    pub fn with_outline(mut self, on: bool) -> Self {
        self.outline = on;
        self
    }

    /// Record sound too: the microphone and/or the computer's own sound, as already chosen in `plan`.
    /// An empty plan records no sound.
    pub fn with_sound(mut self, platform: Box<dyn AudioPlatform>, plan: SoundPlan) -> Self {
        self.sound = (!plan.is_empty()).then(|| Sound::new(platform, plan));
        self
    }

    /// Record this webcam too, into `webcam.mp4` beside the screen video, with each picture placed where it was
    /// taken so that it lines up with the voice. A camera that stops does not stop the recording.
    pub fn with_camera(mut self, platform: Box<dyn CameraPlatform>, camera: CameraInfo) -> Self {
        self.camera = Some((platform, camera));
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
        match &self.window {
            Some(window) => format!("window:{}", window.process),
            None => format!("screen:{}", self.display),
        }
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
            camera: None,
        }];
        if let Some(sound) = &self.sound {
            files.extend(sound.plan().media_files());
        }
        if let Some((_, camera)) = &self.camera {
            files.push(MediaFile {
                file: WEBCAM_FILE.into(),
                kind: "webcam".into(),
                container: "fragmented-mp4".into(),
                mix: None,
                region: None,
                camera: Some(CameraRecord { name: camera.name.clone() }),
            });
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
        let Prepared { source, first, listening, outline, camera } = prepared;
        let dropped_at_start = source.dropped();
        let received_at_start = source.received();
        let capture_work_at_start = source.work();

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
        let webcam = match (camera, &self.camera) {
            (Some(ready), Some((_, info))) => Some(self.begin_webcam(take_dir, ready, &info.name)?),
            _ => None,
        };
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
            _outline: outline,
            source: Some(source),
            sink: Some(sink),
            pacer: Pacer::new(self.fps),
            latest: first,
            written: 0,
            sound,
            webcam,
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
            capture_work_at_start,
            thread_cpu_start: self.video.thread_cpu_time(),
            capture_work: CaptureWork::default(),
            pictures_received: 0,
            pause_mark: None,
            dropped_in_pauses: 0,
            received_in_pauses: 0,
            most_waiting: 0,
            looks: LookTracker::default(),
        }))
    }
}

/// What starting to capture gives: the pictures, the frame shown round a region (if any), how long starting
/// took, and the first picture.
struct Started {
    source: Box<dyn FrameSource>,
    outline: Option<Outline>,
    capture_started: Duration,
    first: Frame,
}

/// Everything that can be started before the take's folder is known.
struct Prepared {
    source: Box<dyn FrameSource>,
    first: Frame,
    listening: Option<crate::sound::OpenSound>,
    /// The frame shown around the region, if any; it goes when this is dropped.
    outline: Option<Outline>,
    /// The webcam, started and with its first picture, if one is to be recorded.
    camera: Option<ReadyCamera>,
}

/// A webcam that has been started and has sent its first picture.
struct ReadyCamera {
    source: Box<dyn FrameSource>,
    mode: CameraMode,
    first: Frame,
}

impl ScreenMedia {
    /// Start Windows' video tools, the screen capture, and the sound. Waits for the first picture.
    fn prepare(&self) -> Result<Prepared, SessionError> {
        let started = Instant::now();
        self.video.start_media().map_err(|e| media_error(format!("Windows' video tools would not start: {e}")))?;
        let tools_ready = started.elapsed();
        let Started { source, outline, capture_started, first } = match &self.window {
            Some(window) => self.prepare_window(window, started)?,
            None => self.prepare_display(started)?,
        };
        let first_picture = started.elapsed();
        if first.width % 2 != 0 || first.height % 2 != 0 {
            return Err(media_error(format!(
                "The display is {} by {} pixels. H.264 video needs an even width and height, and trimming is not built yet.",
                first.width, first.height
            )));
        }
        // The webcam takes the longest to wake up, so it is started early. Like the sound, it is started before
        // any video file exists, so a camera that cannot be opened leaves nothing behind.
        let camera = match &self.camera {
            Some((platform, info)) => Some(self.prepare_camera(platform.as_ref(), info)?),
            None => None,
        };
        let camera_ready = started.elapsed();
        let listening = match &self.sound {
            Some(sound) => Some(sound.open()?),
            None => None,
        };
        eprintln!(
            "preparing: video tools {} ms, screen capture started {} ms, first picture {} ms, {}sound listening {} ms (counted from the start of preparing)",
            tools_ready.as_millis(),
            capture_started.as_millis(),
            first_picture.as_millis(),
            if camera.is_some() { format!("webcam's first picture {} ms, ", camera_ready.as_millis()) } else { String::new() },
            started.elapsed().as_millis()
        );
        Ok(Prepared { source, first, listening, outline, camera })
    }

    /// Start the webcam and wait for its first picture. Says in words what to do if it will not start.
    fn prepare_camera(&self, platform: &dyn CameraPlatform, info: &CameraInfo) -> Result<ReadyCamera, SessionError> {
        let StartedCamera { mut source, mode } = platform.start_camera(info).map_err(|e| {
            media_error(format!(
                "Could not start the webcam {}: {e} To record without the webcam, leave out --camera.",
                info.describe()
            ))
        })?;
        // Wait up to 5 seconds, in short steps so a camera that has already said it is gone is not waited for.
        let mut first = None;
        for _ in 0..50 {
            first = source.next_frame(Duration::from_millis(100));
            if first.is_some() || source.ended().is_some() {
                break;
            }
        }
        let first = first.ok_or_else(|| {
            let why = source.ended().map(|why| format!(" Windows said: {why}")).unwrap_or_default();
            media_error(format!(
                "No picture arrived from the webcam {} within 5 seconds.{why} Another program may be using it, or Windows may be blocking cameras (Settings, Privacy & security, Camera). To record without the webcam, leave out --camera.",
                info.describe()
            ))
        })?;
        if first.width % 2 != 0 || first.height % 2 != 0 {
            return Err(media_error(format!(
                "The webcam {} sends pictures {} by {} pixels. H.264 video needs an even width and height. Another camera mode is needed; tell me which camera this is.",
                info.describe(),
                first.width,
                first.height
            )));
        }
        Ok(ReadyCamera { source, mode, first })
    }

    /// Make the webcam's video file and the take that fills it.
    fn begin_webcam(&self, take_dir: &Path, ready: ReadyCamera, name: &str) -> Result<WebcamTake, SessionError> {
        let ReadyCamera { source, mode, first } = ready;
        // The file has the camera's own rate, between 10 and 30 a second.
        let fps = (mode.fps.round() as u32).clamp(10, 30);
        let spec = SinkSpec {
            path: take_dir.join(WEBCAM_FILE),
            width: first.width,
            height: first.height,
            fps,
            bitrate_kbps: webcam_bitrate_kbps(first.width, first.height),
            fragmented: true,
            audio: None,
        };
        let sink = self
            .video
            .create_sink(&spec)
            .map_err(|e| media_error(format!("Could not start the webcam video file: {e}")))?;
        // At a pause and at the end, wait a little for the pictures taken in the last moments to arrive (they are a
        // tenth of a second or more on their way), so the webcam video does not end on older pictures than the rest.
        Ok(WebcamTake::new(name.to_string(), source, sink, first, fps, Rc::clone(&self.camera_clock))
            .with_settle(self.camera_settle))
    }

    /// Start capturing the chosen display (or the part of it chosen as a region) and wait for its first picture.
    fn prepare_display(&self, started: Instant) -> Result<Started, SessionError> {
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
        let outline = match (self.outline, region) {
            (true, Some(r)) => self.screen.show_region_outline(display, &r),
            _ => None,
        };
        let first = source.next_frame(Duration::from_secs(3)).ok_or_else(|| {
            media_error(
                "No picture arrived from the screen within 3 seconds. Windows only sends a picture when something changes: try again while moving a window.",
            )
        })?;
        Ok(Started { source, outline, capture_started, first })
    }

    /// Start capturing the chosen window and wait for its first picture. The video gets that picture's size
    /// (made even); a window that is later resized is fitted into it.
    fn prepare_window(&self, window: &WindowInfo, started: Instant) -> Result<Started, SessionError> {
        let source = self
            .screen
            .start_window_capture(window, false)
            .map_err(|e| media_error(format!("Could not start capturing the window {}: {e}", window.describe())))?;
        let capture_started = started.elapsed();
        let mut source = source;
        let first = source.next_frame(Duration::from_secs(3)).ok_or_else(|| {
            media_error(format!(
                "No picture arrived from the window {} within 3 seconds. Windows only sends a picture when something in the window changes, and none while it is minimized: restore it and move something in it, then try again.",
                window.describe()
            ))
        })?;
        let (width, height) = recording_size(first.width, first.height).map_err(media_error)?;
        let first = if (first.width, first.height) == (width, height) {
            first
        } else {
            let fitted = fit_into(&first, width, height, Vec::new());
            source.recycle(first);
            fitted
        };
        Ok(Started {
            source: Box::new(FittedSource::new(source, width, height)),
            outline: None,
            capture_started,
            first,
        })
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
    /// The frame shown around the region; it goes when the take ends.
    _outline: Option<Outline>,
    source: Option<Box<dyn FrameSource>>,
    sink: Option<Box<dyn VideoSink>>,
    pacer: Pacer,
    /// The newest picture of the screen; repeated while the screen does not change.
    latest: Frame,
    written: u64,
    sound: Option<SoundTake>,
    /// The webcam, when one is being recorded.
    webcam: Option<WebcamTake>,
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
    /// What the source had done before recording began (the countdown reads pictures back too), and,
    /// once recording ends, what it did while recording.
    capture_work_at_start: CaptureWork,
    capture_work: CaptureWork,
    /// The recording thread's processor time when recording began.
    thread_cpu_start: Option<Duration>,
    pictures_received: u64,
    /// Pictures Windows threw away and delivered while recording was paused, when nobody was taking them: not counted.
    pause_mark: Option<(u64, u64)>,
    dropped_in_pauses: u64,
    received_in_pauses: u64,
    /// The most pictures taken at one look. The waiting line holds 8, but more can arrive while it is emptied.
    most_waiting: usize,
    looks: LookTracker,
}

/// The quality of the webcam video: 3 megabits a second at 1280 by 720, more or less in step with the number of
/// pixels, never below 1 or above 8. A face in front of a plain background needs far less than a screen of text.
fn webcam_bitrate_kbps(width: u32, height: u32) -> u32 {
    let pixels = width as u64 * height as u64;
    ((3000 * pixels / (1280 * 720)) as u32).clamp(1000, 8000)
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
    /// A pause has ended (or recording stopped while paused): what Windows delivered and threw away meanwhile
    /// is not part of the recording's figures.
    fn count_pause(&mut self) {
        if let (Some((dropped, received)), Some(source)) = (self.pause_mark.take(), self.source.as_ref()) {
            self.dropped_in_pauses += source.dropped().saturating_sub(dropped);
            self.received_in_pauses += source.received().saturating_sub(received);
        }
    }

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
                // The picture being replaced is used up: give its memory back for a later one.
                let older = std::mem::replace(&mut self.latest, newer);
                source.recycle(older);
                waiting += 1;
            }
            self.most_waiting = self.most_waiting.max(waiting);
            // A window that was closed will send no more pictures: stop, keeping everything recorded so far.
            if let Some(why) = source.ended() {
                return Err(media_error(why));
            }
        }
        if let Some(beat) = self.pacer.due(Duration::from_millis(media_ms)) {
            self.write_beat(beat)?;
        }
        if let Some(webcam) = self.webcam.as_mut() {
            webcam.pump(media_ms)?;
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
        self.pause_mark = self.source.as_ref().map(|s| (s.dropped(), s.received()));
        // The microphone keeps running while recording time stands still; what it hears is not kept.
        if let Some(sound) = self.sound.as_mut() {
            sound.pause();
        }
        if let Some(webcam) = self.webcam.as_mut() {
            webcam.paused();
        }
    }

    fn resumed(&mut self) {
        self.looks.forget();
        self.count_pause();
        if let Some(sound) = self.sound.as_mut() {
            sound.resume();
        }
        if let Some(webcam) = self.webcam.as_mut() {
            webcam.resumed();
        }
    }

    fn notices(&mut self) -> Vec<(String, String)> {
        self.webcam.as_mut().map(|w| w.notices()).unwrap_or_default()
    }

    fn finish(mut self: Box<Self>, media_ms: u64) -> Result<u32, SessionError> {
        // Stop taking pictures first, so nothing arrives while the file is being completed.
        self.count_pause();
        self.pictures_replaced = self
            .source
            .as_ref()
            .map_or(0, |s| s.dropped())
            .saturating_sub(self.dropped_at_start)
            .saturating_sub(self.dropped_in_pauses);
        self.pictures_received = self
            .source
            .as_ref()
            .map_or(0, |s| s.received())
            .saturating_sub(self.received_at_start)
            .saturating_sub(self.received_in_pauses);
        self.capture_work =
            self.source.as_ref().map(|s| s.work().since(&self.capture_work_at_start)).unwrap_or_default();
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
        // The webcam's last moments are written after the screen and the sound are done, so a camera that is slow to
        // send its last pictures cannot hold them up; an error is told after the screen video is complete.
        let webcam = self.webcam.take().map(|w| w.finish(media_ms));
        if webcam.is_some() {
            files += 1;
        }
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
            work: PictureWork {
                capture: self.capture_work,
                sink: sink.work(),
                loop_cpu: self.thread_cpu_start.zip(self.video.thread_cpu_time()).map(|(a, b)| b.saturating_sub(a)),
            },
            webcam: webcam.as_ref().and_then(|w| w.as_ref().ok().cloned()),
        });
        sink.finish().map_err(|e| media_error(format!("The video file could not be completed: {e}")))?;
        if let Some(Err(e)) = webcam {
            return Err(e);
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayInfo, EncoderInfo, ScreenError, VideoInspection, WindowsVersion};
    use openstage_capture::{run_session, EventSink, Input, ManualClock, ScriptedInput, SessionConfig, VecSink};
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
        /// The regions a frame was shown around, and how many such frames are on the screen right now.
        outlines: Rc<RefCell<Vec<Region>>>,
        outlines_showing: Rc<std::cell::Cell<i32>>,
    }

    /// A frame on the fake screen; it counts itself off when it goes.
    struct FakeFrame(Rc<std::cell::Cell<i32>>);
    impl Drop for FakeFrame {
        fn drop(&mut self) {
            self.0.set(self.0.get() - 1);
        }
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
        fn show_region_outline(&self, _: &DisplayInfo, region: &Region) -> Option<crate::Outline> {
            self.outlines.borrow_mut().push(*region);
            self.outlines_showing.set(self.outlines_showing.get() + 1);
            Some(Box::new(FakeFrame(Rc::clone(&self.outlines_showing))))
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
        /// Make completing this file fail (for the webcam's file, in the test that checks the failure is told).
        fail_finish: bool,
    }

    #[derive(Default)]
    struct Files(Rc<RefCell<Vec<Rc<RefCell<File>>>>>);

    struct FakeSink(Rc<RefCell<File>>);
    impl VideoSink for FakeSink {
        fn work(&self) -> crate::SinkWork {
            crate::SinkWork { pictures: self.0.borrow().beats.len() as u64, ..Default::default() }
        }
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
            if self.0.borrow().fail_finish {
                return Err(ScreenError::Failed("the drive was removed".into()));
            }
            self.0.borrow_mut().finished = true;
            Ok(())
        }
    }

    struct FakeVideo {
        /// Completing a file whose name starts with `webcam` fails.
        webcam_finish_fails: bool,
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
                fail_finish: self.webcam_finish_fails
                    && spec.path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("webcam")),
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
        factory_with(screen, false)
    }

    fn factory_with(screen: FakeScreen, webcam_finish_fails: bool) -> (ScreenMedia, Files) {
        let files = Files::default();
        let video = FakeVideo { webcam_finish_fails, files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) };
        (ScreenMedia::new(Box::new(screen), Box::new(video), 1, 30, 8000), files)
    }

    fn good_screen() -> FakeScreen {
        FakeScreen {
            width: 8,
            height: 4,
            gives_picture: true,
            positional: false,
            captures: Rc::new(std::cell::Cell::new(0)),
            outlines: Rc::new(RefCell::new(Vec::new())),
            outlines_showing: Rc::new(std::cell::Cell::new(0)),
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
        let video =
            FakeVideo { webcam_finish_fails: false, files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) };
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
            FakeVideo { webcam_finish_fails: false, files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) },
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

    #[test]
    fn pictures_that_are_used_up_are_given_back_for_reuse_and_the_picture_work_is_reported() {
        use std::cell::Cell;
        /// A screen that has one fresh picture for every other time it is asked, like Windows delivering
        /// pictures as the screen changes. It counts the pictures given out and the ones handed back.
        struct Stream {
            asks: u32,
            given: Rc<Cell<u32>>,
            recycled: Rc<Cell<u32>>,
        }
        impl FrameSource for Stream {
            fn next_frame(&mut self, _: Duration) -> Option<Frame> {
                self.asks += 1;
                (self.asks % 2 == 1).then(|| {
                    self.given.set(self.given.get() + 1);
                    Frame {
                        width: 8,
                        height: 4,
                        t_ms: 0,
                        captured_100ns: None,
                        arrived_100ns: None,
                        bgra: vec![9; 128],
                    }
                })
            }
            fn dropped(&self) -> u64 {
                0
            }
            fn recycle(&mut self, _: Frame) {
                self.recycled.set(self.recycled.get() + 1);
            }
            fn work(&self) -> CaptureWork {
                let n = self.given.get() as u64;
                CaptureWork { pictures: n, readback: Duration::from_millis(2 * n), ..CaptureWork::default() }
            }
        }
        struct StreamScreen(Rc<Cell<u32>>, Rc<Cell<u32>>);
        impl Platform for StreamScreen {
            fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
                Ok(WindowsVersion { major: 10, minor: 0, build: 26200 })
            }
            fn capture_supported(&self) -> bool {
                true
            }
            fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
                good_screen().list_displays()
            }
            fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
                Ok(Box::new(Stream { asks: 0, given: Rc::clone(&self.0), recycled: Rc::clone(&self.1) }))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(2000);
        let (given, recycled) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let files = Files::default();
        let video = FakeVideo { webcam_finish_fails: false, files: Rc::clone(&files.0), memory: Cell::new(100.0) };
        let media = ScreenMedia::new(
            Box::new(StreamScreen(Rc::clone(&given), Rc::clone(&recycled))),
            Box::new(video),
            1,
            30,
            8000,
        );
        let report = media.report_handle();
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new()).unwrap();

        // The first picture came before recording began and was replaced by the second, and so on: every
        // picture but the last one given out went back.
        assert!(given.get() > 10, "pictures kept arriving: {}", given.get());
        assert_eq!(recycled.get(), given.get() - 1, "each picture that was replaced was handed back");

        let r = report.borrow().clone().unwrap();
        assert_eq!(
            r.work.capture.pictures,
            u64::from(given.get() - 1),
            "only the pictures taken while recording count, not the one the countdown read"
        );
        assert_eq!(r.work.capture.readback, Duration::from_millis(2 * (u64::from(given.get()) - 1)));
        assert_eq!(r.work.sink.pictures, r.pictures_written, "every picture written to the video is counted");
        assert_eq!(r.work.loop_cpu, None, "the fake computer cannot say, so none is claimed");
    }

    #[test]
    fn a_frame_is_shown_only_when_asked_for_and_only_with_a_region_and_is_gone_when_the_take_ends() {
        for (frame_on, region, shown) in [(true, true, 1usize), (false, true, 0), (true, false, 0)] {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = config(&dir);
            cfg.max_duration_ms = Some(1000);
            let screen = wide_screen();
            let (outlines, showing) = (Rc::clone(&screen.outlines), Rc::clone(&screen.outlines_showing));
            let (media, _files) = factory(screen);
            let mut media = media.with_outline(frame_on);
            if region {
                media = media.with_region(Region { x: 30, y: 20, width: 64, height: 64 });
            }
            run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new())
                .unwrap();
            assert_eq!(outlines.borrow().len(), shown, "frame {frame_on}, region {region}");
            assert_eq!(showing.get(), 0, "no frame is left on the screen once the take has ended");
            if shown == 1 {
                assert_eq!(outlines.borrow()[0], Region { x: 30, y: 20, width: 64, height: 64 }, "round the region");
            }
        }
    }

    #[test]
    fn the_frame_is_on_the_screen_during_the_countdown_and_all_through_the_recording() {
        use openstage_capture_protocol::State;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 3000;
        cfg.max_duration_ms = Some(1000);
        let screen = wide_screen();
        let showing = Rc::clone(&screen.outlines_showing);
        let (media, _files) = factory(screen);
        let media = media.with_region(Region { x: 30, y: 20, width: 64, height: 64 }).with_outline(true);
        // How many frames were on the screen each time the session reported its state.
        struct Watch<'a> {
            inner: &'a mut VecSink,
            showing: Rc<std::cell::Cell<i32>>,
            during: Vec<(State, i32)>,
        }
        impl EventSink for Watch<'_> {
            fn emit(&mut self, event: Event) -> bool {
                if let Event::State { state, .. } = &event {
                    self.during.push((*state, self.showing.get()));
                }
                self.inner.emit(event)
            }
        }
        let mut sink = VecSink::default();
        let mut watch = Watch { inner: &mut sink, showing: Rc::clone(&showing), during: Vec::new() };
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut watch, &ManualClock::new()).unwrap();
        let of = |wanted: State| -> Vec<i32> {
            watch.during.iter().filter(|(s, _)| *s == wanted).map(|(_, n)| *n).collect()
        };
        assert!(of(State::Countdown).contains(&1), "seen during the countdown: {:?}", watch.during);
        assert!(!of(State::Recording).is_empty() && of(State::Recording).iter().all(|n| *n == 1), "{:?}", watch.during);
        assert!(of(State::Stopped).iter().all(|n| *n == 0), "gone once the take has ended: {:?}", watch.during);
        assert_eq!(showing.get(), 0);
    }

    // ---- one window ----

    /// A window that delivers a picture every other time it is asked, at sizes taken in turn from `sizes`
    /// (the last one for ever), and is closed after `close_after` pictures if that is set.
    struct WindowFrames {
        sizes: Vec<(u32, u32)>,
        asked: usize,
        delivered: usize,
        close_after: Option<usize>,
    }
    impl FrameSource for WindowFrames {
        fn next_frame(&mut self, _: Duration) -> Option<Frame> {
            self.asked += 1;
            if self.asked & 1 == 0 {
                return None;
            }
            let (width, height) = self.sizes[self.delivered.min(self.sizes.len() - 1)];
            self.delivered += 1;
            Some(Frame {
                width,
                height,
                t_ms: 0,
                captured_100ns: None,
                arrived_100ns: None,
                bgra: vec![9; (width * height * 4) as usize],
            })
        }
        fn dropped(&self) -> u64 {
            0
        }
        fn ended(&self) -> Option<String> {
            self.close_after.filter(|n| self.delivered >= *n).map(|_| {
                "The window Untitled - Notepad was closed. Everything recorded until then has been kept.".into()
            })
        }
    }

    struct WindowScreen {
        sizes: Vec<(u32, u32)>,
        close_after: Option<usize>,
    }
    impl Platform for WindowScreen {
        fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
            Ok(WindowsVersion { major: 10, minor: 0, build: 26200 })
        }
        fn capture_supported(&self) -> bool {
            true
        }
        fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
            Ok(vec![])
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Err(ScreenError::Failed("a window is recorded here, not a display".into()))
        }
        fn start_window_capture(&self, _: &WindowInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Ok(Box::new(WindowFrames {
                sizes: self.sizes.clone(),
                asked: 0,
                delivered: 0,
                close_after: self.close_after,
            }))
        }
    }

    fn notepad() -> WindowInfo {
        WindowInfo {
            number: 1,
            handle: 77,
            title: "Untitled - Notepad".into(),
            process: "notepad.exe".into(),
            width: 201,
            height: 121,
        }
    }

    fn window_media(screen: WindowScreen) -> (ScreenMedia, Files) {
        let files = Files::default();
        let video =
            FakeVideo { webcam_finish_fails: false, files: Rc::clone(&files.0), memory: std::cell::Cell::new(100.0) };
        (ScreenMedia::new(Box::new(screen), Box::new(video), 1, 30, 8000).with_window(notepad()), files)
    }

    #[test]
    fn a_window_is_recorded_at_its_own_even_size_and_named_in_take_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(1000);
        let (media, files) = window_media(WindowScreen { sizes: vec![(201, 121)], close_after: None });
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new())
            .unwrap();
        assert!(out.ok);
        let files = files.0.borrow();
        let f = files[0].borrow();
        assert_eq!(f.size, (200, 120), "the odd size is made even");
        assert!(!f.pictures.is_empty() && f.pictures.iter().all(|p| (p.0, p.1) == (200, 120)), "{:?}", f.pictures);
        assert_eq!(read_take(&cfg.project, 1).unwrap().sources, vec!["window:notepad.exe".to_string()]);
    }

    #[test]
    fn a_window_that_is_resized_while_recording_keeps_the_size_of_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(1500);
        // Its own size for a while, then much bigger, then smaller than it began.
        let sizes: Vec<(u32, u32)> =
            std::iter::repeat_n((200, 120), 5).chain(std::iter::repeat_n((600, 360), 5)).chain([(64, 64)]).collect();
        let (media, files) = window_media(WindowScreen { sizes, close_after: None });
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new()).unwrap();
        let files = files.0.borrow();
        let f = files[0].borrow();
        assert_eq!(f.size, (200, 120));
        assert!(f.pictures.len() > 30);
        assert!(
            f.pictures.iter().all(|p| (p.0, p.1) == (200, 120)),
            "every picture written is the size of the video, whatever the window did"
        );
    }

    #[test]
    fn a_window_that_is_closed_ends_the_recording_with_its_plain_message_and_keeps_what_was_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        // A time limit, so that if the closed window were not noticed the test would fail, not run for ever.
        cfg.max_duration_ms = Some(5000);
        let (media, files) = window_media(WindowScreen { sizes: vec![(200, 120)], close_after: Some(12) });
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        let (code, message) = sink
            .events
            .iter()
            .find_map(|e| match e {
                Event::Error { code, message, fatal: true } => Some((code.clone(), message.clone())),
                _ => None,
            })
            .expect("a fatal error event");
        assert_eq!(code, "media-failed");
        assert!(message.contains("was closed") && message.contains("has been kept"), "{message}");
        let files = files.0.borrow();
        let f = files[0].borrow();
        assert!(f.finished, "the video was completed");
        assert!(!f.pictures.is_empty(), "what was recorded before the window closed is in it");
    }

    #[test]
    fn a_window_that_is_too_small_is_refused_in_words_and_makes_no_video() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let (media, files) = window_media(WindowScreen { sizes: vec![(40, 700)], close_after: None });
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
        assert!(message.contains("at least 64 by 64") && message.contains("40 by 700"), "{message}");
        assert!(files.0.borrow().is_empty());
    }

    // ---- the webcam ----

    use crate::report::summarize;
    use crate::webcam::{CameraInfo, CameraMode, CameraPlatform, StartedCamera};
    use openstage_capture::Clock;

    /// The webcam's clock reads the session clock plus this, as a real camera's clock differs from the recorder's.
    const CAMERA_OFFSET: i64 = 9_000_000_000;

    /// A webcam taking pictures by the session's clock. Picture n is taken at n / fps seconds and reaches the program
    /// `delay_ms` later, except picture 0, which is ready as soon as the camera starts. Its pixels are n.
    struct FakeCamera {
        clock: Rc<ManualClock>,
        fps: u64,
        delay_ms: u64,
        next: u64,
        /// The camera is unplugged at this time: no picture is taken after it.
        stops_at_ms: Option<u64>,
    }
    impl FrameSource for FakeCamera {
        fn next_frame(&mut self, _: Duration) -> Option<Frame> {
            let now = self.clock.now_ms();
            let n = self.next;
            let taken = n * 1000 / self.fps;
            let arrives = if n == 0 { 0 } else { taken + self.delay_ms };
            if arrives > self.stops_at_ms.map_or(now, |s| now.min(s)) {
                return None;
            }
            self.next += 1;
            Some(Frame {
                width: 4,
                height: 2,
                t_ms: 0,
                captured_100ns: Some(CAMERA_OFFSET + taken as i64 * 10_000),
                arrived_100ns: Some(CAMERA_OFFSET + arrives as i64 * 10_000),
                bgra: vec![n as u8; 4 * 2 * 4],
            })
        }
        fn dropped(&self) -> u64 {
            0
        }
        fn ended(&self) -> Option<String> {
            self.stops_at_ms.filter(|s| self.clock.now_ms() >= *s).map(|_| "The camera was unplugged.".to_string())
        }
    }

    struct FakeCameras {
        clock: Rc<ManualClock>,
        delay_ms: u64,
        stops_at_ms: Option<u64>,
        refuses: Option<String>,
        /// Starts fine but never sends a picture.
        silent: bool,
    }
    impl CameraPlatform for FakeCameras {
        fn list_cameras(&self) -> Result<Vec<CameraInfo>, ScreenError> {
            Ok(vec![CameraInfo { number: 1, name: "Test Camera".into(), id: "x".into() }])
        }
        fn start_camera(&self, _: &CameraInfo) -> Result<StartedCamera, ScreenError> {
            if let Some(why) = &self.refuses {
                return Err(ScreenError::Failed(why.clone()));
            }
            let camera = FakeCamera {
                clock: Rc::clone(&self.clock),
                fps: 30,
                delay_ms: self.delay_ms,
                next: if self.silent { 1 } else { 0 },
                stops_at_ms: if self.silent { Some(0) } else { self.stops_at_ms },
            };
            Ok(StartedCamera {
                source: Box::new(camera),
                mode: CameraMode { width: 4, height: 2, fps: 30.0, format: "MJPG".into() },
            })
        }
    }

    /// A screen with a webcam, both following the same fake clock.
    fn with_webcam(clock: &Rc<ManualClock>, tune: impl FnOnce(&mut FakeCameras)) -> (ScreenMedia, Files) {
        with_webcam_files(clock, tune, false)
    }

    fn with_webcam_files(
        clock: &Rc<ManualClock>,
        tune: impl FnOnce(&mut FakeCameras),
        webcam_finish_fails: bool,
    ) -> (ScreenMedia, Files) {
        let mut cameras =
            FakeCameras { clock: Rc::clone(clock), delay_ms: 100, stops_at_ms: None, refuses: None, silent: false };
        tune(&mut cameras);
        let info = cameras.list_cameras().unwrap().remove(0);
        let (mut media, files) = factory_with(good_screen(), webcam_finish_fails);
        let camera_clock = Rc::clone(clock);
        media.camera_clock = Rc::new(move || CAMERA_OFFSET + camera_clock.now_ms() as i64 * 10_000);
        media.camera_settle = Duration::ZERO;
        (media.with_camera(Box::new(cameras), info), files)
    }

    fn errors_of(sink: &VecSink) -> Vec<String> {
        sink.events
            .iter()
            .filter_map(|e| match e {
                Event::Error { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_webcam_is_recorded_into_a_file_of_its_own_with_each_picture_where_it_was_taken() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam(&clock, |c| c.delay_ms = 120);
        let handle = media.report_handle();
        let mut input = ScriptedInput::new(vec![(3000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!(out.ok);
        assert_eq!(out.segments, 2, "the screen video and the webcam video");

        let take = read_take(&cfg.project, 1).unwrap();
        let listed: Vec<(&str, &str, &str)> =
            take.media.iter().map(|m| (m.file.as_str(), m.kind.as_str(), m.container.as_str())).collect();
        assert_eq!(
            listed,
            vec![("screen.mp4", "screen", "fragmented-mp4"), ("webcam.mp4", "webcam", "fragmented-mp4")]
        );
        assert_eq!(take.media[1].camera.as_ref().map(|c| c.name.as_str()), Some("Test Camera"));
        assert!(take.media[0].camera.is_none(), "only the webcam's entry names a camera");

        let files = files.0.borrow();
        assert_eq!(files.len(), 2);
        let screen = files[0].borrow();
        let webcam = files[1].borrow();
        assert!(screen.path.ends_with("screen.mp4") && webcam.path.ends_with("webcam.mp4"));
        assert!(webcam.fragmented && webcam.finished && !webcam.sound_track, "a crash-safe video of its own, no sound");
        assert_eq!(webcam.size, (4, 2), "the webcam video has the camera's own size, not the screen's");
        assert_eq!(screen.size, (8, 4));
        // 120 ms late pictures: moment k shows the picture taken at moment k, for every moment that could be known.
        for (k, picture) in webcam.pictures.iter().enumerate().take(((3000 - 120) * 30 / 1000) as usize) {
            assert_eq!(picture.2, [k as u8; 4], "moment {k}");
        }
        assert!(webcam.beats.len() >= 89, "{} moments", webcam.beats.len());

        let report = handle.borrow().clone().unwrap();
        let w = report.webcam.expect("the webcam is in the report");
        assert_eq!((w.camera.as_str(), w.width, w.height, w.fps), ("Test Camera", 4, 2, 30));
        assert_eq!((w.delay_average_ms, w.stopped), (120, None));
    }

    #[test]
    fn a_take_without_a_webcam_makes_no_webcam_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(500);
        let (media, files) = factory(good_screen());
        run_session(&cfg, &media, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(files.0.borrow().len(), 1);
        assert!(read_take(&cfg.project, 1).unwrap().media.iter().all(|m| m.kind != "webcam"));
    }

    #[test]
    fn a_webcam_that_will_not_start_ends_the_session_in_words_before_anything_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam(&clock, |c| c.refuses = Some("The camera is in use.".into()));
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, clock.as_ref()).unwrap();
        assert!(!out.ok);
        let message = errors_of(&sink).remove(0);
        assert!(
            message.contains("Could not start the webcam 1. Test Camera: The camera is in use.")
                && message.contains("leave out --camera"),
            "{message}"
        );
        assert!(files.0.borrow().is_empty(), "no video file was made, so nothing is left behind");
    }

    #[test]
    fn a_webcam_that_sends_no_picture_is_explained_with_the_likely_causes() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam(&clock, |c| c.silent = true);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut ScriptedInput::none(), &mut sink, clock.as_ref()).unwrap();
        assert!(!out.ok);
        let message = errors_of(&sink).remove(0);
        assert!(
            message.contains("No picture arrived from the webcam 1. Test Camera within 5 seconds")
                && message.contains("Another program may be using it")
                && message.contains("Privacy & security, Camera"),
            "{message}"
        );
        assert!(files.0.borrow().is_empty());
    }

    #[test]
    fn a_webcam_that_stops_does_not_stop_the_screen_and_the_author_is_told_at_once_and_again_in_the_summary() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam(&clock, |c| c.stops_at_ms = Some(1500));
        let handle = media.report_handle();
        let mut input = ScriptedInput::new(vec![(4000, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &media, &mut input, &mut sink, clock.as_ref()).unwrap();
        assert!(out.ok, "the recording carried on");
        let said: Vec<String> = sink
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Warning { code, message } if code == "webcam-stopped" => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(said.len(), 1, "told once: {said:?}");
        assert!(
            said[0].contains("The camera was unplugged.") && said[0].contains("still being recorded"),
            "{}",
            said[0]
        );

        let files = files.0.borrow();
        let screen = files[0].borrow();
        let webcam = files[1].borrow();
        assert!(screen.beats.len() >= 118, "the screen went on to the end: {}", screen.beats.len());
        assert!(webcam.finished && (40..=50).contains(&webcam.beats.len()), "{} webcam moments", webcam.beats.len());
        let w = handle.borrow().clone().unwrap().webcam.unwrap();
        assert!(w.stopped.is_some());
        let finding =
            summarize(&handle.borrow().clone().unwrap()).into_iter().find(|c| c.check == "run-webcam").unwrap();
        assert!(!finding.ok && finding.message.contains("The camera stopped at"), "{}", finding.message);
    }

    #[test]
    fn pausing_pauses_the_webcam_too_and_leaves_no_gap_in_it() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam(&clock, |_| {});
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (3000, Input::Command(Command::Resume)),
            (4000, Input::Command(Command::Stop)),
        ]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!(out.ok);
        let files = files.0.borrow();
        let screen = files[0].borrow();
        let webcam = files[1].borrow();
        // About 2 seconds of recording at 30 a second: one moment each, with no gap where the pause was.
        assert!((59..=63).contains(&webcam.beats.len()), "{} moments", webcam.beats.len());
        for (k, (pts, _)) in webcam.beats.iter().enumerate() {
            assert_eq!(*pts, k as i64 * 10_000_000 / 30, "moment {k}");
        }
        assert!((webcam.beats.len() as i64 - screen.beats.len() as i64).abs() <= 3, "the two videos are as long");
        // The last moments before the pause show pictures taken before it, not one from the end of the pause. (The
        // pause came at 0.98 seconds of recording, the last look before it, so moment 29 is the last before it.)
        for k in 0..=29usize {
            let shown = webcam.pictures[k].2[0] as usize;
            assert!(shown <= k && shown + 4 >= k, "moment {k} shows picture {shown}");
        }
        // The pictures after the resume are ones taken after it: picture numbers jump over the pause (2 s = 60 pictures).
        let before = webcam.pictures[25].2[0];
        let after = webcam.pictures[45].2[0];
        assert!(
            after as i32 - before as i32 >= 60,
            "pictures taken during the pause are not in the video: {before} then {after}"
        );
    }

    #[test]
    fn a_webcam_video_that_cannot_be_completed_is_told_but_only_after_the_screen_video_is_complete() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let (media, files) = with_webcam_files(&clock, |_| {}, true);
        let mut input = ScriptedInput::new(vec![(1500, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let _ = run_session(&cfg, &media, &mut input, &mut sink, clock.as_ref()).unwrap();
        let message = errors_of(&sink).remove(0);
        assert!(
            message.contains("webcam video file could not be completed") && message.contains("the drive was removed"),
            "{message}"
        );
        let files = files.0.borrow();
        assert!(files[0].borrow().finished, "the screen video was completed first");
        assert!(!files[1].borrow().finished);
    }

    #[test]
    fn pictures_windows_threw_away_during_a_pause_are_not_counted_as_lost_while_recording() {
        /// A screen whose counts grow with the clock, paused or not: one thrown away every 10 ms, one delivered every 5.
        struct Losing {
            clock: Rc<ManualClock>,
            first: Option<Frame>,
        }
        impl FrameSource for Losing {
            fn next_frame(&mut self, _: Duration) -> Option<Frame> {
                self.first.take()
            }
            fn dropped(&self) -> u64 {
                self.clock.now_ms() / 10
            }
            fn received(&self) -> u64 {
                self.clock.now_ms() / 5
            }
        }
        struct LosingScreen(Rc<ManualClock>);
        impl Platform for LosingScreen {
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
                let first = Frame {
                    width: 8,
                    height: 4,
                    t_ms: 0,
                    captured_100ns: None,
                    arrived_100ns: None,
                    bgra: vec![9; 8 * 4 * 4],
                };
                Ok(Box::new(Losing { clock: Rc::clone(&self.0), first: Some(first) }))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let clock = Rc::new(ManualClock::new());
        let files = Files::default();
        let video =
            FakeVideo { webcam_finish_fails: false, files: Rc::clone(&files.0), memory: std::cell::Cell::new(1.0) };
        let media = ScreenMedia::new(Box::new(LosingScreen(Rc::clone(&clock))), Box::new(video), 1, 30, 8000);
        let handle = media.report_handle();
        // One second recorded, 2 seconds paused, one second recorded.
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (3000, Input::Command(Command::Resume)),
            (4000, Input::Command(Command::Stop)),
        ]);
        let out = run_session(&cfg, &media, &mut input, &mut VecSink::default(), clock.as_ref()).unwrap();
        assert!(out.ok);
        let r = handle.borrow().clone().unwrap();
        // 2 seconds of recording: about 200 thrown away and 400 delivered. The pause's 200 and 400 are left out.
        assert!((180..=220).contains(&r.pictures_replaced), "thrown away: {}", r.pictures_replaced);
        assert!((360..=440).contains(&r.pictures_received), "delivered: {}", r.pictures_received);
    }
}
