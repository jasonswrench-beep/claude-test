// SPDX-License-Identifier: Apache-2.0
//! Tests that run the real `openstage-capture` program, the way OpenStage will.

use openstage_capture_protocol::{decode_event, Event, State};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_openstage-capture");
const WAIT: Duration = Duration::from_secs(15);

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<Result<Event, String>>,
    seen: Vec<Event>,
}

impl Running {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(BIN)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the program starts");
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(decode_event(&line).map_err(|e| format!("{e}: {line}")));
            }
        });
        let stdin = child.stdin.take();
        Self { child, stdin, events, seen: Vec::new() }
    }

    fn record(project: &Path, extra: &[&str]) -> Self {
        let mut args = vec!["record", "--project", project.to_str().unwrap(), "--countdown-ms", "0", "--cues", "off"];
        args.extend_from_slice(extra);
        Self::start(&args)
    }

    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    fn wait_for(&mut self, what: &str, hit: impl Fn(&Event) -> bool) {
        loop {
            match self.events.recv_timeout(WAIT) {
                Ok(Ok(event)) => {
                    let found = hit(&event);
                    self.seen.push(event);
                    if found {
                        return;
                    }
                }
                Ok(Err(bad)) => panic!("the program printed a line that is not a protocol event: {bad}"),
                Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for {what}; saw {:#?}", self.seen),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("the program ended while waiting for {what}; saw {:#?}", self.seen)
                }
            }
        }
    }

    fn wait_for_state(&mut self, state: State) {
        self.wait_for(&format!("state {state:?}"), |e| matches!(e, Event::State { state: s, .. } if *s == state));
    }

    fn drain(&mut self) {
        while let Ok(item) = self.events.recv_timeout(Duration::from_secs(5)) {
            self.seen.push(item.expect("every line is a protocol event"));
        }
    }

    fn finish(mut self) -> (ExitStatus, Vec<Event>) {
        drop(self.stdin.take());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > WAIT {
                let _ = self.child.kill();
                panic!("the program did not finish; saw {:#?}", self.seen);
            }
            thread::sleep(Duration::from_millis(20));
        };
        self.drain();
        (status, self.seen)
    }

    /// End the program the rough way, like OpenStage's Cancel does.
    fn kill(mut self) -> Vec<Event> {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        drop(self.stdin.take());
        self.drain();
        self.seen
    }
}

fn run_once(args: &[&str]) -> (ExitStatus, Vec<Event>) {
    Running::start(args).finish()
}

fn result_of(events: &[Event]) -> (bool, u32, String, u64, u32) {
    match events.last() {
        Some(Event::Result { ok, take, state, duration_ms, segments, .. }) => {
            (*ok, *take, state.clone(), *duration_ms, *segments)
        }
        other => panic!("the last line should be a result, got {other:?}"),
    }
}

fn take_state(project: &Path, take: u32) -> String {
    let text = fs::read_to_string(project.join(format!("takes/take-{take:03}/take.json"))).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["state"].as_str().unwrap().to_string()
}

#[test]
fn it_says_hello_first_and_a_short_recording_ends_with_a_result() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &["--duration-ms", "1200", "--segment-ms", "500", "--ignore-stdin-eof"]);
    run.wait_for("the end", |e| matches!(e, Event::Result { .. }));
    let (status, events) = run.finish();
    assert!(status.success());
    assert!(
        matches!(&events[0], Event::Hello { protocol, program, .. } if protocol == "0.1" && program == "openstage-capture")
    );
    let (ok, take, state, duration, segments) = result_of(&events);
    assert!(ok);
    assert_eq!((take, state.as_str(), duration), (1, "complete", 1200));
    assert_eq!(segments, 3, "1200 ms in pieces of 500 ms");
    assert_eq!(take_state(&project, 1), "complete");
    assert!(project.join("capture.json").is_file());
}

