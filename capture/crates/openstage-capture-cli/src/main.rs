// SPDX-License-Identifier: Apache-2.0
//! `openstage-capture`: the recording program.
//!
//! It has no window of its own. OpenStage starts it, sends it one-line JSON commands on standard
//! input, and reads one-line JSON events from standard output (see `docs/capture-protocol-spec.md`).
//! Standard error is for diagnostics only.
//!
//! Exit codes: 0 success, 1 something went wrong, 2 not available (an unknown source, or a
//! command that has not been built yet).

use clap::{Parser, Subcommand};
use openstage_capture::{
    run_session, EventSink, Input, InputSource, SessionConfig, Source, SyntheticSource, SystemClock,
};
use openstage_capture_project::recover_project;
use openstage_capture_protocol::{decode_command, encode_event, Event, ProtocolError, SourceInfo, MAX_LINE_BYTES};
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
        /// Where picture and sound come from. Only `synthetic` (a test source) exists so far.
        #[arg(long, default_value = "synthetic")]
        source: String,
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
    },
    /// Repair a recording that was interrupted.
    Recover {
        #[arg(long)]
        project: PathBuf,
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

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Probe => {
            hello();
            say(Event::Probe {
                platform: std::env::consts::OS.to_string(),
                sources: vec![
                    SourceInfo {
                        id: "synthetic".into(),
                        available: true,
                        note: "A built-in test source. It makes fake picture and sound so the recorder can be tested anywhere.".into(),
                    },
                    SourceInfo {
                        id: "windows".into(),
                        available: false,
                        note: "Screen, window, microphone, and system sound capture for Windows is not built yet.".into(),
                    },
                ],
            });
            ExitCode::SUCCESS
        }
        Cmd::Record {
            project,
            source,
            countdown_ms,
            segment_ms,
            flush_ms,
            duration_ms,
            ignore_stdin_eof,
            title,
            language,
        } => {
            hello();
            if source != "synthetic" {
                fatal(
                    "source-unavailable",
                    format!("The source \"{source}\" is not available. Use \"synthetic\" for now."),
                );
                return ExitCode::from(2);
            }
            let mut cfg = SessionConfig::new(&project);
            cfg.title = title;
            cfg.language = language;
            cfg.countdown_ms = countdown_ms;
            cfg.segment_ms = segment_ms.max(1);
            cfg.flush_ms = flush_ms.max(1);
            cfg.max_duration_ms = duration_ms;
            cfg.stop_when_input_closes = !ignore_stdin_eof;

            let (tx, rx) = mpsc::channel();
            read_commands(tx);
            let make_source = || -> Box<dyn Source> { Box::new(SyntheticSource::new()) };
            let result =
                run_session(&cfg, &make_source, &mut ChannelInput { rx }, &mut StdoutSink, &SystemClock::new());
            match result {
                Ok(outcome) => {
                    say(Event::Result {
                        ok: outcome.ok,
                        project: path_text(&project),
                        take: outcome.take,
                        state: outcome.state.clone(),
                        duration_ms: outcome.duration_ms,
                        segments: outcome.segments,
                    });
                    if outcome.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    fatal("project", e.to_string());
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
