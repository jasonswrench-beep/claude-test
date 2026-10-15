// SPDX-License-Identifier: Apache-2.0
//! The conversation between OpenStage and the capture program.
//!
//! Both directions use **one JSON object per line**:
//!
//! * the program prints [`Event`]s on standard output, and
//! * OpenStage sends [`Command`]s on the program's standard input.
//!
//! The first line the program prints is always a `hello` event that names the
//! protocol version. The last line of a recording is a `result` event.
//! Logs and diagnostics go to standard error, never to standard output.
//!
//! Anything that reads events must ignore event types it does not know, so the
//! protocol can grow without breaking older readers. The full description is in
//! `docs/capture-protocol-spec.md`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The protocol version this crate speaks.
pub const PROTOCOL_VERSION: &str = "0.1";

/// The longest line, in bytes, that either side will accept.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// What the recorder is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Idle,
    Countdown,
    Recording,
    Paused,
    Stopping,
    Stopped,
    Error,
}

/// One recording source the program can offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub available: bool,
    /// A plain-language note for people, such as why a source is unavailable.
    pub note: String,
}

/// A line the program prints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// Always the first line.
    Hello { protocol: String, program: String, program_version: String, platform: String },
    /// The answer to `probe`.
    Probe { platform: String, sources: Vec<SourceInfo> },
    /// The recorder changed state, or is reporting it again (about once a second).
    State {
        state: State,
        elapsed_ms: u64,
        take: u32,
        /// True while recording time runs and the recorder is also noting what the author does (the cursor,
        /// clicks and keys; story A6). Left out when false. Say it wherever the state is shown or spoken, so
        /// the indicator is not only something seen.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        watching: bool,
    },
    /// A sound level, with a plain-language description for people who cannot see a meter.
    Level { source: String, peak: f32, text: String, elapsed_ms: u64 },
    /// Something worth knowing that did not stop the recording.
    Warning { code: String, message: String },
    /// Something went wrong. If `fatal`, the program is stopping.
    Error { code: String, message: String, fatal: bool },
    /// A marker was added at this point.
    Marker { label: String, elapsed_ms: u64 },
    /// The author started again: this take was set aside (not erased) and a new one begins.
    TakeSetAside { take: u32, next_take: u32 },
    /// `recover` repaired an interrupted take.
    Recovered { take: u32, duration_ms: u64, max_loss_ms: u64, segments: u32 },
    /// One finding from a test or setup check, in plain language (see `screen-test`).
    Check { check: String, ok: bool, message: String },
    /// How far a long job has got, in words, so that someone who cannot see a progress bar can hear it. Sent by
    /// `render` about every tenth of the way.
    Progress { task: String, percent: u32, message: String },
    /// The last line of a `record` or `recover` run.
    Result { ok: bool, project: String, take: u32, state: String, duration_ms: u64, segments: u32 },
}

impl Event {
    /// The `hello` line for this program.
    pub fn hello(program_version: &str) -> Event {
        Event::Hello {
            protocol: PROTOCOL_VERSION.to_string(),
            program: "openstage-capture".to_string(),
            program_version: program_version.to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    }
}

/// A line OpenStage sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Command {
    Pause,
    Resume,
    /// Finish cleanly: make every file complete, then print the result.
    Stop,
    /// Set this take aside (never erase it) and start a new one with the same settings.
    Restart,
    Marker {
        #[serde(default)]
        label: Option<String>,
    },
}

/// Why a line could not be used.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("the line is empty")]
    Empty,
    #[error("the line is longer than {} bytes", MAX_LINE_BYTES)]
    TooLong,
    #[error("not a valid message: {0}")]
    Invalid(String),
}

fn clean_line(line: &str) -> Result<&str, ProtocolError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::TooLong);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::Empty);
    }
    Ok(trimmed)
}

/// Turn an event into one line of JSON (without the trailing newline).
pub fn encode_event(event: &Event) -> String {
    // Events contain only strings, numbers, and booleans, so this cannot fail.
    serde_json::to_string(event).expect("events always serialize")
}

/// Read one line as an event.
pub fn decode_event(line: &str) -> Result<Event, ProtocolError> {
    let line = clean_line(line)?;
    serde_json::from_str(line).map_err(|e| ProtocolError::Invalid(e.to_string()))
}

/// Read one line as a command.
pub fn decode_command(line: &str) -> Result<Command, ProtocolError> {
    let line = clean_line(line)?;
    serde_json::from_str(line).map_err(|e| ProtocolError::Invalid(e.to_string()))
}

