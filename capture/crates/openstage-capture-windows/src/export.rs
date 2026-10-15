// SPDX-License-Identifier: Apache-2.0
//! Exporting an edit as a finished MP4 (story F1).
//!
//! [`export`] runs the whole job: it makes the plan from the edit, then for every frame of the finished video it
//! draws the frame (the render crate) and hands it to the video writer, and mixes the sound for the same stretch
//! of time and hands that over too. The pieces that need Windows (reading frames out of the recorded videos, drawing
//! text, writing the MP4) are passed in, so the flow, and every message a person will read, is tested here with fakes.
//!
//! * **Atomic.** The video is written to `name.part.mp4` and renamed to `name.mp4` only when it is whole. A
//!   cancelled or failed export deletes the `.part.mp4` file and leaves any earlier `name.mp4` exactly as it was. There
//!   is never a half-finished file under the real name.
//! * **Cancellable.** The caller is asked before every frame whether to stop.
//! * **The same every time.** Frames are drawn with arithmetic that is the same everywhere, and the sound is mixed
//!   with whole numbers, so the same edit and the same recordings hand the video writer exactly the same pictures
//!   and sound. (Whether the writer's own encoder then gives the same bytes is up to Windows' encoder.)
//! * **Frame by frame, sound by frame.** Each frame's sound is the stretch of time between it and the next frame,
//!   cut on whole sample boundaries, so no sound is skipped or repeated at a frame's edge.

use crate::video::{AudioSpec, SinkSpec, VideoPlatform};
use crate::Frame;
use openstage_capture_project::{Edit, EditSourceRef, WavFile};
use openstage_capture_render::{compose_frame, mix_block, plan, Canvas, PictureSource, RenderSettings, TextRenderer};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the sound of a source comes from.
pub trait SoundReader {
    /// Up to `frames` frames of the source's sound from frame `first_frame`: 48,000 a second, two channels
    /// interleaved. Fewer, or none, past the end of the sound (the rest is silence).
    fn read(&mut self, source: &EditSourceRef, first_frame: u64, frames: usize) -> Result<Vec<i16>, String>;
}

/// What to make.
#[derive(Debug, Clone)]
pub struct ExportJob {
    pub settings: RenderSettings,
    /// Make the video this size, instead of the size of the first picture on the screen track.
    pub size: Option<(u32, u32)>,
    /// Video quality in kilobits a second, or `None` for a quality that suits the size.
    pub bitrate_kbps: Option<u32>,
    /// Where the finished video goes.
    pub out: PathBuf,
}

/// Everything the job needs from outside.
pub struct ExportTools<'a> {
    pub video: &'a dyn VideoPlatform,
    pub pictures: &'a mut dyn PictureSource,
    pub text: &'a dyn TextRenderer,
    pub sound: &'a mut dyn SoundReader,
}

/// How far along the job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub frames_done: u64,
    pub frame_count: u64,
}

/// Why an export did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// The author asked to stop. Nothing was left behind.
    Cancelled,
    /// Something is wrong, in words.
    Failed(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Cancelled => write!(f, "The export was cancelled. No video file was made."),
            ExportError::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// What a finished export made.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportReport {
    pub out: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u64,
    pub seconds: f64,
    /// Whether the video has a sound track.
    pub sound: bool,
    /// Things the author should know about the result (text that did not fit), each once.
    pub notes: Vec<String>,
}

/// The quality that suits a size: 8 megabits a second at 1080p, in step with the number of pixels.
pub fn bitrate_for(width: u32, height: u32) -> u32 {
    ((8000u64 * width as u64 * height as u64) / (1920 * 1080)).clamp(1500, 16_000) as u32
}

/// Where the video is written until it is whole: `name.part.mp4` beside `name.mp4`.
///
/// The name must END in `.mp4`. Windows' video writer chooses the kind of file from the end of the name, and it does
/// not know `.part`: the first run on Windows, with `name.mp4.part`, failed at once with error 0xC00D36D5 ("The
/// specified object or value does not exist"). The `part` in the name still says it is not finished.
fn part_path(out: &Path) -> PathBuf {
    let name = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if name.to_ascii_lowercase().ends_with(".mp4") { &name[..name.len() - 4] } else { name.as_str() };
    out.with_file_name(format!("{stem}.part.mp4"))
}