#[test]
fn pause_resume_and_stop_work_over_standard_input() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &[]);
    run.wait_for_state(State::Recording);
    thread::sleep(Duration::from_millis(400));
    run.send(r#"{"cmd":"pause"}"#);
    run.wait_for_state(State::Paused);
    thread::sleep(Duration::from_millis(700));
    run.send(r#"{"cmd":"resume"}"#);
    run.wait_for("recording again", |e| matches!(e, Event::State { state: State::Recording, .. }));
    thread::sleep(Duration::from_millis(400));
    run.send(r#"{"cmd":"stop"}"#);
    let (status, events) = run.finish();
    assert!(status.success());
    let (ok, _, state, duration, _) = result_of(&events);
    assert!(ok && state == "complete");
    // About 800 ms were recorded and 700 ms were paused, so the paused time is not in the recording.
    assert!((600..=1400).contains(&duration), "duration was {duration}");
}

#[test]
fn start_again_keeps_the_first_take_and_begins_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &["--segment-ms", "300"]);
    run.wait_for_state(State::Recording);
    thread::sleep(Duration::from_millis(900));
    run.send(r#"{"cmd":"restart"}"#);
    run.wait_for("the first take being set aside", |e| matches!(e, Event::TakeSetAside { take: 1, next_take: 2 }));
    thread::sleep(Duration::from_millis(500));
    run.send(r#"{"cmd":"stop"}"#);
    let (status, events) = run.finish();
    assert!(status.success());
    let (ok, take, _, _, _) = result_of(&events);
    assert!(ok);
    assert_eq!(take, 2);
    assert_eq!(take_state(&project, 1), "setAside");
    assert_eq!(take_state(&project, 2), "complete");
    let kept = fs::read_dir(project.join("takes/take-001/segments")).unwrap().count();
    assert!(kept >= 2, "the first take's recording is still on disk ({kept} files)");
}

#[test]
fn a_command_that_makes_no_sense_gets_a_warning_and_recording_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &[]);
    run.wait_for_state(State::Recording);
    run.send("this is not json");
    run.send(r#"{"cmd":"explode"}"#);
    run.wait_for("a warning", |e| matches!(e, Event::Warning { code, .. } if code == "bad-command"));
    thread::sleep(Duration::from_millis(200));
    run.send(r#"{"cmd":"stop"}"#);
    let (status, events) = run.finish();
    assert!(status.success());
    assert!(result_of(&events).0);
    let warnings = events.iter().filter(|e| matches!(e, Event::Warning { .. })).count();
    assert_eq!(warnings, 2);
}

#[test]
fn a_very_long_command_line_is_refused_without_trouble() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &[]);
    run.wait_for_state(State::Recording);
    run.send(&"x".repeat(200_000));
    run.wait_for("a warning", |e| matches!(e, Event::Warning { code, .. } if code == "bad-command"));
    run.send(r#"{"cmd":"stop"}"#);
    let (status, events) = run.finish();
    assert!(status.success() && result_of(&events).0);
}

#[test]
fn closing_standard_input_stops_the_recording_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &[]);
    run.wait_for_state(State::Recording);
    thread::sleep(Duration::from_millis(500));
    // OpenStage going away looks like standard input closing. `finish` closes it.
    let (status, events) = run.finish();
    assert!(status.success());
    let (ok, take, state, duration, _) = result_of(&events);
    assert!(ok && take == 1 && state == "complete" && duration >= 300);
    assert_eq!(take_state(&project, 1), "complete");
}