/// Turn a command into one line of JSON (without the trailing newline).
pub fn encode_command(command: &Command) -> String {
    serde_json::to_string(command).expect("commands always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_events() -> Vec<Event> {
        vec![
            Event::hello("0.1.0"),
            Event::Probe {
                platform: "linux".into(),
                sources: vec![SourceInfo { id: "synthetic".into(), available: true, note: "test source".into() }],
            },
            Event::State { state: State::Recording, elapsed_ms: 1500, take: 2, watching: false },
            Event::State { state: State::Recording, elapsed_ms: 1600, take: 2, watching: true },
            Event::Level { source: "mic".into(), peak: 0.5, text: "good".into(), elapsed_ms: 10 },
            Event::Warning { code: "bad-command".into(), message: "nope".into() },
            Event::Error { code: "write-failed".into(), message: "disk full".into(), fatal: true },
            Event::Marker { label: "here".into(), elapsed_ms: 99 },
            Event::TakeSetAside { take: 1, next_take: 2 },
            Event::Recovered { take: 1, duration_ms: 4000, max_loss_ms: 1000, segments: 3 },
            Event::Check { check: "screen-capture".into(), ok: true, message: "Received 90 pictures.".into() },
            Event::Progress { task: "export".into(), percent: 40, message: "Exported 40 percent.".into() },
            Event::Result {
                ok: true,
                project: "p".into(),
                take: 1,
                state: "complete".into(),
                duration_ms: 1000,
                segments: 1,
            },
        ]
    }

    #[test]
    fn every_event_round_trips() {
        for event in all_events() {
            let line = encode_event(&event);
            assert!(!line.contains('\n'), "an event must be a single line");
            assert_eq!(decode_event(&line).unwrap(), event, "line was {line}");
        }
    }

    #[test]
    fn events_use_camel_case_names() {
        let line = encode_event(&Event::State { state: State::Paused, elapsed_ms: 5, take: 1, watching: false });
        assert_eq!(line, r#"{"event":"state","state":"paused","elapsedMs":5,"take":1}"#);
        let line = encode_event(&Event::State { state: State::Recording, elapsed_ms: 5, take: 1, watching: true });
        assert_eq!(line, r#"{"event":"state","state":"recording","elapsedMs":5,"take":1,"watching":true}"#);
        let line = encode_event(&Event::TakeSetAside { take: 1, next_take: 2 });
        assert_eq!(line, r#"{"event":"takeSetAside","take":1,"nextTake":2}"#);
    }

    #[test]
    fn hello_names_the_protocol_version() {
        match Event::hello("9.9.9") {
            Event::Hello { protocol, program, program_version, .. } => {
                assert_eq!(protocol, PROTOCOL_VERSION);
                assert_eq!(program, "openstage-capture");
                assert_eq!(program_version, "9.9.9");
            }
            other => panic!("not hello: {other:?}"),
        }
    }

    #[test]
    fn commands_decode_from_plain_lines() {
        assert_eq!(decode_command(r#"{"cmd":"pause"}"#), Ok(Command::Pause));
        assert_eq!(decode_command(r#"{"cmd":"resume"}"#), Ok(Command::Resume));
        assert_eq!(decode_command(r#"{"cmd":"stop"}"#), Ok(Command::Stop));
        assert_eq!(decode_command(r#"{"cmd":"restart"}"#), Ok(Command::Restart));
        assert_eq!(decode_command(r#"{"cmd":"marker"}"#), Ok(Command::Marker { label: None }));
        assert_eq!(
            decode_command(r#"{"cmd":"marker","label":"oops"}"#),
            Ok(Command::Marker { label: Some("oops".into()) })
        );
    }

    #[test]
    fn commands_round_trip() {
        for command in [
            Command::Pause,
            Command::Resume,
            Command::Stop,
            Command::Restart,
            Command::Marker { label: Some("x".into()) },
        ] {
            assert_eq!(decode_command(&encode_command(&command)).unwrap(), command);
        }
    }

    #[test]
    fn windows_line_endings_and_spaces_are_fine() {
        assert_eq!(decode_command("  {\"cmd\":\"stop\"}\r\n"), Ok(Command::Stop));
    }

    #[test]
    fn extra_fields_are_ignored_so_the_protocol_can_grow() {
        assert_eq!(decode_command(r#"{"cmd":"stop","future":true}"#), Ok(Command::Stop));
    }

    #[test]
    fn bad_input_is_an_error_not_a_crash() {
        assert_eq!(decode_command(""), Err(ProtocolError::Empty));
        assert_eq!(decode_command("   \r\n"), Err(ProtocolError::Empty));
        assert!(matches!(decode_command("not json"), Err(ProtocolError::Invalid(_))));
        assert!(matches!(decode_command(r#"{"cmd":"explode"}"#), Err(ProtocolError::Invalid(_))));
        assert!(matches!(decode_command(r#"{"nocmd":1}"#), Err(ProtocolError::Invalid(_))));
    }

    #[test]
    fn overlong_lines_are_refused() {
        let long = format!(r#"{{"cmd":"marker","label":"{}"}}"#, "a".repeat(MAX_LINE_BYTES));
        assert_eq!(decode_command(&long), Err(ProtocolError::TooLong));
    }
}
