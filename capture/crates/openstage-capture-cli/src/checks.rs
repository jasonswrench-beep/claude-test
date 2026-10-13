// SPDX-License-Identifier: Apache-2.0
//! The commands that test this computer (`screen-test`, `video-test`, `video-crash-test`) share
//! how they print their findings and how they end.

use openstage_capture_protocol::{encode_event, Event};
use openstage_capture_windows::{run_crash_report, system_video_platform, Check};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
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