/// Story A8: kill the program mid-recording, then recover. At most the flush interval is lost.
#[test]
fn killing_the_program_mid_recording_loses_almost_nothing_and_recover_repairs_it() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("lesson.capture");
    let mut run = Running::record(&project, &["--segment-ms", "1000", "--flush-ms", "250"]);
    run.wait_for_state(State::Recording);
    let recording_began = Instant::now();
    thread::sleep(Duration::from_millis(3200));
    let recorded_before_kill_ms = recording_began.elapsed().as_millis() as u64;
    let events = run.kill();
    assert!(!events.iter().any(|e| matches!(e, Event::Result { .. })), "a killed program prints no result");
    assert_eq!(take_state(&project, 1), "recording", "the take was left mid-recording");
    let parts = fs::read_dir(project.join("takes/take-001/segments"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().unwrap() == "part")
        .count();
    assert_eq!(parts, 1, "the piece being written when the program died");

    let (status, recovered) = run_once(&["recover", "--project", project.to_str().unwrap()]);
    assert!(status.success());
    let found: Vec<_> = recovered
        .iter()
        .filter_map(|e| {
            if let Event::Recovered { take, duration_ms, max_loss_ms, segments } = e {
                Some((*take, *duration_ms, *max_loss_ms, *segments))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(found.len(), 1);
    let (take, duration_ms, max_loss_ms, segments) = found[0];
    assert_eq!((take, max_loss_ms), (1, 250));
    assert!(segments >= 3, "three seconds in pieces of one second, got {segments}");
    let lost = recorded_before_kill_ms.saturating_sub(duration_ms);
    assert!(lost <= 10_000, "the requirement allows 10 seconds; lost {lost} ms");
    assert!(lost <= 1_200, "with a 250 ms flush interval the loss should be small; lost {lost} ms");
    assert!(duration_ms >= 1_500, "recovered {duration_ms} ms of about {recorded_before_kill_ms}");
    assert_eq!(result_of(&recovered).2, "recovered");

    assert_eq!(take_state(&project, 1), "recovered");
    let leftover = fs::read_dir(project.join("takes/take-001/segments"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().unwrap() != "seg")
        .count();
    assert_eq!(leftover, 0, "every piece is complete after recovery");

    // Running it again changes nothing.
    let (_, again) = run_once(&["recover", "--project", project.to_str().unwrap()]);
    assert!(!again.iter().any(|e| matches!(e, Event::Recovered { .. })));
    assert_eq!(result_of(&again).2, "none");
}

#[test]
fn probe_lists_the_test_source_and_says_windows_capture_is_not_built() {
    let (status, events) = run_once(&["probe"]);
    assert!(status.success());
    let sources = events
        .iter()
        .find_map(|e| if let Event::Probe { sources, .. } = e { Some(sources.clone()) } else { None })
        .unwrap();
    assert!(sources.iter().any(|s| s.id == "synthetic" && s.available));
    assert!(sources.iter().any(|s| s.id == "windows" && !s.available));
}

#[cfg(not(windows))]
#[test]
fn screen_test_says_it_needs_windows_and_exits_with_2() {
    let (status, events) = run_once(&["screen-test", "--seconds", "1"]);
    assert_eq!(status.code(), Some(2));
    assert!(matches!(&events[0], Event::Hello { .. }));
    let checks: Vec<_> = events
        .iter()
        .filter_map(|e| if let Event::Check { ok, message, .. } = e { Some((*ok, message.clone())) } else { None })
        .collect();
    assert_eq!(checks.len(), 1, "stops at the first problem: {checks:?}");
    assert!(!checks[0].0 && checks[0].1.contains("only works on Windows"));
}

#[cfg(not(windows))]
#[test]
fn timing_test_says_it_needs_windows_and_exits_with_2() {
    let (status, events) = run_once(&["timing-test", "--seconds", "1"]);
    assert_eq!(status.code(), Some(2));
    assert!(matches!(&events[0], Event::Hello { .. }));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Check { ok: false, message, .. } if message.contains("only works on Windows"))));
}

#[cfg(not(windows))]
#[test]
fn recorder_crash_test_says_it_needs_windows_and_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let (status, events) = run_once(&["recorder-crash-test", "--out", dir.path().join("o").to_str().unwrap()]);
    assert_eq!(status.code(), Some(2));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Check { ok: false, message, .. } if message.contains("only work on Windows"))));
}

#[cfg(not(windows))]
#[test]
fn screen_test_plain_prints_sentences_not_json() {
    let out = Command::new(BIN).args(["screen-test", "--plain"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"PROBLEM: Screen capture only works on Windows."));
    assert_eq!(lines.last(), Some(&"At least one check found a problem."));
    assert!(!text.contains('{'), "no JSON in plain mode: {text}");
}

