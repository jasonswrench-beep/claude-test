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

use crate::video::Pacer;
use crate::{Frame, FrameSource, Platform, SinkSpec, VideoPlatform, VideoSink};
use openstage_capture::{MediaFactory, SessionConfig, SessionError, TakeMedia};
use openstage_capture_project::MediaFile;
use std::path::Path;
use std::time::Duration;

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
}

impl ScreenMedia {
    pub fn new(
        screen: Box<dyn Platform>,
        video: Box<dyn VideoPlatform>,
        display: u32,
        fps: u32,
        bitrate_kbps: u32,
    ) -> Self {
        Self { screen, video, display, fps: fps.max(1), bitrate_kbps }
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
        vec![MediaFile { file: SCREEN_FILE.into(), kind: "screen".into(), container: "fragmented-mp4".into() }]
    }

    fn declared_flush_ms(&self, _cfg: &SessionConfig) -> u64 {
        DECLARED_LOSS_MS
    }

    fn begin(&self, take_dir: &Path, _cfg: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
        self.video.start_media().map_err(|e| media_error(format!("Windows' video tools would not start: {e}")))?;

        let displays = self.screen.list_displays().map_err(media_error)?;
        let display = displays.iter().find(|d| d.number == self.display).ok_or_else(|| {
            media_error(format!("There is no Display {}. Choose a number from 1 to {}.", self.display, displays.len()))
        })?;
        let mut source = self
            .screen
            .start_display_capture(display, false)
            .map_err(|e| media_error(format!("Could not start capturing {}: {e}", display.describe())))?;
        let first = source.next_frame(Duration::from_secs(3)).ok_or_else(|| {
            media_error(
                "No picture arrived from the screen within 3 seconds. Windows only sends a picture when something changes: try again while moving a window.",
            )
        })?;
        if first.width % 2 != 0 || first.height % 2 != 0 {
            return Err(media_error(format!(
                "The display is {} by {} pixels. H.264 video needs an even width and height, and trimming is not built yet.",
                first.width, first.height
            )));
        }

        let spec = SinkSpec {
            path: take_dir.join(SCREEN_FILE),
            width: first.width,
            height: first.height,
            fps: self.fps,
            bitrate_kbps: self.bitrate_kbps,
            fragmented: true,
        };
        let sink =
            self.video.create_sink(&spec).map_err(|e| media_error(format!("Could not start the video file: {e}")))?;
        Ok(Box::new(ScreenTake {
            source: Some(source),
            sink: Some(sink),
            pacer: Pacer::new(self.fps),
            latest: first,
            written: 0,
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
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SessionError> {
        // Windows' writer closes a fragment about once a second by itself; there is nothing to push.
        Ok(())
    }

    fn levels(&self, _media_ms: u64) -> Vec<(&'static str, f32)> {
        // No sound is recorded yet.
        Vec::new()
    }

    fn finish(mut self: Box<Self>, _media_ms: u64) -> Result<u32, SessionError> {
        // Stop taking pictures first, so nothing arrives while the file is being completed.
        self.source = None;
        if self.written == 0 {
            // A recording stopped at once still gets one picture, so the file is a valid video.
            self.write_beat(0)?;
        }
        let sink = self.sink.take().expect("the video file is open until it is finished");
        sink.finish().map_err(|e| media_error(format!("The video file could not be completed: {e}")))?;
        Ok(1)
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
    }

    #[derive(Default)]
    struct Files(Rc<RefCell<Vec<Rc<RefCell<File>>>>>);

    struct FakeSink(Rc<RefCell<File>>);
    impl VideoSink for FakeSink {
        fn write(&mut self, _: &Frame, pts: i64, dur: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().beats.push((pts, dur));
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
            let file =
                Rc::new(RefCell::new(File { fragmented: spec.fragmented, path: spec.path.clone(), ..File::default() }));
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
}
