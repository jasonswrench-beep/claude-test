// SPDX-License-Identifier: Apache-2.0
//! `openstage-capture`: the recording program.
//!
//! It has no window of its own. OpenStage starts it, sends it one-line JSON commands on standard
//! input, and reads one-line JSON events from standard output (see `docs/capture-protocol-spec.md`).
//! Standard error is for diagnostics only.
//!
//! Exit codes: 0 success, 1 something went wrong, 2 not available (an unknown source, or a
//! command that has not been built yet).

mod checks;
mod demo;
mod plain;

#[cfg(windows)]
use checks::emit_progress;
use checks::{emit_check, finish_checks, finish_checks_with, run_recorder_crash_test, run_video_crash_test};
use clap::{Parser, Subcommand};
use openstage_capture::{
    run_session_with_cues, CuePlayer, EventSink, Input, InputSource, MediaFactory, NoCues, ScriptedInput,
    SessionConfig, Source, SyntheticSource, SystemClock,
};
use openstage_capture_project::{recover_project, take_dir};
use openstage_capture_protocol::{decode_command, encode_event, Event, ProtocolError, SourceInfo, MAX_LINE_BYTES};
#[cfg(windows)]
use openstage_capture_windows::{
    cancel_requested, install_cancel_handler, run_render_with, ExportTools, FileSounds, RenderOptions, WindowsPictures,
    WindowsText,
};
use openstage_capture_windows::{
    check_balance_in_files, check_echo_in_files, check_sound_files, choose_camera, choose_window,
    compare_audio_readback, compare_readback, compare_size_readback, compare_webcam_readback, describe_cameras,
    describe_devices, output_may_have_changed, plan_sound, run_audio_test, run_cue_test, run_find_output,
    run_pick_region, run_screen_test, run_setup_check, run_timing_test, run_video_test, summarize,
    system_audio_platform, system_camera_platform, system_cue_player, system_platform, system_video_platform,
    AudioChoice, AudioTestOptions, Check, CueTestOptions, DeviceKind, FindOutputOptions, Mix, PickRegionOptions,
    Region, ScreenMedia, ScreenTestOptions, SetupOptions, SoundPlan, TimingTestOptions, VideoTestOptions, WindowInfo,
    MIC_FILE, SCREEN_FILE, SYSTEM_FILE, WEBCAM_FILE,
};
use plain::PlainSink;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Parser)]
#[command(name = "openstage-capture", version, about = "Records the screen for OpenStage. Has no window of its own.")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
// The command line is read once, so one big variant (Record has many options) costs nothing.
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Say what this program can record on this computer.
    Probe,
    /// Record into a capture project folder, taking commands on standard input.
    Record {
        /// The capture project folder (created if it does not exist).
        #[arg(long)]
        project: PathBuf,
        /// Where picture and sound come from: `screen` (your real screen, Windows only) or `synthetic`
        /// (a built-in test source that makes fake picture and sound and works anywhere).
        #[arg(long, default_value = "synthetic")]
        source: String,
        /// With `--source screen`: which display to record (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// With `--source screen`: record only part of the display, as four whole numbers in pixels from
        /// the display's top left corner: how far from the left, how far from the top, the width and the
        /// height, for example `100,200,1280,720`. An odd width or height is made one pixel smaller.
        #[arg(long)]
        region: Option<String>,
        /// With `--source screen`: record just one window instead of a display: its number from the
        /// `windows` command, or part of its title or program name. Nothing else on the screen is in the
        /// video, whatever is in front of the window. Cannot be used with `--region`.
        #[arg(long)]
        window: Option<String>,
        /// With `--source screen`: pictures per second in the video.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// With `--source screen`: video quality in kilobits per second.
        #[arg(long, default_value_t = 8000)]
        bitrate_kbps: u32,
        /// With `--source screen`: which microphone to record. Its number from `audio-devices`, part of
        /// its name, `default`, or `none` to record no microphone.
        #[arg(long, default_value = "default")]
        microphone: String,
        /// With `--source screen`: which sound output's sound to record (the computer's own sound,
        /// including a screen reader's speech). Its number from `audio-devices`, part of its name,
        /// `default`, or `none` to record none of it. With `default` the recording follows Windows if
        /// the sound moves to another output (headphones plugged in); a number or name is kept to.
        #[arg(long, default_value = "default")]
        output: String,
        /// With `--source screen`: a webcam to record too, into `webcam.mp4` beside the screen video: its number
        /// from the `cameras` command, part of its name, `default` for the first, or `none` (the default) to
        /// record no webcam. A camera is private, so it is off unless asked for. Each picture is placed where
        /// the camera says it was taken, so it lines up with your voice. A camera that stops does not stop the
        /// recording.
        #[arg(long, default_value = "none")]
        camera: String,
        /// With `--source screen`: make the microphone louder (positive) or quieter (negative) by this
        /// many decibels in the mix inside the video. 0 leaves it as heard; 6 is about double the volume.
        /// The separate `mic.wav` is never changed.
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        mic_gain_db: i32,
        /// With `--source screen`: the same for the computer's sound. Use a negative number to make it
        /// quieter, for example -10. The separate `system.wav` is never changed.
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        system_gain_db: i32,
        /// Whether the recorder makes a short sound for each step (countdown, start, pause, resume, stop,
        /// error), so an author who cannot see the screen always knows what it is doing: `on` or `off`.
        /// The sounds are never recorded.
        #[arg(long, default_value = "on")]
        cues: String,
        /// How loud those sounds are, from 0 (silent) to 100.
        #[arg(long, default_value_t = 70, value_parser = clap::value_parser!(u8).range(0..=100))]
        cue_volume: u8,
        /// With `--region`: whether a thin frame is shown on the screen round the region from the start of
        /// the countdown until the recording ends, so you can see what is being kept: `on` or `off`. The
        /// frame is drawn just outside the region and is never in the video.
        #[arg(long, default_value = "on")]
        outline: String,
        /// Count the summary findings (skipped pictures, processor use, memory growth, sound drifting
        /// from the picture) as part of passing or failing. Meant for long test recordings.
        #[arg(long)]
        strict: bool,
        /// Milliseconds of countdown before recording starts. 0 turns it off.
        #[arg(long, default_value_t = 3000)]
        countdown_ms: u64,
        /// Milliseconds of recording per segment file.
        #[arg(long, default_value_t = 2000)]
        segment_ms: u64,
        /// How often data is forced to disk. A crash can lose at most this much.
        #[arg(long, default_value_t = 1000)]
        flush_ms: u64,
        /// Stop by itself after this much recording time (for tests and demos).
        #[arg(long)]
        duration_ms: Option<u64>,
        /// Keep recording if standard input closes (the default is to stop gracefully).
        #[arg(long)]
        ignore_stdin_eof: bool,
        #[arg(long, default_value = "")]
        title: String,
        #[arg(long, default_value = "en-US")]
        language: String,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
        /// Instead of reading commands on standard input, follow a script such as
        /// `pause@7000;resume@9000;marker:look here@11000;stop@13000` (times in milliseconds from the
        /// start of the session, countdown included). For demos and tests.
        #[arg(long)]
        demo_script: Option<String>,
    },
    /// Check whether this computer is ready to record: the Windows version, screen capture, the video
    /// and sound parts MP4 needs, the microphone and its permission, and free disk space. Looks only;
    /// records nothing and asks for nothing. Windows only.
    SetupCheck {
        /// Where recordings will be kept; the free space of its drive is reported.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Choose the part of a display to record: drag a rectangle with corner handles on the dimmed screen,
    /// or type its four numbers in a small window (which works with the keyboard and a screen reader).
    /// Prints the numbers `--region` takes. Windows only.
    PickRegion {
        /// Which display to choose from (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// Start from this rectangle (four numbers, as for `--region`) instead of the middle of the display.
        #[arg(long)]
        region: Option<String>,
        /// Save the chosen numbers in this file, as `left,top,width,height` on one line.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Play the recorder's own sounds one by one, saying in words what each is for, so you can hear
    /// whether they are clear and comfortable. Plays on Windows; saves each as a WAV file everywhere.
    CueTest {
        /// How loud, from 0 (silent) to 100.
        #[arg(long, default_value_t = 70, value_parser = clap::value_parser!(u8).range(0..=100))]
        volume: u8,
        /// The folder to also save each sound in, as a file (created if needed).
        #[arg(long, default_value = "cue-test-output")]
        out: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Repair a recording that was interrupted.
    Recover {
        #[arg(long)]
        project: PathBuf,
    },
    /// Try screen capture on this computer: find the displays, capture a few seconds, and save the
    /// first and last pictures so they can be looked at. Windows only. Does not make a project.
    ScreenTest {
        /// Which display to capture (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// How long to capture.
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
        /// The folder to save the pictures in (created if needed).
        #[arg(long, default_value = "screen-test-output")]
        out: PathBuf,
        /// Draw the mouse pointer in the pictures. Off by default: the pointer is recorded separately.
        #[arg(long)]
        with_cursor: bool,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Find which sound output the sound is playing through: listen to every output at the same time
    /// and say which carry sound. Start something playing first. Windows only.
    FindOutput {
        /// How long to listen.
        #[arg(long, default_value_t = 6.0)]
        seconds: f64,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// List the open windows that can be recorded, numbered, so one can be chosen with `record --window`.
    /// Windows that are minimized or hidden, and windows without a title, are not listed. Windows only.
    Windows {
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// List the cameras this computer has, numbered, to choose one with `record --camera`. Windows only.
    Cameras {
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// List the microphones and sound outputs this computer has, and say which are the defaults.
    /// Windows only.
    AudioDevices {
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Try listening to the microphone and to what the computer is playing, for a few seconds, and
    /// save what was heard as sound files to listen to. Windows only. Does not make a project.
    AudioTest {
        /// How long to listen.
        #[arg(long, default_value_t = 8.0)]
        seconds: f64,
        /// The folder to save `mic.wav`, `system.wav`, and `both.wav` in (created if needed).
        #[arg(long, default_value = "audio-test-output")]
        out: PathBuf,
        /// Which microphone: its number from `audio-devices`, part of its name, or `default`.
        #[arg(long, default_value = "default")]
        microphone: String,
        /// Which sound output to listen to: its number from `audio-devices`, part of its name, or `default`.
        #[arg(long, default_value = "default")]
        output: String,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Measure how late the picture and the sound reach this program after Windows captures them, and
    /// whether the two clocks agree. Keep a window moving and play some sound. Windows only. Does not
    /// make a project.
    TimingTest {
        /// Which display to capture (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// How long to listen.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        /// Which microphone: its number from `audio-devices`, part of its name, or `default`.
        #[arg(long, default_value = "default")]
        microphone: String,
        /// Which sound output to listen to: its number from `audio-devices`, part of its name, or `default`.
        #[arg(long, default_value = "default")]
        output: String,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Try making an MP4 video of the screen on this computer: capture a few seconds at a steady
    /// frame rate, encode it as H.264, and report how hard the computer worked. Windows only.
    /// Does not make a project and records no sound.
    VideoTest {
        /// Which display to capture (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// How long to record.
        #[arg(long, default_value_t = 8.0)]
        seconds: f64,
        /// Pictures per second in the video.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// Video quality in kilobits per second.
        #[arg(long, default_value_t = 8000)]
        bitrate_kbps: u32,
        /// Write a crash-safe fragmented MP4 (`test_fragmented.mp4`) instead of an ordinary one.
        #[arg(long)]
        fragmented: bool,
        /// Create this file at the moment recording begins (used by `video-crash-test` to time things).
        #[arg(long, hide = true)]
        start_marker: Option<PathBuf>,
        /// The folder to save the video in (created if needed).
        #[arg(long, default_value = "video-test-output")]
        out: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Open a saved video the way a player would and say how many pictures and seconds it holds.
    /// Windows only.
    VideoInfo {
        /// The video file.
        #[arg(long)]
        file: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Test crash safety: record a crash-safe video, kill the program without warning partway
    /// through, then repair the file and report how much of the recording survived. Windows only.
    VideoCrashTest {
        /// Which display to record (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// How long to record before the program is killed.
        #[arg(long, default_value_t = 8.0)]
        seconds_before_kill: f64,
        /// The folder for the video file (created if needed).
        #[arg(long, default_value = "video-crash-output")]
        out: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Test crash safety of the real recorder: record the screen and sound through a pause, kill the
    /// program without warning, repair the project, and report how much of each file survived.
    /// Windows only.
    RecorderCrashTest {
        /// Which display to record (1 is the main display).
        #[arg(long, default_value_t = 1)]
        display: u32,
        /// Which microphone: its number from `audio-devices`, part of its name, `default`, or `none`.
        #[arg(long, default_value = "default")]
        microphone: String,
        /// Which sound output to record: its number from `audio-devices`, part of its name, `default`, or `none`.
        #[arg(long, default_value = "default")]
        output: String,
        /// How long to record before the program is killed.
        #[arg(long, default_value_t = 12.0)]
        seconds_before_kill: f64,
        /// The folder for the test recording (created if needed).
        #[arg(long, default_value = "recorder-crash-output")]
        out: PathBuf,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
    /// Turn a recording, or an edit, into a finished MP4 (Windows only). Draws every frame (the screen, zooms, blur,
    /// blacked-out boxes, shapes, arrows, the webcam and text), mixes the sound, writes the video to `<out>.part`,
    /// and puts it at `<out>` only when it is whole, so a cancelled export leaves nothing behind and an earlier
    /// video at `<out>` is never damaged. Press Ctrl+C to cancel. Afterwards it reads the video back through
    /// Windows and checks its length, size and sound.
    Render {
        /// The capture project folder (where the recordings are).
        #[arg(long)]
        project: PathBuf,
        /// Where the finished video goes, for example `lesson.mp4`.
        #[arg(long)]
        out: PathBuf,
        /// The edit to export, if not the project's own `edit.json`.
        #[arg(long)]
        edit: Option<PathBuf>,
        /// Instead of an edit, export this take as it was recorded: the screen, the voice, the computer's sound and
        /// the webcam, each whole.
        #[arg(long)]
        take: Option<u32>,
        /// With `--take`: also add a title, a box, an ellipse, an arrow, a blur, a blacked-out box and a zoom, to
        /// see that each is drawn. For testing the exporter, not for real lessons.
        #[arg(long)]
        demo: bool,
        /// Where the course's own files are (pictures and sounds an edit imports).
        #[arg(long)]
        assets: Option<PathBuf>,
        /// Pictures per second in the video.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// Make the video this size, for example `1280x720`, instead of the size of the first picture.
        #[arg(long)]
        size: Option<String>,
        /// Use cuts instead of animated zooms, for viewers who cannot turn motion off.
        #[arg(long)]
        reduce_motion: bool,
        /// Export even if the edit still lacks things OpenStage requires (alt text, descriptions).
        #[arg(long)]
        allow_unfinished: bool,
        /// Print sentences for a person to read instead of one-line JSON events.
        #[arg(long)]
        plain: bool,
    },
}

struct StdoutSink;

impl EventSink for StdoutSink {
    fn emit(&mut self, event: Event) -> bool {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(out, "{}", encode_event(&event)).and_then(|_| out.flush()).is_ok()
    }
}

struct ChannelInput {
    rx: Receiver<Input>,
}

impl InputSource for ChannelInput {
    fn poll(&mut self, _now_ms: u64) -> Vec<Input> {
        let mut out = Vec::new();
        while let Ok(item) = self.rx.try_recv() {
            out.push(item);
        }
        out
    }
}

/// Read commands from standard input in a background thread, one bounded line at a time.
fn read_commands(tx: Sender<Input>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = (&mut reader).take(MAX_LINE_BYTES as u64 + 1).read_until(b'\n', &mut line);
            match read {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Input::Closed);
                    return;
                }
                Ok(_) => {}
            }
            if line.len() > MAX_LINE_BYTES && line.last() != Some(&b'\n') {
                // Too long: throw away the rest of this line without keeping it in memory.
                loop {
                    let mut scratch = Vec::new();
                    match (&mut reader).take(MAX_LINE_BYTES as u64).read_until(b'\n', &mut scratch) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if scratch.last() == Some(&b'\n') => break,
                        Ok(_) => {}
                    }
                }
                let _ = tx
                    .send(Input::Invalid(format!("a command was longer than {MAX_LINE_BYTES} bytes and was ignored")));
                continue;
            }
            match decode_command(&String::from_utf8_lossy(&line)) {
                Ok(command) => {
                    let _ = tx.send(Input::Command(command));
                }
                Err(ProtocolError::Empty) => {}
                Err(other) => {
                    let _ = tx.send(Input::Invalid(other.to_string()));
                }
            }
        }
    });
}

fn say(event: Event) {
    StdoutSink.emit(event);
}

/// A video size such as `1280x720`: both sides even and at least 64.
fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("The size \"{text}\" is not written like 1280x720.");
    let (w, h) = text.trim().split_once(['x', 'X', ',']).ok_or_else(bad)?;
    let (w, h): (u32, u32) = (w.trim().parse().map_err(|_| bad())?, h.trim().parse().map_err(|_| bad())?);
    if w < 64 || h < 64 || w % 2 == 1 || h % 2 == 1 {
        return Err(format!(
            "The size {w} by {h} cannot be used: H.264 video needs an even width and height, each at least 64."
        ));
    }
    Ok((w, h))
}

/// `on` or `off` (or yes/no, true/false) for an option, or a sentence saying what was wrong.
fn on_or_off(option: &str, value: &str) -> Result<bool, String> {
    match value.trim().to_lowercase().as_str() {
        "on" | "yes" | "true" => Ok(true),
        "off" | "no" | "false" => Ok(false),
        other => Err(format!("{option} must be on or off, not \"{other}\".")),
    }
}

fn hello() {
    say(Event::hello(env!("CARGO_PKG_VERSION")));
}

fn fatal(code: &str, message: String) {
    say(Event::Error { code: code.to_string(), message, fatal: true });
}

/// What a person typed for a sound device: nothing to record (`none`), or a choice to resolve later.
fn sound_choice(text: &str) -> Option<AudioChoice> {
    let t = text.trim();
    if t.eq_ignore_ascii_case("none") || t.eq_ignore_ascii_case("off") {
        None
    } else {
        Some(AudioChoice::parse(t))
    }
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Probe => {
            hello();
            let mut sources = vec![SourceInfo {
                id: "synthetic".into(),
                available: true,
                note: "A built-in test source. It makes fake picture and sound so the recorder can be tested anywhere."
                    .into(),
            }];
            // Real displays, found on Windows only.
            if let Ok(displays) = system_platform().list_displays() {
                for d in displays {
                    sources.push(SourceInfo {
                        id: format!("display:{}", d.number),
                        available: true,
                        note: format!(
                            "{}. Record it with: record --source screen --display {}. Picture only: no sound yet.",
                            d.describe(),
                            d.number
                        ),
                    });
                }
            }
            if let Ok(devices) = system_audio_platform().list_devices() {
                for d in devices {
                    let (id, what, option) = match d.kind {
                        DeviceKind::Microphone => (format!("microphone:{}", d.number), "Microphone", "--microphone"),
                        DeviceKind::SoundOutput => {
                            (format!("output:{}", d.number), "Sound output (records the sound it plays)", "--output")
                        }
                    };
                    sources.push(SourceInfo {
                        id,
                        available: true,
                        note: format!(
                            "{what} {}: {}{}. Record it with: record --source screen {option} {}.",
                            d.number,
                            d.name,
                            if d.is_default { ", the default" } else { "" },
                            d.number
                        ),
                    });
                }
            }
            // Real cameras, found on Windows only.
            if let Ok(cameras) = system_camera_platform().list_cameras() {
                for c in cameras {
                    sources.push(SourceInfo {
                        id: format!("camera:{}", c.number),
                        available: true,
                        note: format!(
                            "Camera {}: {}. Record it with: record --source screen --camera {}. It goes into webcam.mp4, beside the screen video.",
                            c.number, c.name, c.number
                        ),
                    });
                }
            }
            say(Event::Probe { platform: std::env::consts::OS.to_string(), sources });
            ExitCode::SUCCESS
        }
        Cmd::ScreenTest { display, seconds, out, with_cursor, plain } => {
            if !plain {
                hello();
            }
            let opts = ScreenTestOptions { display, seconds, out_dir: out, with_cursor };
            let all_fine = run_screen_test(&opts, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::Record {
            project,
            source,
            display,
            region,
            window,
            fps,
            bitrate_kbps,
            microphone,
            output,
            camera,
            mic_gain_db,
            system_gain_db,
            cues,
            cue_volume,
            outline,
            strict,
            countdown_ms,
            segment_ms,
            flush_ms,
            duration_ms,
            ignore_stdin_eof,
            title,
            language,
            plain,
            demo_script,
        } => {
            let mut sink: Box<dyn EventSink> = if plain { Box::new(PlainSink::new()) } else { Box::new(StdoutSink) };
            if !plain {
                hello();
            }
            let (cues_on, outline_on) = match (on_or_off("--cues", &cues), on_or_off("--outline", &outline)) {
                (Ok(cues_on), Ok(outline_on)) => (cues_on, outline_on),
                (Err(message), _) | (_, Err(message)) => {
                    sink.emit(Event::Error { code: "bad-option".into(), message, fatal: true });
                    return ExitCode::from(2);
                }
            };
            let mut sound_plan = SoundPlan::default();
            let mut chosen_region: Option<Region> = None;
            let mut chosen_window: Option<WindowInfo> = None;
            let mut chosen_camera: Option<(
                Box<dyn openstage_capture_windows::CameraPlatform>,
                openstage_capture_windows::CameraInfo,
            )> = None;
            let mut recorded_camera = false;
            let mut report_handle = None;
            // The test source has no camera. Saying nothing would let the author think a webcam was being recorded.
            if source == "synthetic" && !camera.trim().eq_ignore_ascii_case("none") {
                sink.emit(Event::Error {
                    code: "bad-option".into(),
                    message: "--camera only works with --source screen. The built-in test source has no camera, so no webcam would be recorded.".into(),
                    fatal: true,
                });
                return ExitCode::from(2);
            }
            let factory: Box<dyn MediaFactory> = match source.as_str() {
                "synthetic" => Box::new(|| -> Box<dyn Source> { Box::new(SyntheticSource::new()) }),
                "screen" if cfg!(windows) => {
                    // Choose the sound devices now, so a wrong choice is explained before the countdown.
                    let platform = system_audio_platform();
                    sound_plan = match plan_sound(
                        platform.as_ref(),
                        sound_choice(&microphone).as_ref(),
                        sound_choice(&output).as_ref(),
                    ) {
                        Ok(plan) => plan,
                        Err(message) => {
                            sink.emit(Event::Error { code: "source-unavailable".into(), message, fatal: true });
                            return ExitCode::from(2);
                        }
                    };
                    sound_plan = match sound_plan.with_gains(mic_gain_db, system_gain_db) {
                        Ok(plan) => plan,
                        Err(message) => {
                            sink.emit(Event::Error { code: "bad-option".into(), message, fatal: true });
                            return ExitCode::from(2);
                        }
                    };
                    sink.emit(Event::Check { check: "sound-devices".into(), ok: true, message: sound_plan.describe() });
                    // A window is chosen now, so a mistake is explained before the countdown.
                    if let Some(text) = &window {
                        if region.is_some() {
                            sink.emit(Event::Error {
                                code: "bad-option".into(),
                                message: "--window and --region cannot be used together: a window is recorded at its own size.".into(),
                                fatal: true,
                            });
                            return ExitCode::from(2);
                        }
                        let chosen = system_platform()
                            .list_windows()
                            .map_err(|e| e.to_string())
                            .and_then(|list| choose_window(&list, text).cloned());
                        match chosen {
                            Ok(found) => {
                                sink.emit(Event::Check {
                                    check: "window".into(),
                                    ok: true,
                                    message: format!(
                                        "Recording the window {}. The video keeps the window's size from when recording begins. If the window is resized, its picture is made smaller to fit, in the middle on black. If it is minimized, its last picture stays until it is restored. If it is closed, the recording stops at once and keeps everything recorded until then.",
                                        found.describe()
                                    ),
                                });
                                chosen_window = Some(found);
                            }
                            Err(message) => {
                                sink.emit(Event::Error { code: "bad-option".into(), message, fatal: true });
                                return ExitCode::from(2);
                            }
                        }
                    }
                    // A region is checked against the display now, so a mistake is explained before the countdown.
                    if let Some(text) = &region {
                        let fitted = Region::parse(text).and_then(|r| {
                            let displays = system_platform().list_displays().map_err(|e| e.to_string())?;
                            let shown = displays.iter().find(|d| d.number == display).ok_or_else(|| {
                                format!("There is no Display {display}. Choose a number from 1 to {}.", displays.len())
                            })?;
                            r.fit(shown.width, shown.height).map(|(fitted, notes)| (fitted, notes, shown.number))
                        });
                        match fitted {
                            Ok((fitted, notes, number)) => {
                                let mut message = format!("Recording {} of Display {number}.", fitted.describe());
                                for note in notes {
                                    message.push(' ');
                                    message.push_str(&note);
                                }
                                if outline_on {
                                    message.push_str(" A thin frame is shown round it on the screen from the start of the countdown until the recording ends. It is not in the video. Turn it off with --outline off.");
                                }
                                sink.emit(Event::Check { check: "region".into(), ok: true, message });
                                chosen_region = Some(fitted);
                            }
                            Err(message) => {
                                sink.emit(Event::Error { code: "bad-option".into(), message, fatal: true });
                                return ExitCode::from(2);
                            }
                        }
                    }
                    // A camera is chosen now too, so a wrong choice is explained before the countdown.
                    if !camera.trim().eq_ignore_ascii_case("none") {
                        let cameras = system_camera_platform();
                        let found = cameras
                            .list_cameras()
                            .map_err(|e| format!("Could not list the cameras: {e}"))
                            .and_then(|list| choose_camera(&list, &camera).cloned());
                        match found {
                            Ok(info) => {
                                sink.emit(Event::Check {
                                    check: "camera".into(),
                                    ok: true,
                                    message: format!(
                                        "Recording the webcam {} into {WEBCAM_FILE}, beside the screen video. It is recorded as the camera sees it (not mirrored), at about 1280 by 720 and up to 30 pictures a second. Each picture is placed where the camera says it was taken, so it lines up with your voice. If the camera stops, the recording carries on without it and you are told. Leave out --camera to record no webcam.",
                                        info.describe()
                                    ),
                                });
                                chosen_camera = Some((cameras, info));
                            }
                            Err(message) => {
                                sink.emit(Event::Error { code: "bad-option".into(), message, fatal: true });
                                return ExitCode::from(2);
                            }
                        }
                    }
                    let mut media = ScreenMedia::for_this_computer(display, fps, bitrate_kbps)
                        .with_sound(platform, sound_plan.clone());
                    if let Some((cameras, info)) = chosen_camera.take() {
                        media = media.with_camera(cameras, info);
                        recorded_camera = true;
                    }
                    if let Some(r) = chosen_region {
                        media = media.with_region(r).with_outline(outline_on);
                    }
                    if let Some(found) = chosen_window {
                        media = media.with_window(found);
                    }
                    report_handle = Some(media.report_handle());
                    Box::new(media)
                }
                "screen" => {
                    sink.emit(Event::Error {
                        code: "source-unavailable".into(),
                        message:
                            "Recording the screen only works on Windows. Use \"synthetic\" to try the recorder here."
                                .into(),
                        fatal: true,
                    });
                    return ExitCode::from(2);
                }
                other => {
                    sink.emit(Event::Error {
                        code: "source-unavailable".into(),
                        message: format!("The source \"{other}\" is not available. Use \"screen\" or \"synthetic\"."),
                        fatal: true,
                    });
                    return ExitCode::from(2);
                }
            };
            let mut cfg = SessionConfig::new(&project);
            cfg.title = title;
            cfg.language = language;
            cfg.countdown_ms = countdown_ms;
            cfg.segment_ms = segment_ms.max(1);
            cfg.flush_ms = flush_ms.max(1);
            cfg.max_duration_ms = duration_ms;
            cfg.stop_when_input_closes = !ignore_stdin_eof;
            if source == "screen" {
                // Look at the screen and the sound every 5 ms, not every 20: a steady 30 pictures a second needs
                // a look at least every 33 ms, and Windows' sleeping is coarse, so a 20 ms wait can become 30 or 45.
                cfg.tick_ms = 5;
            }

            let mut input: Box<dyn InputSource> = match demo_script {
                Some(script) => match demo::parse(&script) {
                    Ok(items) => Box::new(ScriptedInput::new(items)),
                    Err(message) => {
                        sink.emit(Event::Error { code: "bad-demo-script".into(), message, fatal: true });
                        return ExitCode::FAILURE;
                    }
                },
                None => {
                    let (tx, rx) = mpsc::channel();
                    read_commands(tx);
                    Box::new(ChannelInput { rx })
                }
            };
            let cue_player: Box<dyn CuePlayer> =
                if cues_on && cue_volume > 0 { system_cue_player(cue_volume) } else { Box::new(NoCues) };
            if source == "screen" && cues_on && cue_volume > 0 {
                sink.emit(Event::Check {
                    check: "cues".into(),
                    ok: true,
                    message: format!(
                        "The recorder will make a short sound for the countdown, the start, a pause, a resume, the stop and an error, at volume {cue_volume} of 100. None of them is recorded. Turn them off with --cues off, or change the volume with --cue-volume."
                    ),
                });
            }
            match run_session_with_cues(
                &cfg,
                factory.as_ref(),
                input.as_mut(),
                sink.as_mut(),
                &SystemClock::new(),
                cue_player.as_ref(),
            ) {
                Ok(outcome) => {
                    // A real screen recording is read back through Windows, the way a player would.
                    let mut readback_ok = true;
                    if source == "screen" && outcome.ok && outcome.take > 0 {
                        let dir = take_dir(&project, outcome.take);
                        let inspection = system_video_platform().inspect_video(&dir.join(SCREEN_FILE));
                        let mut findings: Vec<Check> = Vec::new();
                        if let Some(r) = chosen_region {
                            findings.push(compare_size_readback(&inspection, (r.width, r.height)));
                        }
                        if !sound_plan.is_empty() {
                            findings.push(compare_audio_readback(&inspection, outcome.duration_ms));
                        }
                        findings.insert(0, compare_readback(inspection, outcome.duration_ms));
                        if recorded_camera {
                            // What the camera did, as far as the recorder noticed, says how long its video should be.
                            let stopped = report_handle
                                .as_ref()
                                .and_then(|h| h.borrow().clone())
                                .and_then(|r| r.webcam)
                                .and_then(|w| w.stopped.map(|(at, _)| at));
                            findings.push(compare_webcam_readback(
                                system_video_platform().inspect_video(&dir.join(WEBCAM_FILE)),
                                outcome.duration_ms,
                                stopped,
                            ));
                        }
                        // The separate sound files are looked at too: what was really saved.
                        let files = check_sound_files(&dir, &sound_plan, outcome.duration_ms);
                        findings.extend(files.checks);
                        for check in findings {
                            readback_ok &= check.ok;
                            sink.emit(Event::Check { check: check.check, ok: check.ok, message: check.message });
                        }
                        let mut warnings = files.warnings;
                        // If Windows now sends sound to a different output than the one chosen, say so: the
                        // recorder kept to that output, so the rest of the computer's sound would be missing.
                        // An output left as the default is followed, so this only concerns one chosen by number or name.
                        if let Some(chosen) = sound_plan.system.as_ref().filter(|_| !sound_plan.follow_default_output) {
                            if let Ok(list) = system_audio_platform().list_devices() {
                                if let Some(message) = output_may_have_changed(&list, chosen) {
                                    warnings.push(("output-changed".to_string(), message));
                                }
                            }
                        }
                        // With both sounds, see whether the microphone also heard the speakers. An echo is
                        // something for the author to know, not a fault of the recorder, so it does not
                        // change the exit code.
                        if sound_plan.microphone.is_some() && sound_plan.system.is_some() {
                            // How loud the two were next to each other, and what to change if the voice is hard to hear.
                            match check_balance_in_files(
                                &dir.join(MIC_FILE),
                                &dir.join(SYSTEM_FILE),
                                Mix {
                                    microphone_db: sound_plan.microphone_gain_db,
                                    system_db: sound_plan.system_gain_db,
                                },
                            ) {
                                Ok(finding) => {
                                    sink.emit(Event::Check {
                                        check: finding.check.check,
                                        ok: finding.check.ok,
                                        message: finding.check.message,
                                    });
                                    if let Some(message) = finding.warning {
                                        warnings.push(("balance-off".to_string(), message));
                                    }
                                }
                                Err(message) => warnings.push(("balance-unchecked".to_string(), message)),
                            }
                            match check_echo_in_files(&dir.join(MIC_FILE), &dir.join(SYSTEM_FILE)) {
                                Ok(finding) => {
                                    sink.emit(Event::Check {
                                        check: finding.check.check,
                                        ok: finding.check.ok,
                                        message: finding.check.message,
                                    });
                                    if let Some(message) = finding.warning {
                                        warnings.push(("echo-likely".to_string(), message));
                                    }
                                }
                                Err(message) => warnings.push(("echo-unchecked".to_string(), message)),
                            }
                        }
                        for (code, message) in warnings {
                            sink.emit(Event::Warning { code, message });
                        }
                        // What the recording did: did the program keep up, did it grow, did sound stay with picture.
                        if let Some(report) = report_handle.as_ref().and_then(|h| h.borrow().clone()) {
                            for check in summarize(&report) {
                                if strict {
                                    readback_ok &= check.ok;
                                }
                                sink.emit(Event::Check { check: check.check, ok: check.ok, message: check.message });
                            }
                        }
                    }
                    sink.emit(Event::Result {
                        ok: outcome.ok,
                        project: path_text(&project),
                        take: outcome.take,
                        state: outcome.state.clone(),
                        duration_ms: outcome.duration_ms,
                        segments: outcome.segments,
                    });
                    if outcome.ok && readback_ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    sink.emit(Event::Error { code: "project".into(), message: e.to_string(), fatal: true });
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Recover { project } => {
            hello();
            match recover_project(&project) {
                Ok(report) => {
                    for r in &report.recovered {
                        say(Event::Recovered {
                            take: r.take,
                            duration_ms: r.duration_ms,
                            max_loss_ms: r.max_loss_ms,
                            segments: r.segments,
                        });
                    }
                    for problem in &report.problems {
                        say(Event::Warning { code: "recover-problem".into(), message: problem.clone() });
                    }
                    let last = report.recovered.last();
                    say(Event::Result {
                        ok: true,
                        project: path_text(&project),
                        take: last.map(|r| r.take).unwrap_or(0),
                        state: if last.is_some() { "recovered" } else { "none" }.into(),
                        duration_ms: last.map(|r| r.duration_ms).unwrap_or(0),
                        segments: last.map(|r| r.segments).unwrap_or(0),
                    });
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    fatal("project", e.to_string());
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::FindOutput { seconds, plain } => {
            if !plain {
                hello();
            }
            let all_fine = run_find_output(&FindOutputOptions { seconds }, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::Windows { plain } => {
            if !plain {
                hello();
            }
            let mut all_fine = true;
            match system_platform().list_windows() {
                Ok(list) if !list.is_empty() => {
                    emit_check(
                        plain,
                        Check {
                            check: "windows".into(),
                            ok: true,
                            message: format!(
                                "{} window{} can be recorded. Choose one with --window and its number, or part of its title or program name.",
                                list.len(),
                                if list.len() == 1 { "" } else { "s" }
                            ),
                        },
                    );
                    for window in &list {
                        emit_check(plain, Check { check: "window".into(), ok: true, message: window.describe() });
                    }
                }
                Ok(_) => {
                    all_fine = false;
                    emit_check(
                        plain,
                        Check {
                            check: "windows".into(),
                            ok: false,
                            message: "There are no open windows that can be recorded. Windows that are minimized or hidden, and windows without a title, are not listed.".into(),
                        },
                    );
                }
                Err(e) => {
                    all_fine = false;
                    emit_check(
                        plain,
                        Check {
                            check: "windows".into(),
                            ok: false,
                            message: format!("Could not list the open windows: {e}"),
                        },
                    );
                }
            }
            finish_checks(plain, all_fine)
        }
        Cmd::Cameras { plain } => {
            if !plain {
                hello();
            }
            let check = match system_camera_platform().list_cameras() {
                Ok(list) if !list.is_empty() => {
                    Check { check: "cameras".into(), ok: true, message: describe_cameras(&list) }
                }
                Ok(_) => Check { check: "cameras".into(), ok: false, message: describe_cameras(&[]) },
                Err(e) => {
                    Check { check: "cameras".into(), ok: false, message: format!("Could not list the cameras: {e}") }
                }
            };
            let ok = check.ok;
            emit_check(plain, check);
            finish_checks(plain, ok)
        }
        Cmd::AudioDevices { plain } => {
            if !plain {
                hello();
            }
            let check = match system_audio_platform().list_devices() {
                Ok(list) if !list.is_empty() => {
                    Check { check: "audio-devices".into(), ok: true, message: describe_devices(&list) }
                }
                Ok(_) => Check {
                    check: "audio-devices".into(),
                    ok: false,
                    message: "No microphone or sound output was found on this computer.".into(),
                },
                Err(e) => Check {
                    check: "audio-devices".into(),
                    ok: false,
                    message: format!("Could not list the sound devices: {e}"),
                },
            };
            let ok = check.ok;
            emit_check(plain, check);
            finish_checks(plain, ok)
        }
        Cmd::AudioTest { seconds, out, microphone, output, plain } => {
            if !plain {
                hello();
            }
            let opts = AudioTestOptions {
                seconds,
                out_dir: out,
                microphone: AudioChoice::parse(&microphone),
                output: AudioChoice::parse(&output),
            };
            let all_fine = run_audio_test(&opts, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::TimingTest { display, seconds, microphone, output, plain } => {
            if !plain {
                hello();
            }
            let opts = TimingTestOptions {
                display,
                seconds,
                microphone: AudioChoice::parse(&microphone),
                output: AudioChoice::parse(&output),
            };
            let all_fine = run_timing_test(&opts, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::SetupCheck { dir, plain } => {
            if !plain {
                hello();
            }
            // "folder\." and "folder" are the same place; say it the short way.
            let dir: PathBuf = dir.components().collect();
            let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
            let all_fine = run_setup_check(&SetupOptions { dir }, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::PickRegion { display, region, out, plain } => {
            if !plain {
                hello();
            }
            let opts = PickRegionOptions { display, start: region, out };
            let all_fine = run_pick_region(&opts, &mut |c| emit_check(plain, c));
            finish_checks_with(plain, all_fine, "A region was chosen.", "No region was chosen.")
        }
        Cmd::CueTest { volume, out, plain } => {
            if !plain {
                hello();
            }
            let opts = CueTestOptions { volume, out_dir: Some(out) };
            let all_fine = run_cue_test(&opts, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::VideoTest { display, seconds, fps, bitrate_kbps, fragmented, start_marker, out, plain } => {
            if !plain {
                hello();
            }
            let opts = VideoTestOptions { display, seconds, fps, bitrate_kbps, out_dir: out, fragmented, start_marker };
            let all_fine = run_video_test(&opts, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::VideoInfo { file, plain } => {
            if !plain {
                hello();
            }
            let check = match system_video_platform().inspect_video(&file) {
                Ok(info) if info.frames > 0 => Check {
                    check: "video-info".into(),
                    ok: true,
                    message: format!(
                        "{} holds {} pictures and is {:.1} seconds long.",
                        file.display(),
                        info.frames,
                        info.duration_ms as f64 / 1000.0
                    ),
                },
                Ok(_) => Check {
                    check: "video-info".into(),
                    ok: false,
                    message: format!("{} opened, but holds no pictures.", file.display()),
                },
                Err(e) => Check {
                    check: "video-info".into(),
                    ok: false,
                    message: format!("{} could not be read as a video: {e}", file.display()),
                },
            };
            let ok = check.ok;
            emit_check(plain, check);
            finish_checks(plain, ok)
        }
        Cmd::VideoCrashTest { display, seconds_before_kill, out, plain } => {
            if !plain {
                hello();
            }
            let all_fine = run_video_crash_test(display, seconds_before_kill, &out, &mut |c| emit_check(plain, c));
            finish_checks(plain, all_fine)
        }
        Cmd::RecorderCrashTest { display, microphone, output, seconds_before_kill, out, plain } => {
            if !plain {
                hello();
            }
            let all_fine =
                run_recorder_crash_test(display, &microphone, &output, seconds_before_kill, &out, &mut |c| {
                    emit_check(plain, c)
                });
            finish_checks(plain, all_fine)
        }
        Cmd::Render { project, out, edit, take, demo, assets, fps, size, reduce_motion, allow_unfinished, plain } => {
            if !plain {
                hello();
            }
            let size = match size.as_deref().map(parse_size).transpose() {
                Ok(size) => size,
                Err(message) => {
                    emit_check(plain, Check { check: "export".into(), ok: false, message });
                    return finish_checks_with(plain, false, "", "The export did not start.");
                }
            };
            #[cfg(windows)]
            {
                let opts = RenderOptions {
                    project: project.clone(),
                    out,
                    edit_file: edit,
                    take,
                    demo,
                    assets: assets.clone(),
                    fps: fps.max(1),
                    size,
                    reduce_motion,
                    allow_unfinished,
                };
                install_cancel_handler();
                let video = system_video_platform();
                let mut pictures = match WindowsPictures::new(&project, assets.as_deref()) {
                    Ok(pictures) => pictures,
                    Err(message) => {
                        emit_check(plain, Check { check: "export".into(), ok: false, message });
                        return finish_checks_with(plain, false, "", "The export did not start.");
                    }
                };
                let mut sounds = FileSounds::new(&project, assets.as_deref());
                let ok = run_render_with(
                    &opts,
                    ExportTools {
                        video: video.as_ref(),
                        pictures: &mut pictures,
                        text: &WindowsText,
                        sound: &mut sounds,
                    },
                    &cancel_requested,
                    &mut |c| emit_check(plain, c),
                    &mut |p| {
                        let percent = (p.frames_done * 100 / p.frame_count.max(1)) as u32;
                        emit_progress(
                            plain,
                            "export",
                            percent,
                            format!("Exporting: {percent} percent ({} of {} pictures).", p.frames_done, p.frame_count),
                        )
                    },
                );
                finish_checks_with(
                    plain,
                    ok,
                    "The video was exported.",
                    "The export did not finish, or the finished video did not read back as it should.",
                )
            }
            #[cfg(not(windows))]
            {
                let _ = (project, out, edit, take, demo, assets, fps, size, reduce_motion, allow_unfinished);
                emit_check(
                    plain,
                    Check {
                        check: "export".into(),
                        ok: false,
                        message: "Exporting a video only works on Windows, because Windows' own video tools read the recordings and write the MP4.".into(),
                    },
                );
                finish_checks_with(plain, false, "", "The export did not start.")
            }
        }
    }
}
