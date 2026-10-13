// SPDX-License-Identifier: Apache-2.0
//! The commands that test this computer (`screen-test`, `video-test`, `video-crash-test`) share
//! how they print their findings and how they end.

use openstage_capture_protocol::{
    decode_event, encode_command, encode_event, Command as RecorderCommand, Event, State,
};
use openstage_capture_windows::{
    plan_sound, run_crash_report, run_recorder_crash_report, system_audio_platform, system_video_platform, Check,
};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Print one finding: as a sentence for a person (`plain`), or as a protocol event.
pub fn emit_check(plain: bool, c: Check) {
    if plain {
        println!("{}: {}", if c.ok { "OK" } else { "PROBLEM" }, c.message);
    } else {
        println!("{}", encode_event(&Event::Check { check: c.check, ok: c.ok, message: c.message }));
    }
    let _ = io::stdout().flush();
}

/// Print the closing line (in plain mode) and choose the exit code: 0 all fine, 1 a problem was
/// found, 2 this computer is not running Windows.
pub fn finish_checks(plain: bool, all_fine: bool) -> ExitCode {
    if plain {
        println!("{}", if all_fine { "All checks passed." } else { "At least one check found a problem." });
        let _ = io::stdout().flush();
    }
    if all_fine {
        ExitCode::SUCCESS
    } else if cfg!(windows) {
        ExitCode::FAILURE
    } else {
        ExitCode::from(2)
    }
}

fn say(report: &mut dyn FnMut(Check), name: &str, ok: bool, message: impl Into<String>) {
    report(Check { check: name.to_string(), ok, message: message.into() });
}

/// Record a video, kill the program without warning partway through (like a crash or a power
/// cut), then find out what survived. Windows only.
pub fn run_video_crash_test(display: u32, seconds_before_kill: f64, out: &Path, report: &mut dyn FnMut(Check)) -> bool {
    if !cfg!(windows) {
        say(report, "crash-test", false, "Screen capture and video only work on Windows.");
        return false;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            say(report, "crash-test", false, format!("The program could not find itself to start a recording: {e}"));
            return false;
        }
    };
    if let Err(e) = fs::create_dir_all(out) {
        say(report, "crash-test", false, format!("Could not create the folder {}: {e}", out.display()));
        return false;
    }
    let file = out.join("test_fragmented.mp4");
    let marker = out.join("recording-started.txt");
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&marker);

    let mut child = match Command::new(exe)
        .args(["video-test", "--fragmented", "--plain", "--seconds", "120", "--display"])
        .arg(display.to_string())
        .arg("--start-marker")
        .arg(&marker)
        .arg("--out")
        .arg(out)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            say(report, "crash-test", false, format!("Could not start the recording: {e}"));
            return false;
        }
    };

    // Wait for the recording to begin: the program drops a marker file at that exact moment.
    let waiting_since = Instant::now();
    let recording_since = loop {
        if let Ok(Some(_)) = child.try_wait() {
            let mut said = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_string(&mut said);
            }
            say(
                report,
                "crash-test",
                false,
                format!("The recording stopped by itself before it could be killed. What it said: {}", said.trim()),
            );
            return false;
        }
        if marker.is_file() {
            break Instant::now();
        }
        if waiting_since.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            say(
                report,
                "crash-test",
                false,
                "The recording did not begin within 30 seconds, so there was nothing to test.",
            );
            return false;
        }
        sleep(Duration::from_millis(100));
    };

    sleep(Duration::from_secs_f64(seconds_before_kill.clamp(2.0, 60.0)));

    // No warning and no chance to tidy up: this is what a crash or a power cut looks like to the program.
    let _ = child.kill();
    let _ = child.wait();
    let recorded_ms = recording_since.elapsed().as_millis() as u64;
    say(
        report,
        "crash-test",
        true,
        format!(
            "Recorded for about {:.1} seconds, then stopped the program without any warning.",
            recorded_ms as f64 / 1000.0
        ),
    );

    run_crash_report(system_video_platform().as_ref(), &file, recorded_ms, report)
}