#[cfg(not(windows))]
#[test]
fn video_test_says_it_needs_windows_and_exits_with_2() {
    let out = Command::new(BIN).args(["video-test", "--plain", "--seconds", "1"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("PROBLEM: Windows' video tools would not start"), "{text}");
    assert_eq!(lines.last(), Some(&"At least one check found a problem."));
}

#[cfg(not(windows))]
#[test]
fn video_crash_test_says_it_needs_windows_and_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .args(["video-crash-test", "--plain", "--out", dir.path().join("o").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("PROBLEM: Screen capture and video only work on Windows."), "{text}");
}

#[cfg(not(windows))]
#[test]
fn recording_the_screen_says_it_needs_windows_and_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let (status, events) =
        run_once(&["record", "--project", dir.path().join("p.capture").to_str().unwrap(), "--source", "screen"]);
    assert_eq!(status.code(), Some(2));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, message, fatal: true }
        if code == "source-unavailable" && message.contains("only works on Windows"))));
}

#[test]
fn the_sound_choices_are_accepted_and_do_not_disturb_the_test_source() {
    // The built-in test source makes its own fake sound; the real-sound options are for `--source screen`.
    let dir = tempfile::tempdir().unwrap();
    let (status, events) = run_once(&[
        "record",
        "--project",
        dir.path().join("p.capture").to_str().unwrap(),
        "--countdown-ms",
        "0",
        "--duration-ms",
        "500",
        "--microphone",
        "none",
        "--output",
        "2",
        "--mic-gain-db",
        "3",
        "--system-gain-db",
        "-10",
    ]);
    assert_eq!(status.code(), Some(0));
    assert!(events.iter().any(|e| matches!(e, Event::Result { ok: true, .. })));
}

#[test]
fn plain_mode_speaks_in_sentences_follows_a_demo_script_and_leaves_no_json() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("p.capture");
    let out = Command::new(BIN)
        .args([
            "record",
            "--project",
            project.to_str().unwrap(),
            "--countdown-ms",
            "0",
            "--plain",
            "--demo-script",
            "marker:look here@500;pause@800;resume@1100;stop@1600",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains('{'), "no JSON in plain mode: {text}");
    for sentence in [
        "Recording take 1.",
        "Marker added at",
        "look here",
        "Paused at",
        "Stopped. Take 1 is finished.",
        "Finished. Take 1 is complete",
    ] {
        assert!(text.contains(sentence), "missing \"{sentence}\" in:\n{text}");
    }
    // The pause is recorded as time not spent: about 1.6 s of wall time minus 0.3 s paused.
    let take = openstage_capture_project::read_take(&project, 1).unwrap();
    let ms = take.duration_ms.unwrap();
    assert!((1100..=1500).contains(&ms), "the recording is about 1.3 seconds long, got {ms}");
    assert_eq!(take.sources, vec!["synthetic".to_string()]);
}

#[test]
fn a_demo_script_that_makes_no_sense_is_explained_and_nothing_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("p.capture");
    let (status, events) = run_once(&[
        "record",
        "--project",
        project.to_str().unwrap(),
        "--countdown-ms",
        "0",
        "--demo-script",
        "explode@100",
    ]);
    assert_eq!(status.code(), Some(1));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, message, fatal: true }
        if code == "bad-demo-script" && message.contains("is not a command"))));
    assert!(!events.iter().any(|e| matches!(e, Event::State { .. })), "no recording began");
}

#[cfg(not(windows))]
#[test]
fn audio_test_says_it_needs_windows_and_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .args(["audio-test", "--plain", "--seconds", "1", "--out", dir.path().join("o").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("PROBLEM: Could not list the sound devices: Screen capture only works on Windows."),
        "{text}"
    );
    assert!(text.contains("PROBLEM: Could not listen to the microphone:"), "{text}");
    assert!(text.trim_end().ends_with("At least one check found a problem."));
}