fn failed(what: impl fmt::Display) -> ExportError {
    ExportError::Failed(what.to_string())
}

/// Make the video. `cancelled` is asked before every frame; `progress` is told after every frame.
pub fn export(
    edit: &Edit,
    job: &ExportJob,
    tools: ExportTools,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(Progress),
) -> Result<ExportReport, ExportError> {
    let ExportTools { video, pictures, text, sound } = tools;
    let mut settings = job.settings.clone();
    // The size: asked for, else the first picture on the screen track (made even), else the settings' own.
    let probe = plan(edit, &settings).map_err(failed)?;
    let (width, height) = match job.size {
        Some(size) => size,
        None => {
            let first = probe
                .video
                .iter()
                .find(|s| s.track_kind != openstage_capture_project::EditTrackKind::Webcam)
                .or_else(|| probe.video.first());
            match first {
                Some(span) => {
                    let layer = probe.layer_for(span);
                    let picture = pictures.picture(&layer).map_err(failed)?;
                    (picture.width & !1, picture.height & !1)
                }
                None => (settings.width, settings.height),
            }
        }
    };
    settings.width = width;
    settings.height = height;
    let plan = plan_with(edit, &settings)?;
    if plan.frame_count == 0 {
        return Err(failed("The edit is empty: there is nothing to put in a video yet."));
    }

    let has_sound = !plan.audio.is_empty();
    let spec = SinkSpec {
        path: part_path(&job.out),
        width,
        height,
        fps: settings.fps,
        bitrate_kbps: job.bitrate_kbps.unwrap_or_else(|| bitrate_for(width, height)),
        fragmented: false,
        audio: has_sound.then_some(AudioSpec { sample_rate: settings.sample_rate, channels: 2, bitrate_kbps: 128 }),
    };
    let _ = fs::remove_file(&spec.path);
    if let Some(folder) = job.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(folder)
            .map_err(|e| failed(format!("Could not make the folder {}: {e}", folder.display())))?;
    }
    video.start_media().map_err(|e| failed(format!("Windows' video tools would not start: {e}")))?;
    let mut sink = video.create_sink(&spec).map_err(|e| failed(format!("Could not start the video file: {e}")))?;

    let outcome = (|| -> Result<Vec<String>, ExportError> {
        let mut canvas = Canvas::new(width, height);
        let mut frame = Frame { width, height, t_ms: 0, captured_100ns: None, arrived_100ns: None, bgra: Vec::new() };
        let mut notes: Vec<String> = Vec::new();
        let rate = settings.sample_rate as u64;
        let fps = settings.fps as u64;
        for index in 0..plan.frame_count {
            if cancelled() {
                return Err(ExportError::Cancelled);
            }
            let recipe = plan.frame(index);
            let mut frame_notes = Vec::new();
            compose_frame(&recipe, pictures, text, &mut canvas, &mut frame_notes).map_err(failed)?;
            for note in frame_notes {
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
            frame.t_ms = recipe.time_us / 1000;
            frame.bgra.clear();
            frame.bgra.extend_from_slice(&canvas.bgra);
            // Exact to a tenth of a microsecond, and each frame starts where the one before ended.
            let start = (index * 10_000_000 / fps) as i64;
            let duration = ((index + 1) * 10_000_000 / fps) as i64 - start;
            sink.write(&frame, start, duration)
                .map_err(|e| failed(format!("Writing the video stopped with a problem: {e}")))?;

            if has_sound {
                let first = index * rate / fps;
                let end = (index + 1) * rate / fps;
                let mut read_problem: Option<String> = None;
                let mixed = mix_block(
                    &plan.audio,
                    first,
                    (end - first) as usize,
                    settings.sample_rate,
                    &mut |span, at, count| match sound.read(&span.source, at, count) {
                        Ok(frames) => frames,
                        Err(why) => {
                            read_problem.get_or_insert(why);
                            Vec::new()
                        }
                    },
                );
                if let Some(why) = read_problem {
                    return Err(failed(why));
                }
                let at = (first * 10_000_000 / rate) as i64;
                sink.write_audio(&mixed, at)
                    .map_err(|e| failed(format!("Writing the sound into the video stopped with a problem: {e}")))?;
            }
            progress(Progress { frames_done: index + 1, frame_count: plan.frame_count });
        }
        Ok(notes)
    })();

    match outcome {
        Err(e) => {
            // Nothing is left behind: the half-made file goes, and an earlier export is untouched.
            drop(sink);
            let _ = fs::remove_file(&spec.path);
            Err(e)
        }
        Ok(notes) => {
            if let Err(e) = sink.finish() {
                let _ = fs::remove_file(&spec.path);
                return Err(failed(format!("The video file could not be completed: {e}")));
            }
            if let Err(e) = fs::rename(&spec.path, &job.out) {
                let _ = fs::remove_file(&spec.path);
                return Err(failed(format!("The video was made but could not be put at {}: {e}", job.out.display())));
            }
            Ok(ExportReport {
                out: job.out.clone(),
                width,
                height,
                fps: settings.fps,
                frames: plan.frame_count,
                seconds: plan.frame_count as f64 / settings.fps as f64,
                sound: has_sound,
                notes,
            })
        }
    }
}

