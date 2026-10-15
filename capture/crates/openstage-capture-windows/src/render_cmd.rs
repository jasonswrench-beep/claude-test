// SPDX-License-Identifier: Apache-2.0
//! The `render` command: turn a recording, or an edit, into a finished MP4 (story F1).
//!
//! It chooses the edit (the project's `edit.json`, another file, or one made from a take as recorded), exports it
//! with [`export`], says in plain words what it made, and then does what the recorder does after every recording:
//! reads the finished video back through Windows, the way a player would, and checks that its length, its size and
//! its sound are what they should be. The pieces that need Windows are passed in, so all of it is tested here.

use crate::export::{export, ExportError, ExportJob, ExportReport, ExportTools, Progress};
use crate::make_edit::{edit_from_take, EditOptions};
use crate::video::{compare_audio_readback, compare_readback, compare_size_readback};
use crate::{check, Check};
use openstage_capture_project::{parse_edit, Edit, EditSeverity};
use openstage_capture_render::RenderSettings;
use std::path::PathBuf;

/// What to make.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// The capture project folder (where the recordings are).
    pub project: PathBuf,
    /// Where the video goes.
    pub out: PathBuf,
    /// The edit to use, if not the project's own `edit.json`.
    pub edit_file: Option<PathBuf>,
    /// Make an edit from this take as it was recorded, instead of reading one.
    pub take: Option<u32>,
    /// With `take`: add one of every kind of overlay and a zoom, to see that they are drawn.
    pub demo: bool,
    /// Where the course's own files are (pictures and sounds an edit imports).
    pub assets: Option<PathBuf>,
    pub fps: u32,
    /// The video's size, if not the size of the first picture.
    pub size: Option<(u32, u32)>,
    /// Use cuts instead of animated zooms.
    pub reduce_motion: bool,
    /// Export even if the edit still lacks things OpenStage requires (alt text, descriptions).
    pub allow_unfinished: bool,
}

fn say(report: &mut dyn FnMut(Check), name: &str, ok: bool, message: impl Into<String>) -> bool {
    report(check(name, ok, message));
    ok
}

/// Choose the edit. `Err` is the message to give the author.
fn choose_edit(opts: &RenderOptions) -> Result<(Edit, String), String> {
    if opts.demo && opts.take.is_none() {
        return Err("--demo adds overlays to a take as recorded, so it needs --take <number>.".to_string());
    }
    if let Some(take) = opts.take {
        if opts.edit_file.is_some() {
            return Err(
                "Use either --take (export a take as recorded) or --edit (export an edit file), not both.".to_string()
            );
        }
        let edit = edit_from_take(&opts.project, take, EditOptions { demo: opts.demo })?;
        let what = if opts.demo {
            format!("take {take} as recorded, with a title, a box, an arrow, a blur, a blacked-out box and a zoom added to test them")
        } else {
            format!("take {take} as it was recorded")
        };
        return Ok((edit, what));
    }
    let path = opts.edit_file.clone().unwrap_or_else(|| opts.project.join("edit.json"));
    if !path.is_file() {
        return Err(format!(
            "There is no edit to export: {} does not exist. To export a recording as it was made, use --take <number>.",
            path.display()
        ));
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("The edit {} could not be read: {e}", path.display()))?;
    let parsed = parse_edit(&text)?;
    if let Some(error) = parsed.problems.iter().find(|p| p.severity == EditSeverity::Error) {
        return Err(format!("The edit {} cannot be exported: {}", path.display(), error.message));
    }
    Ok((parsed.edit, format!("the edit in {}", path.display())))
}

fn seconds_text(seconds: f64) -> String {
    format!("{seconds:.1} seconds")
}