#[cfg(not(windows))]
#[test]
fn video_info_says_it_needs_windows_and_exits_with_2() {
    let out = Command::new(BIN).args(["video-info", "--plain", "--file", "nothing.mp4"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("PROBLEM: nothing.mp4 could not be read as a video"), "{text}");
}

#[test]
fn render_says_it_is_not_built_yet_and_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let (status, events) = run_once(&["render", "--project", dir.path().to_str().unwrap(), "--out", "x.mp4"]);
    assert_eq!(status.code(), Some(2));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, fatal: true, .. } if code == "not-implemented")));
}

#[test]
fn an_unknown_source_is_refused_with_exit_code_2() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("p.capture");
    let (status, events) = run_once(&["record", "--project", project.to_str().unwrap(), "--source", "windows"]);
    assert_eq!(status.code(), Some(2));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, .. } if code == "source-unavailable")));
}

#[test]
fn recover_on_a_folder_that_is_not_a_project_fails_politely() {
    let dir = tempfile::tempdir().unwrap();
    let (status, events) = run_once(&["recover", "--project", dir.path().to_str().unwrap()]);
    assert_eq!(status.code(), Some(1));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, fatal: true, .. } if code == "project")));
}

#[cfg(not(windows))]
#[test]
fn audio_devices_says_it_needs_windows_and_exits_with_2() {
    let out = Command::new(BIN).args(["audio-devices", "--plain"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("PROBLEM: Could not list the sound devices: Screen capture only works on Windows."),
        "{text}"
    );
}

#[cfg(not(windows))]
#[test]
fn find_output_says_it_needs_windows_and_exits_with_2() {
    let out = Command::new(BIN).args(["find-output", "--plain", "--seconds", "1"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("PROBLEM: Could not list the sound devices: Screen capture only works on Windows."),
        "{text}"
    );
}

#[test]
fn the_cue_options_are_checked_in_words_and_a_wrong_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (status, events) =
        run_once(&["record", "--project", dir.path().join("p.capture").to_str().unwrap(), "--cues", "maybe"]);
    assert_eq!(status.code(), Some(2));
    assert!(events.iter().any(|e| matches!(e, Event::Error { code, message, fatal: true }
        if code == "bad-option" && message.contains("--cues must be on or off"))));
    let out = Command::new(BIN)
        .args(["record", "--project", dir.path().join("q.capture").to_str().unwrap(), "--cue-volume", "150"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a volume above 100 is refused");
}

#[test]
fn recording_with_the_cues_on_or_off_is_accepted_and_records_the_same() {
    for cues in ["on", "off"] {
        let dir = tempfile::tempdir().unwrap();
        let (status, events) = run_once(&[
            "record",
            "--project",
            dir.path().join("p.capture").to_str().unwrap(),
            "--countdown-ms",
            "0",
            "--duration-ms",
            "600",
            "--cues",
            cues,
            "--cue-volume",
            "40",
            "--ignore-stdin-eof",
        ]);
        assert!(status.success(), "cues {cues}");
        let (ok, _, state, duration_ms, _) = result_of(&events);
        assert!(ok && state == "complete" && (500..=700).contains(&duration_ms), "cues {cues}: {duration_ms}");
    }
}

#[cfg(not(windows))]
#[test]
fn the_cue_test_saves_each_sound_as_a_file_and_says_it_cannot_play_them_here() {
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("sounds");
    let out = Command::new(BIN).args(["cue-test", "--plain", "--out", out_dir.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(2), "playing only works on Windows");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Next: the start sound. It is three rising notes"), "{text}");
    assert!(text.contains("The error sound could not be played on this computer."), "{text}");
    for name in ["countdown", "start", "pause", "resume", "stop", "error"] {
        assert!(out_dir.join(format!("cue-{name}.wav")).is_file(), "{name}");
    }
}

#[cfg(not(windows))]
#[test]
fn the_setup_check_says_it_needs_windows_and_exits_with_2() {
    let out = Command::new(BIN).args(["setup-check", "--plain"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Windows would not say which version this is"), "{text}");
    assert!(text.contains("The free space on the drive that holds"), "{text}");
    assert_eq!(text.lines().last(), Some("At least one check found a problem."));
}