fn plan_with(edit: &Edit, settings: &RenderSettings) -> Result<openstage_capture_render::RenderPlan, ExportError> {
    plan(edit, settings).map_err(failed)
}

/// Where a source's file is: a recording's file under its take, or a course asset under the assets folder.
pub fn source_path(project: &Path, assets: Option<&Path>, source: &EditSourceRef) -> Result<PathBuf, String> {
    match source {
        EditSourceRef::Take { take, media } => Ok(openstage_capture_project::take_dir(project, *take).join(media)),
        EditSourceRef::Asset { asset } => match assets {
            Some(folder) => Ok(folder.join(asset)),
            None => Err(format!(
                "The edit uses the course file {asset}. Say where the course's files are, with --assets <folder>."
            )),
        },
    }
}

/// The sound of recordings and course files, read from their sound files.
pub struct FileSounds {
    project: PathBuf,
    assets: Option<PathBuf>,
    open: HashMap<EditSourceRef, WavFile>,
}

impl FileSounds {
    pub fn new(project: &Path, assets: Option<&Path>) -> Self {
        Self { project: project.to_path_buf(), assets: assets.map(Path::to_path_buf), open: HashMap::new() }
    }
}

impl SoundReader for FileSounds {
    fn read(&mut self, source: &EditSourceRef, first_frame: u64, frames: usize) -> Result<Vec<i16>, String> {
        if !self.open.contains_key(source) {
            let path = source_path(&self.project, self.assets.as_deref(), source)?;
            let file = WavFile::open(&path)
                .map_err(|e| format!("The sound file {} could not be opened: {e}", path.display()))?;
            if file.sample_rate() != 48_000 {
                return Err(format!(
                    "The sound file {} is {} samples a second. This program mixes 48,000 a second; converting other rates is not built yet.",
                    path.display(),
                    file.sample_rate()
                ));
            }
            self.open.insert(source.clone(), file);
        }
        let file = self.open.get_mut(source).expect("just opened");
        file.read(first_frame, frames).map_err(|e| format!("A sound file could not be read: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{VideoInspection, VideoSink};
    use crate::{EncoderInfo, ScreenError};
    use openstage_capture_project::{parse_edit, EditTrackKind};
    use openstage_capture_render::{BlockText, Layer, Picture, Rgb};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    fn edit(name: &str) -> Edit {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures").join(name);
        parse_edit(&fs::read_to_string(path).unwrap()).unwrap().edit
    }

    // ---- fakes ----

    /// A recorded screen of 96 by 54, and a webcam, that colour each picture by the moment of the recording it
    /// shows: the blue byte is the moment in 100 ms steps, so a test can tell which moment each frame drew.
    struct Pictures {
        asked: Vec<u64>,
        fail_after: Option<usize>,
    }
    impl PictureSource for Pictures {
        fn picture(&mut self, layer: &Layer) -> Result<Rc<Picture>, String> {
            self.asked.push(layer.source_time_ms);
            if self.fail_after.is_some_and(|n| self.asked.len() > n) {
                return Err("The recording screen.mp4 could not be read any further.".into());
            }
            let (w, h) = if layer.track_kind == EditTrackKind::Webcam { (32, 18) } else { (96, 54) };
            Ok(Rc::new(Picture::solid(w, h, Rgb(0, (layer.source_time_ms / 100) as u8, 99))))
        }
    }

    /// Sound that is a steady value, as long as the source asked for.
    struct Sounds {
        value: i16,
        fail: bool,
        reads: usize,
    }
    impl SoundReader for Sounds {
        fn read(&mut self, _: &EditSourceRef, _: u64, frames: usize) -> Result<Vec<i16>, String> {
            self.reads += 1;
            if self.fail {
                return Err("The sound file mic.wav could not be read.".into());
            }
            Ok(vec![self.value; frames * 2])
        }
    }

    #[derive(Default)]
    struct Written {
        frames: Vec<(i64, i64, u64)>,
        audio: Vec<(i64, usize, i16)>,
        finished: bool,
        spec_audio: bool,
        size: (u32, u32),
        fps: u32,
        part: PathBuf,
    }

    struct FakeSink(Rc<RefCell<Written>>);
    impl VideoSink for FakeSink {
        fn write(&mut self, frame: &Frame, pts: i64, duration: i64) -> Result<(), ScreenError> {
            // A hash of the picture, so two exports can be compared without keeping every frame.
            let mut hash = 1469598103934665603u64;
            for b in &frame.bgra {
                hash = (hash ^ *b as u64).wrapping_mul(1099511628211);
            }
            self.0.borrow_mut().frames.push((pts, duration, hash));
            Ok(())
        }
        fn write_audio(&mut self, samples: &[i16], pts: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().audio.push((pts, samples.len() / 2, samples.first().copied().unwrap_or(0)));
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            self.0.borrow_mut().finished = true;
            Ok(())
        }
    }

    struct FakeVideo(Rc<RefCell<Written>>);
    impl VideoPlatform for FakeVideo {
        fn start_media(&self) -> Result<(), ScreenError> {
            Ok(())
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(vec![])
        }
        fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            {
                let mut w = self.0.borrow_mut();
                w.spec_audio = spec.audio.is_some();
                w.size = (spec.width, spec.height);
                w.fps = spec.fps;
                w.part = spec.path.clone();
            }
            // Windows' writer chooses the kind of file from the end of its name and fails on one it does not know.
            let ends_in_mp4 = spec.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
            if !ends_in_mp4 {
                return Err(ScreenError::Failed(
                    "Could not create the video file (Windows error code 0xC00D36D5: The specified object or value does not exist.)"
                        .into(),
                ));
            }
            // The real writer makes the file as soon as it is asked to.
            fs::write(&spec.path, b"half a video").map_err(|e| ScreenError::Failed(e.to_string()))?;
            Ok(Box::new(FakeSink(Rc::clone(&self.0))))
        }
        fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            None
        }
        fn logical_cores(&self) -> u32 {
            1
        }
    }

    struct Run {
        result: Result<ExportReport, ExportError>,
        written: Rc<RefCell<Written>>,
        asked: Vec<u64>,
        progress: Vec<Progress>,
    }

    /// A quick run: 5 pictures a second, so a minute of edit is 300 frames.
    fn run(
        edit: &Edit,
        out: &Path,
        cancel_after: Option<usize>,
        pictures_fail_after: Option<usize>,
        sound_fails: bool,
    ) -> Run {
        run_at(edit, out, 5, cancel_after, pictures_fail_after, sound_fails)
    }

    fn run_at(
        edit: &Edit,
        out: &Path,
        fps: u32,
        cancel_after: Option<usize>,
        pictures_fail_after: Option<usize>,
        sound_fails: bool,
    ) -> Run {
        let written = Rc::new(RefCell::new(Written::default()));
        let video = FakeVideo(Rc::clone(&written));
        let mut pictures = Pictures { asked: Vec::new(), fail_after: pictures_fail_after };
        let mut sounds = Sounds { value: 1000, fail: sound_fails, reads: 0 };
        let job = ExportJob {
            settings: RenderSettings { fps, reduce_motion: false, ..RenderSettings::default() },
            size: None,
            bitrate_kbps: None,
            out: out.to_path_buf(),
        };
        let calls = std::cell::Cell::new(0usize);
        let cancelled = || {
            calls.set(calls.get() + 1);
            cancel_after.is_some_and(|n| calls.get() > n)
        };
        let mut progress = Vec::new();
        let result = export(
            edit,
            &job,
            ExportTools { video: &video, pictures: &mut pictures, text: &BlockText, sound: &mut sounds },
            &cancelled,
            &mut |p| progress.push(p),
        );
        Run { result, written, asked: pictures.asked, progress }
    }

    // ---- tests ----

    #[test]
    fn the_unfinished_video_is_named_so_that_windows_still_knows_it_is_an_mp4() {
        let name = |p: &str| part_path(Path::new(p)).to_string_lossy().replace('\\', "/");
        assert_eq!(name("out/lesson.mp4"), "out/lesson.part.mp4");
        assert_eq!(name("out/Lesson.MP4"), "out/Lesson.part.mp4");
        assert_eq!(name("lesson.mp4"), "lesson.part.mp4");
        assert_eq!(name("out/lesson"), "out/lesson.part.mp4", "a name with no .mp4 still gets one");
        assert_eq!(name("out/lesson.mov"), "out/lesson.mov.part.mp4");
        for p in ["a.mp4", "a.MP4", "a", "a.mov", "a.mp4.mp4"] {
            let part = part_path(Path::new(p));
            assert!(part.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")), "{part:?}");
            assert_ne!(part, Path::new(p), "never the real name");
        }
    }

    #[test]
    fn a_whole_edit_is_drawn_frame_by_frame_with_its_sound_and_put_in_place_only_when_whole() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("lesson.mp4");
        let r = run_at(&edit("valid-basic.json"), &out, 30, None, None, false);
        let report = r.result.unwrap();
        assert_eq!(
            (report.width, report.height, report.fps, report.frames),
            (96, 54, 30, 1800),
            "the size of the screen picture"
        );
        assert!(report.sound && (report.seconds - 60.0).abs() < 1e-9);
        assert!(out.is_file(), "the finished video is at its name");
        assert!(!part_path(&out).exists(), "and no half-made file is left");
        let w = r.written.borrow();
        assert!(w.finished && w.spec_audio);
        assert_eq!(w.frames.len(), 1800);
        // Frames follow one another with no gap: each starts where the one before ended.
        for pair in w.frames.windows(2) {
            assert_eq!(pair[0].0 + pair[0].1, pair[1].0);
        }
        assert_eq!(w.frames[1].0, 333_333, "frame 1 is at 33.3 ms, in units of 100 nanoseconds");
        assert_eq!(w.frames[30].0, 10_000_000, "and frame 30 is at exactly one second");
        assert_eq!(r.asked.first(), Some(&0), "the recording's first moment was asked for first, to learn its size");
        assert!(r.asked.len() >= 1800);
        // Sound: every frame's stretch is whole samples, and together they are exactly the length of the video.
        let samples: usize = w.audio.iter().map(|a| a.1).sum();
        assert_eq!(samples, 60 * 48_000);
        assert_eq!(w.audio.len(), 1800);
        assert_eq!(w.audio[0].2, 2000, "the voice and the computer's sound, both at 1000, added");
        // Progress was told after every frame, ending at the total.
        assert_eq!(r.progress.len(), 1800);
        assert_eq!(r.progress.last(), Some(&Progress { frames_done: 1800, frame_count: 1800 }));
    }

    #[test]
    fn an_edit_with_no_sound_makes_a_video_with_no_sound_track() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = edit("valid-basic.json");
        e.tracks.retain(|t| t.kind != EditTrackKind::Audio);
        let r = run(&e, &dir.path().join("quiet.mp4"), None, None, false);
        let report = r.result.unwrap();
        assert!(!report.sound);
        assert!(!r.written.borrow().spec_audio && r.written.borrow().audio.is_empty());
    }

