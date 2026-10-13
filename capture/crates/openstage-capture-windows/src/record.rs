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
use crate::sound::{Sound, SoundPlan, SoundTake, MIXED_BITRATE_KBPS};
use crate::video::{AudioSpec, Pacer};
use crate::{Frame, FrameSource, Platform, SinkSpec, VideoPlatform, VideoSink};
use openstage_capture::{MediaFactory, SessionConfig, SessionError, TakeMedia};
use openstage_capture_project::MediaFile;
use std::path::Path;
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
    video: Box<dyn VideoPlatform>,
    display: u32,
    fps: u32,
    bitrate_kbps: u32,
    sound: Option<Sound>,
}

impl ScreenMedia {
    pub fn new(
        screen: Box<dyn Platform>,
        video: Box<dyn VideoPlatform>,
        display: u32,
        fps: u32,
        bitrate_kbps: u32,
    ) -> Self {
        Self { screen, video, display, fps: fps.max(1), bitrate_kbps, sound: None }
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
        let mut files =
            vec![MediaFile { file: SCREEN_FILE.into(), kind: "screen".into(), container: "fragmented-mp4".into() }];
        if let Some(sound) = &self.sound {
            files.extend(sound.plan().media_files());
        }
        files
    }

    fn declared_flush_ms(&self, _cfg: &SessionConfig) -> u64 {
        DECLARED_LOSS_MS
    }

    fn begin(&self, take_dir: &Path, _cfg: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
        let started = Instant::now();
        self.video.start_media().map_err(|e| media_error(format!("Windows' video tools would not start: {e}")))?;

        let tools_ready = started.elapsed();
        let displays = self.screen.list_displays().map_err(media_error)?;
        let display = displays.iter().find(|d| d.number == self.display).ok_or_else(|| {
            media_error(format!("There is no Display {}. Choose a number from 1 to {}.", self.display, displays.len()))
        })?;
        let mut source = self
            .screen
            .start_display_capture(display, false)
            .map_err(|e| media_error(format!("Could not start capturing {}: {e}", display.describe())))?;
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
            "start-up timing: video tools {} ms, screen capture started {} ms, first picture {} ms, video file ready {} ms (all counted from the start)",
            tools_ready.as_millis(),
            capture_started.as_millis(),
            first_picture.as_millis(),
            started.elapsed().as_millis()
        );
        Ok(Box::new(ScreenTake {
            source: Some(source),
            sink: Some(sink),
            pacer: Pacer::new(self.fps),
            latest: first,
            written: 0,
            sound,
        }))
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
}

impl ScreenTake {
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
        if let Some(source) = self.source.as_mut() {
            while let Some(newer) = source.next_frame(Duration::ZERO) {
                self.latest = newer;
            }
        }
        if let Some(beat) = self.pacer.due(Duration::from_millis(media_ms)) {
            self.write_beat(beat)?;
        }
        if let (Some(sound), Some(sink)) = (self.sound.as_mut(), self.sink.as_mut()) {
            sound.pump(media_ms, sink.as_mut())?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SessionError> {
        // Windows' writer closes a fragment about once a second by itself; the separate sound files are
        // pushed to the disk here, with their lengths recorded, so a crash costs about a second of them.
        match self.sound.as_mut() {
            Some(sound) => sound.sync(),
            None => Ok(()),
        }
    }

    fn levels(&self, _media_ms: u64) -> Vec<(&'static str, f32)> {
        self.sound.as_ref().map(|s| s.levels()).unwrap_or_default()
    }

    fn paused(&mut self) {
        // The microphone keeps running while recording time stands still; what it hears is not kept.
        if let Some(sound) = self.sound.as_mut() {
            sound.pause();
        }
    }

    fn resumed(&mut self) {
        if let Some(sound) = self.sound.as_mut() {
            sound.resume();
        }
    }

    fn finish(mut self: Box<Self>, media_ms: u64) -> Result<u32, SessionError> {
        // Stop taking pictures first, so nothing arrives while the file is being completed.
        self.source = None;
        if self.written == 0 {
            // A recording stopped at once still gets one picture, so the file is a valid video.
            self.write_beat(0)?;
        }
        let mut sink = self.sink.take().expect("the video file is open until it is finished");
        let mut files = 1;
        if let Some(sound) = self.sound.take() {
            files += sound.track_count();
            sound.finish(media_ms, sink.as_mut())?;
        }
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
            let picture = self.gives_picture.then(|| Frame {
                width: self.width,
                height: self.height,
                t_ms: 0,
                bgra: vec![9; self.width as usize * self.height as usize * 4],
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
    }

    #[derive(Default)]
    struct Files(Rc<RefCell<Vec<Rc<RefCell<File>>>>>);

    struct FakeSink(Rc<RefCell<File>>);
    impl VideoSink for FakeSink {
        fn write(&mut self, _: &Frame, pts: i64, dur: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().beats.push((pts, dur));
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
    }

    fn factory(screen: FakeScreen) -> (ScreenMedia, Files) {
        let files = Files::default();
        let video = FakeVideo { files: Rc::clone(&files.0) };
        (ScreenMedia::new(Box::new(screen), Box::new(video), 1, 30, 8000), files)
    }

    fn good_screen() -> FakeScreen {
        FakeScreen { width: 8, height: 4, gives_picture: true }
    }

    fn config(dir: &tempfile::TempDir) -> SessionConfig {
        let mut cfg = SessionConfig::new(dir.path().join("lesson.capture"));
        cfg.countdown_ms = 0;
        cfg
    }

    // ---- tests ----

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
        let video = FakeVideo { files: Rc::clone(&files.0) };
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
}