/// Run the command with everything it needs supplied. Reports checks as it goes; `progress` is told how far the
/// export has got (about every tenth of the way, plus the end). Returns whether the export worked and the finished
/// video read back as it should. Notes about the result (text that did not fit) are reported as `export-note`
/// checks that are not ok; they do not change the answer, because the exporter did its job.
pub fn run_render_with(
    opts: &RenderOptions,
    tools: ExportTools,
    cancelled: &dyn Fn() -> bool,
    report: &mut dyn FnMut(Check),
    progress: &mut dyn FnMut(Progress),
) -> bool {
    let (edit, what) = match choose_edit(opts) {
        Ok(chosen) => chosen,
        Err(message) => return say(report, "edit", false, message),
    };
    say(report, "edit", true, format!("Exporting {what}, {} long.", seconds_text(edit.length_ms() / 1000.0)));

    let job = ExportJob {
        settings: RenderSettings {
            fps: opts.fps,
            reduce_motion: opts.reduce_motion,
            require_complete: !opts.allow_unfinished,
            ..RenderSettings::default()
        },
        size: opts.size,
        bitrate_kbps: None,
        out: opts.out.clone(),
    };
    let video = tools.video;
    let outcome = export(&edit, &job, tools, cancelled, &mut |p| {
        // Told after every frame; passed on only about every tenth of the way, and at the end.
        let tenth = (p.frame_count / 10).max(1);
        if p.frames_done % tenth == 0 || p.frames_done == p.frame_count {
            progress(p);
        }
    });
    let finished: ExportReport = match outcome {
        Ok(done) => done,
        Err(ExportError::Cancelled) => return say(report, "export", false, ExportError::Cancelled.to_string()),
        Err(ExportError::Failed(why)) => return say(report, "export", false, why),
    };
    say(
        report,
        "export",
        true,
        format!(
            "Exported {} of video, {} pictures at {} by {} pixels, {} a second{}: {}",
            seconds_text(finished.seconds),
            finished.frames,
            finished.width,
            finished.height,
            finished.fps,
            if finished.sound { ", with sound" } else { ", with no sound" },
            finished.out.display()
        ),
    );

    // Read the finished video back the way a player would.
    let read = video.inspect_video(&finished.out);
    let recorded_ms = (finished.seconds * 1000.0).round() as u64;
    let mut ok = true;
    let mut findings = vec![compare_readback(
        read.as_ref().map(Clone::clone).map_err(|e| crate::ScreenError::Failed(e.to_string())),
        recorded_ms,
    )];
    findings.push(compare_size_readback(&read, (finished.width, finished.height)));
    if finished.sound {
        findings.push(compare_audio_readback(&read, recorded_ms));
    }
    for finding in findings {
        ok &= finding.ok;
        report(finding);
    }
    for note in &finished.notes {
        report(check("export-note", false, note.clone()));
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::SoundReader;
    use crate::video::{AudioInspection, SinkSpec, VideoInspection, VideoPlatform, VideoSink};
    use crate::{EncoderInfo, Frame, ScreenError};
    use openstage_capture_project::{
        create_project, create_take_with, finish_take, EditSourceRef, EditTrackKind, MediaFile,
    };
    use openstage_capture_render::{BlockText, Layer, Picture, PictureSource, Rgb};
    use std::cell::RefCell;
    use std::fs;
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Duration;

    struct Pictures;
    impl PictureSource for Pictures {
        fn picture(&mut self, layer: &Layer) -> Result<Rc<Picture>, String> {
            let (w, h) = if layer.track_kind == EditTrackKind::Webcam { (32, 18) } else { (64, 36) };
            Ok(Rc::new(Picture::solid(w, h, Rgb(0, 80, 160))))
        }
    }
    struct Sounds;
    impl SoundReader for Sounds {
        fn read(&mut self, _: &EditSourceRef, _: u64, frames: usize) -> Result<Vec<i16>, String> {
            Ok(vec![500; frames * 2])
        }
    }

    /// A video writer that remembers what it was given and says it read back whatever it wrote, unless told to lie.
    #[derive(Default)]
    struct State {
        frames: u64,
        audio_frames: u64,
        size: (u32, u32),
        fps: u32,
        sound: bool,
        shorten_by_ms: u64,
        lose_sound: bool,
    }
    struct Sink(Rc<RefCell<State>>);
    impl VideoSink for Sink {
        fn write(&mut self, _: &Frame, _: i64, _: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().frames += 1;
            Ok(())
        }
        fn write_audio(&mut self, samples: &[i16], _: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().audio_frames += samples.len() as u64 / 2;
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            Ok(())
        }
    }
    struct Video(Rc<RefCell<State>>);
    impl VideoPlatform for Video {
        fn start_media(&self) -> Result<(), ScreenError> {
            Ok(())
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(vec![])
        }
        fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            {
                let mut s = self.0.borrow_mut();
                s.size = (spec.width, spec.height);
                s.fps = spec.fps;
                s.sound = spec.audio.is_some();
            }
            fs::write(&spec.path, b"video").map_err(|e| ScreenError::Failed(e.to_string()))?;
            Ok(Box::new(Sink(Rc::clone(&self.0))))
        }
        fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
            let s = self.0.borrow();
            let duration = (s.frames * 1000 / s.fps.max(1) as u64).saturating_sub(s.shorten_by_ms);
            Ok(VideoInspection {
                frames: s.frames,
                duration_ms: duration,
                audio: (s.sound && !s.lose_sound)
                    .then(|| AudioInspection { samples: 10, duration_ms: s.audio_frames * 1000 / 48_000 }),
                size: Some(s.size),
            })
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            None
        }
        fn logical_cores(&self) -> u32 {
            1
        }
    }

    fn media(file: &str, kind: &str) -> MediaFile {
        MediaFile {
            file: file.into(),
            kind: kind.into(),
            container: "wav".into(),
            mix: None,
            region: None,
            camera: None,
            interactions: None,
        }
    }

    fn project(seconds: u64) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("p.capture");
        create_project(&project, "t", "en-US").unwrap();
        let files = vec![media("screen.mp4", "screen"), media("mic.wav", "microphone"), media("webcam.mp4", "webcam")];
        create_take_with(&project, 1, vec!["screen:1".into()], None, 2000, files).unwrap();
        finish_take(&project, 1, seconds * 1000, 3).unwrap();
        (dir, project)
    }

    fn options(project: &Path, out: &Path) -> RenderOptions {
        RenderOptions {
            project: project.to_path_buf(),
            out: out.to_path_buf(),
            edit_file: None,
            take: Some(1),
            demo: false,
            assets: None,
            fps: 10,
            size: None,
            reduce_motion: false,
            allow_unfinished: false,
        }
    }

    struct Outcome {
        ok: bool,
        checks: Vec<Check>,
        progress: Vec<Progress>,
        state: Rc<RefCell<State>>,
    }

    fn render(opts: &RenderOptions, tune: impl FnOnce(&mut State), cancel_after: Option<usize>) -> Outcome {
        let state = Rc::new(RefCell::new(State::default()));
        tune(&mut state.borrow_mut());
        let video = Video(Rc::clone(&state));
        let mut pictures = Pictures;
        let mut sounds = Sounds;
        let mut checks = Vec::new();
        let mut progress = Vec::new();
        let calls = std::cell::Cell::new(0usize);
        let cancelled = || {
            calls.set(calls.get() + 1);
            cancel_after.is_some_and(|n| calls.get() > n)
        };
        let ok = run_render_with(
            opts,
            ExportTools { video: &video, pictures: &mut pictures, text: &BlockText, sound: &mut sounds },
            &cancelled,
            &mut |c| checks.push(c),
            &mut |p| progress.push(p),
        );
        Outcome { ok, checks, progress, state }
    }

    fn find<'a>(o: &'a Outcome, name: &str) -> &'a Check {
        o.checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {:?}", o.checks))
    }

    #[test]
    fn a_take_is_exported_as_recorded_and_read_back_in_words() {
        let (dir, project) = project(10);
        let out = dir.path().join("lesson.mp4");
        let o = render(&options(&project, &out), |_| {}, None);
        assert!(o.ok, "{:?}", o.checks);
        let names: Vec<&str> = o.checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(names, ["edit", "export", "video-readback", "video-size", "audio-readback"]);
        assert_eq!(find(&o, "edit").message, "Exporting take 1 as it was recorded, 10.0 seconds long.");
        assert!(
            find(&o, "export").message.starts_with(
                "Exported 10.0 seconds of video, 100 pictures at 64 by 36 pixels, 10 a second, with sound: "
            ),
            "{}",
            find(&o, "export").message
        );
        assert!(out.is_file());
        assert_eq!(o.state.borrow().size, (64, 36));
    }

    #[test]
    fn progress_is_told_about_every_tenth_of_the_way_and_at_the_end() {
        let (dir, project) = project(10);
        let o = render(&options(&project, &dir.path().join("x.mp4")), |_| {}, None);
        let percents: Vec<u64> = o.progress.iter().map(|p| p.frames_done * 100 / p.frame_count).collect();
        assert_eq!(percents, [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
    }

    #[test]
    fn the_demo_edit_exports_too_and_says_what_was_added() {
        let (dir, project) = project(10);
        let mut opts = options(&project, &dir.path().join("demo.mp4"));
        opts.demo = true;
        let o = render(&opts, |_| {}, None);
        assert!(o.ok, "{:?}", o.checks);
        assert!(find(&o, "edit")
            .message
            .contains("a title, a box, an arrow, a blur, a blacked-out box and a zoom added to test them"));
    }

    #[test]
    fn a_cancelled_export_says_so_and_fails() {
        let (dir, project) = project(10);
        let out = dir.path().join("x.mp4");
        let o = render(&options(&project, &out), |_| {}, Some(20));
        assert!(!o.ok);
        assert_eq!(find(&o, "export").message, "The export was cancelled. No video file was made.");
        assert!(!out.exists() && !dir.path().join("x.mp4.part").exists());
    }

    #[test]
    fn no_edit_yet_is_explained_with_what_to_do() {
        let (dir, project) = project(10);
        let mut opts = options(&project, &dir.path().join("x.mp4"));
        opts.take = None;
        let o = render(&opts, |_| {}, None);
        assert!(!o.ok);
        let m = &find(&o, "edit").message;
        assert!(m.contains("There is no edit to export") && m.contains("--take <number>"), "{m}");
    }

    #[test]
    fn an_edit_file_is_read_and_a_bad_one_is_refused_in_words() {
        let (dir, project) = project(10);
        let good = dir.path().join("edit.json");
        fs::write(
            &good,
            fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures/valid-basic.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut opts = options(&project, &dir.path().join("x.mp4"));
        opts.take = None;
        opts.edit_file = Some(good);
        opts.fps = 5;
        let o = render(&opts, |_| {}, None);
        assert!(o.ok, "{:?}", o.checks);
        assert!(find(&o, "edit").message.starts_with("Exporting the edit in "));

        let bad = dir.path().join("bad.json");
        fs::write(
            &bad,
            fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures/overlap.json"),
            )
            .unwrap(),
        )
        .unwrap();
        opts.edit_file = Some(bad);
        let o = render(&opts, |_| {}, None);
        assert!(!o.ok && find(&o, "edit").message.contains("cannot be exported"), "{:?}", o.checks);

        let junk = dir.path().join("junk.json");
        fs::write(&junk, "nonsense").unwrap();
        opts.edit_file = Some(junk);
        assert!(find(&render(&opts, |_| {}, None), "edit").message.contains("not a readable edit file"));
    }

    #[test]
    fn a_choice_that_makes_no_sense_is_refused() {
        let (dir, project) = project(10);
        let mut opts = options(&project, &dir.path().join("x.mp4"));
        opts.take = None;
        opts.demo = true;
        assert!(find(&render(&opts, |_| {}, None), "edit")
            .message
            .contains("--demo adds overlays to a take as recorded, so it needs --take"));
        let mut opts = options(&project, &dir.path().join("x.mp4"));
        opts.edit_file = Some(dir.path().join("e.json"));
        assert!(find(&render(&opts, |_| {}, None), "edit").message.contains("either --take"));
    }

    #[test]
    fn a_finished_video_that_reads_back_wrong_is_a_problem_even_though_it_was_written() {
        let (dir, project) = project(10);
        let shorter = render(&options(&project, &dir.path().join("a.mp4")), |s| s.shorten_by_ms = 3000, None);
        assert!(!shorter.ok);
        assert!(!find(&shorter, "video-readback").ok);
        let silent = render(&options(&project, &dir.path().join("b.mp4")), |s| s.lose_sound = true, None);
        assert!(!silent.ok);
        assert!(!find(&silent, "audio-readback").ok);
    }

    #[test]
    fn an_edit_that_lacks_things_is_exported_only_when_allowed() {
        let (dir, project) = project(10);
        let needs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures/needs-attention.json");
        let file = dir.path().join("needs.json");
        fs::write(&file, fs::read_to_string(needs).unwrap()).unwrap();
        let mut opts = options(&project, &dir.path().join("x.mp4"));
        opts.take = None;
        opts.edit_file = Some(file);
        opts.fps = 5;
        let refused = render(&opts, |_| {}, None);
        assert!(!refused.ok && find(&refused, "export").message.starts_with("The edit is not finished:"));
        opts.allow_unfinished = true;
        let allowed = render(&opts, |_| {}, None);
        assert!(allowed.ok, "{:?}", allowed.checks);
    }
}