    #[test]
    fn the_same_edit_exported_twice_hands_the_writer_the_same_pictures_and_sound() {
        let dir = tempfile::tempdir().unwrap();
        let first = run(&edit("valid-with-overlays.json"), &dir.path().join("a.mp4"), None, None, false);
        let second = run(&edit("valid-with-overlays.json"), &dir.path().join("b.mp4"), None, None, false);
        first.result.unwrap();
        second.result.unwrap();
        assert_eq!(first.written.borrow().frames, second.written.borrow().frames);
        assert_eq!(first.written.borrow().audio, second.written.borrow().audio);
    }

    #[test]
    fn cancelling_leaves_nothing_behind_and_does_not_touch_an_earlier_export() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("lesson.mp4");
        fs::write(&out, b"the earlier export").unwrap();
        let r = run(&edit("valid-basic.json"), &out, Some(100), None, false);
        assert_eq!(r.result.unwrap_err(), ExportError::Cancelled);
        assert_eq!(fs::read(&out).unwrap(), b"the earlier export", "the earlier video is as it was");
        assert!(!part_path(&out).exists(), "and the half-made file is gone");
        assert_eq!(r.written.borrow().frames.len(), 100, "it stopped when asked");
        assert!(!r.written.borrow().finished);
        assert_eq!(ExportError::Cancelled.to_string(), "The export was cancelled. No video file was made.");
    }

    #[test]
    fn a_cancelled_export_with_no_earlier_video_makes_no_file_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("new.mp4");
        let r = run(&edit("valid-basic.json"), &out, Some(0), None, false);
        assert_eq!(r.result.unwrap_err(), ExportError::Cancelled);
        assert!(!out.exists() && !part_path(&out).exists());
    }

    #[test]
    fn a_recording_that_cannot_be_read_part_way_ends_the_export_in_words_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("lesson.mp4");
        let r = run(&edit("valid-basic.json"), &out, None, Some(50), false);
        let e = r.result.unwrap_err();
        assert_eq!(e, ExportError::Failed("The recording screen.mp4 could not be read any further.".into()));
        assert!(!out.exists() && !part_path(&out).exists());
    }

    #[test]
    fn a_sound_file_that_cannot_be_read_ends_the_export_in_words_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("lesson.mp4");
        let r = run(&edit("valid-basic.json"), &out, None, None, true);
        assert_eq!(r.result.unwrap_err(), ExportError::Failed("The sound file mic.wav could not be read.".into()));
        assert!(!out.exists() && !part_path(&out).exists());
    }

    #[test]
    fn an_edit_that_is_not_finished_is_refused_before_anything_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("lesson.mp4");
        let r = run(&edit("needs-attention.json"), &out, None, None, false);
        let ExportError::Failed(message) = r.result.unwrap_err() else { panic!("should fail") };
        assert!(message.starts_with("The edit is not finished:"), "{message}");
        assert!(r.written.borrow().frames.is_empty());
        assert!(!out.exists() && !part_path(&out).exists());
    }

    #[test]
    fn an_edit_with_errors_is_refused_with_the_first_problem() {
        let dir = tempfile::tempdir().unwrap();
        let r = run(&edit("overlap.json"), &dir.path().join("x.mp4"), None, None, false);
        let ExportError::Failed(message) = r.result.unwrap_err() else { panic!("should fail") };
        assert!(message.starts_with("The edit cannot be rendered:"), "{message}");
    }

    #[test]
    fn a_folder_that_does_not_exist_yet_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("exports").join("week1").join("lesson.mp4");
        let r = run(&edit("valid-basic.json"), &out, Some(3), None, false);
        assert_eq!(r.result.unwrap_err(), ExportError::Cancelled);
        assert!(out.parent().unwrap().is_dir());
    }

    #[test]
    fn the_size_asked_for_wins_over_the_size_of_the_picture() {
        let dir = tempfile::tempdir().unwrap();
        let written = Rc::new(RefCell::new(Written::default()));
        let video = FakeVideo(Rc::clone(&written));
        let mut pictures = Pictures { asked: Vec::new(), fail_after: None };
        let mut sounds = Sounds { value: 0, fail: false, reads: 0 };
        let job = ExportJob {
            settings: RenderSettings { fps: 5, ..RenderSettings::default() },
            size: Some((128, 72)),
            bitrate_kbps: Some(3000),
            out: dir.path().join("small.mp4"),
        };
        let mut e = edit("valid-basic.json");
        e.tracks.retain(|t| t.kind != EditTrackKind::Audio);
        let report = export(
            &e,
            &job,
            ExportTools { video: &video, pictures: &mut pictures, text: &BlockText, sound: &mut sounds },
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!((report.width, report.height), (128, 72));
        assert_eq!(written.borrow().size, (128, 72));
    }

    #[test]
    fn the_quality_suits_the_size() {
        assert_eq!(bitrate_for(1920, 1080), 8000);
        assert_eq!(bitrate_for(1280, 720), 3555);
        assert_eq!(bitrate_for(96, 54), 1500, "never below 1.5 megabits");
        assert_eq!(bitrate_for(3840, 2160), 16_000, "and never above 16");
    }

    #[test]
    fn text_that_does_not_fit_is_reported_once_however_many_frames_show_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = edit("valid-with-overlays.json");
        // Make the title's box far too small for its words.
        for track in &mut e.tracks {
            for item in &mut track.items {
                if let openstage_capture_project::EditItem::Text { rect, .. } = item {
                    rect.width = 300.0;
                    rect.height = 300.0;
                }
            }
        }
        let report = run(&e, &dir.path().join("x.mp4"), None, None, false).result.unwrap();
        assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
        assert!(report.notes[0].contains("does not fit in its box"));
    }

    #[test]
    fn paths_are_found_for_recordings_and_for_course_files() {
        let project = Path::new("D:/lessons/week1.capture");
        let take = EditSourceRef::Take { take: 2, media: "mic.wav".into() };
        assert_eq!(source_path(project, None, &take).unwrap(), project.join("takes").join("take-002").join("mic.wav"));
        let asset = EditSourceRef::Asset { asset: "music.wav".into() };
        assert_eq!(
            source_path(project, Some(Path::new("D:/course")), &asset).unwrap(),
            Path::new("D:/course").join("music.wav")
        );
        assert!(source_path(project, None, &asset).unwrap_err().contains("--assets <folder>"));
    }

    #[test]
    fn sound_is_read_from_the_files_of_a_take() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("p.capture");
        let take_dir = openstage_capture_project::take_dir(&project, 1);
        fs::create_dir_all(&take_dir).unwrap();
        let mut w = openstage_capture_project::WavWriter::create(&take_dir.join("mic.wav"), 48_000, 2).unwrap();
        w.append(&vec![321i16; 4800 * 2]).unwrap();
        w.finish().unwrap();
        let mut sounds = FileSounds::new(&project, None);
        let mic = EditSourceRef::Take { take: 1, media: "mic.wav".into() };
        assert_eq!(sounds.read(&mic, 100, 4).unwrap(), vec![321i16; 8]);
        assert!(sounds.read(&mic, 4799, 10).unwrap().len() == 2, "only what the file holds");
        let missing = EditSourceRef::Take { take: 1, media: "system.wav".into() };
        assert!(sounds.read(&missing, 0, 4).unwrap_err().contains("could not be opened"));

        // A sound at another rate is refused in words, not played at the wrong speed.
        let mut w = openstage_capture_project::WavWriter::create(&take_dir.join("odd.wav"), 44_100, 2).unwrap();
        w.append(&[0i16; 8]).unwrap();
        w.finish().unwrap();
        let odd = EditSourceRef::Take { take: 1, media: "odd.wav".into() };
        assert!(sounds.read(&odd, 0, 4).unwrap_err().contains("44100 samples a second"));
    }

    #[test]
    fn the_kind_of_source_decides_nothing_here_the_plan_does() {
        // A webcam-only edit is drawn too: the webcam fills the frame.
        let dir = tempfile::tempdir().unwrap();
        let mut e = edit("valid-placement.json");
        e.tracks.retain(|t| t.kind == EditTrackKind::Webcam);
        let r = run(&e, &dir.path().join("w.mp4"), Some(5), None, false);
        assert_eq!(r.result.unwrap_err(), ExportError::Cancelled);
        assert_eq!(r.written.borrow().size, (32, 18), "the size of the webcam picture, since it is the only picture");
    }
}
