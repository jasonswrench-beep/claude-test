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

use checks::{emit_check, finish_checks, run_recorder_crash_test, run_video_crash_test};
use clap::{Parser, Subcommand};
use openstage_capture::{
    run_session, EventSink, Input, InputSource, MediaFactory, ScriptedInput, SessionConfig, Source, SyntheticSource,
    SystemClock,
};
use openstage_capture_project::{recover_project, take_dir};
use openstage_capture_protocol::{decode_command, encode_event, Event, ProtocolError, SourceInfo, MAX_LINE_BYTES};
use openstage_capture_windows::{
    check_balance_in_files, check_echo_in_files, check_sound_files, compare_audio_readback, compare_readback,
    describe_devices, plan_sound, run_audio_test, run_find_output, run_screen_test, run_timing_test, run_video_test,
    summarize, system_audio_platform, system_platform, system_video_platform, AudioChoice, AudioTestOptions, Check,
    DeviceKind, FindOutputOptions, ScreenMedia, ScreenTestOptions, SoundPlan, TimingTestOptions, VideoTestOptions,
    MIC_FILE, SCREEN_FILE, SYSTEM_FILE,
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
        /// `default`, or `none` to record none of it.
        #[arg(long, default_value = "default")]
        output: String,
        /// With `--source screen`: make the microphone louder (positive) or quieter (negative) by this
        /// many decibels in the mix inside the video. 0 leaves it as heard; 6 is about double the volume.
        /// The separate `mic.wav` is never changed.
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        mic_gain_db: i32,
        /// With `--source screen`: the same for the computer's sound. Use a negative number to make it
        /// quieter, for example -10. The separate `system.wav` is never changed.
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        system_gain_db: i32,
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
    /// Turn a project into a video. Not built yet.
    Render {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
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
            sources.push(SourceInfo {
                id: "windows".into(),
                available: false,
                note: "Recording a single window or a region, and the webcam, is not built yet.".into(),
            });
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
            fps,
            bitrate_kbps,
            microphone,
            output,
            mic_gain_db,
            system_gain_db,
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
            let mut sound_plan = SoundPlan::default();
            let mut report_handle = None;
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
                    let media = ScreenMedia::for_this_computer(display, fps, bitrate_kbps)
                        .with_sound(platform, sound_plan.clone());
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
            match run_session(&cfg, factory.as_ref(), input.as_mut(), sink.as_mut(), &SystemClock::new()) {
                Ok(outcome) => {
                    // A real screen recording is read back through Windows, the way a player would.
                    let mut readback_ok = true;
                    if source == "screen" && outcome.ok && outcome.take > 0 {
                        let dir = take_dir(&project, outcome.take);
                        let inspection = system_video_platform().inspect_video(&dir.join(SCREEN_FILE));
                        let mut findings: Vec<Check> = Vec::new();
                        if !sound_plan.is_empty() {
                            findings.push(compare_audio_readback(&inspection, outcome.duration_ms));
                        }
                        findings.insert(0, compare_readback(inspection, outcome.duration_ms));
                        // The separate sound files are looked at too: what was really saved.
                        let files = check_sound_files(&dir, &sound_plan, outcome.duration_ms);
                        findings.extend(files.checks);
                        for check in findings {
                            readback_ok &= check.ok;
                            sink.emit(Event::Check { check: check.check, ok: check.ok, message: check.message });
                        }
                        let mut warnings = files.warnings;
                        // With both sounds, see whether the microphone also heard the speakers. An echo is
                        // something for the author to know, not a fault of the recorder, so it does not
                        // change the exit code.
                        if sound_plan.microphone.is_some() && sound_plan.system.is_some() {
                            // How loud the two were next to each other, and what to change if the voice is hard to hear.
                            match check_balance_in_files(&dir.join(MIC_FILE), &dir.join(SYSTEM_FILE)) {
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
        Cmd::Render { .. } => {
            hello();
            fatal(
                "not-implemented",
                "Turning a recording into a video is not built yet. It arrives in Phase 3.".into(),
            );
            ExitCode::from(2)
        }
    }
}