/// Start the real recorder, record through a pause, kill it without warning, then repair the
/// project and look at every file. Windows only.
pub fn run_recorder_crash_test(
    display: u32,
    microphone: &str,
    output: &str,
    seconds_before_kill: f64,
    out: &Path,
    report: &mut dyn FnMut(Check),
) -> bool {
    if !cfg!(windows) {
        say(report, "crash-test", false, "Screen capture and sound only work on Windows.");
        return false;
    }
    let plan = match plan_sound(
        system_audio_platform().as_ref(),
        super::sound_choice(microphone).as_ref(),
        super::sound_choice(output).as_ref(),
    ) {
        Ok(p) => p,
        Err(why) => {
            say(report, "crash-test", false, why);
            return false;
        }
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            say(report, "crash-test", false, format!("The program could not find itself to start a recording: {e}"));
            return false;
        }
    };
    let project = out.join("lesson.capture");
    if project.exists() {
        let _ = fs::remove_dir_all(&project);
    }
    if let Err(e) = fs::create_dir_all(out) {
        say(report, "crash-test", false, format!("Could not create the folder {}: {e}", out.display()));
        return false;
    }
    say(report, "crash-test", true, plan.describe());

    // The recorder's own notes about its start-up go to a file, for finding out what went wrong.
    let diagnostics =
        fs::File::create(out.join("recorder-diagnostics.txt")).map(Stdio::from).unwrap_or_else(|_| Stdio::null());
    let mut child = match Command::new(exe)
        .args(["record", "--source", "screen", "--countdown-ms", "0", "--display"])
        .arg(display.to_string())
        .arg("--microphone")
        .arg(microphone)
        .arg("--output")
        .arg(output)
        .arg("--project")
        .arg(&project)
        // Standard input stays open (and unused except for the pause) so the recorder does not stop by itself.
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(diagnostics)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            say(report, "crash-test", false, format!("Could not start the recorder: {e}"));
            return false;
        }
    };

    // Watch what the recorder says; note the moment it starts recording.
    let said: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (started_tx, started_rx) = mpsc::channel::<Instant>();
    if let Some(stdout) = child.stdout.take() {
        let said = Arc::clone(&said);
        std::thread::spawn(move || {
            let mut announced = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !announced {
                    if let Ok(Event::State { state: State::Recording, .. }) = decode_event(&line) {
                        announced = true;
                        let _ = started_tx.send(Instant::now());
                    }
                }
                said.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            }
        });
    }
    let recording_since = {
        let waiting_since = Instant::now();
        loop {
            if let Ok(t) = started_rx.try_recv() {
                break t;
            }
            if let Ok(Some(_)) = child.try_wait() {
                let lines = said.lock().unwrap_or_else(|e| e.into_inner()).join(" ");
                say(
                    report,
                    "crash-test",
                    false,
                    format!("The recorder stopped by itself before it could be killed. What it said: {}", lines.trim()),
                );
                return false;
            }
            if waiting_since.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                say(
                    report,
                    "crash-test",
                    false,
                    "The recorder did not begin recording within 60 seconds, so there was nothing to test.",
                );
                return false;
            }
            sleep(Duration::from_millis(100));
        }
    };

    // Record, pause for 2 seconds (that time is not recorded), carry on, then pull the plug.
    let total = Duration::from_secs_f64(seconds_before_kill.clamp(8.0, 120.0));
    let pause_for = Duration::from_secs(2);
    sleep(Duration::from_secs(3).saturating_sub(recording_since.elapsed()));
    let mut stdin = child.stdin.take();
    let mut send = |command: RecorderCommand| {
        if let Some(pipe) = stdin.as_mut() {
            let _ = writeln!(pipe, "{}", encode_command(&command));
            let _ = pipe.flush();
        }
    };
    send(RecorderCommand::Pause);
    sleep(pause_for);
    send(RecorderCommand::Resume);
    sleep(total.saturating_sub(recording_since.elapsed()));

    // No warning and no chance to tidy up: this is what a crash or a power cut looks like to the program.
    let _ = child.kill();
    let _ = child.wait();
    let recorded_ms = recording_since.elapsed().saturating_sub(pause_for).as_millis() as u64;
    say(
        report,
        "crash-test",
        true,
        format!(
            "Recorded for about {:.1} seconds (not counting a 2 second pause), then stopped the program without any warning.",
            recorded_ms as f64 / 1000.0
        ),
    );

    run_recorder_crash_report(system_video_platform().as_ref(), &project, 1, recorded_ms, &plan, report)
}
