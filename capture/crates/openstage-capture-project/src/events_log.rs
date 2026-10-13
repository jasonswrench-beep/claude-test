// SPDX-License-Identifier: Apache-2.0
//! The interaction log, `events.jsonl`: one JSON object per line, only ever appended to.
//!
//! This is the raw record of what the author did while recording (cursor, clicks, keys, windows,
//! clicked element names). It can contain private information, so it lives in the capture
//! project and is never published (see `docs/capture-project-format.md`).

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// What happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LogEvent {
    CursorMove {
        x: i32,
        y: i32,
    },
    Click {
        x: i32,
        y: i32,
        button: String,
    },
    /// Never written for password fields.
    Key {
        key: String,
        modifiers: Vec<String>,
    },
    Window {
        title: String,
        process: String,
    },
    /// The name and role of the element that was clicked, from Windows' accessibility information.
    Element {
        name: String,
        role: String,
    },
    Marker {
        label: String,
    },
}

/// One line of the log: when it happened (milliseconds of recording time) and what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    pub t: u64,
    #[serde(flatten)]
    pub event: LogEvent,
}

/// Appends to `events.jsonl`.
pub struct EventLogWriter {
    out: BufWriter<File>,
}

impl EventLogWriter {
    /// Open (or create) the log for appending.
    pub fn create(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { out: BufWriter::new(file) })
    }

    pub fn append(&mut self, t: u64, event: LogEvent) -> io::Result<()> {
        let mut line =
            serde_json::to_string(&LogLine { t, event }).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        self.out.write_all(line.as_bytes())
    }

    /// Push everything written so far to the disk.
    pub fn flush_sync(&mut self) -> io::Result<()> {
        self.out.flush()?;
        self.out.get_ref().sync_data()
    }
}

/// Read the log. Lines that cannot be read (such as a line cut short by a crash) are skipped.
pub fn read_log(path: &Path) -> io::Result<Vec<LogLine>> {
    let mut out = Vec::new();
    for line in BufReader::new(File::open(path)?).lines() {
        if let Ok(parsed) = serde_json::from_str::<LogLine>(&line?) {
            out.push(parsed);
        }
    }
    Ok(out)
}

/// If the last line was cut short by a crash (the file does not end with a newline), remove it.
/// Returns whether anything was removed.
pub fn trim_incomplete_tail(path: &Path) -> io::Result<bool> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(false);
    }
    // Find the last newline and cut just after it.
    let mut contents = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut contents)?;
    let keep = contents.iter().rposition(|b| *b == b'\n').map(|i| i + 1).unwrap_or(0);
    file.set_len(keep as u64)?;
    file.sync_all()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn events_are_appended_one_per_line_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut w = EventLogWriter::create(&path).unwrap();
        w.append(10, LogEvent::CursorMove { x: 1, y: 2 }).unwrap();
        w.append(20, LogEvent::Click { x: 1, y: 2, button: "left".into() }).unwrap();
        w.append(30, LogEvent::Element { name: "Save".into(), role: "button".into() }).unwrap();
        w.flush_sync().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.lines().next().unwrap().starts_with(r#"{"t":10,"type":"cursorMove""#));
        let lines = read_log(&path).unwrap();
        assert_eq!(
            lines[2],
            LogLine { t: 30, event: LogEvent::Element { name: "Save".into(), role: "button".into() } }
        );
    }

    #[test]
    fn reopening_appends_instead_of_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        for t in [1u64, 2] {
            let mut w = EventLogWriter::create(&path).unwrap();
            w.append(t, LogEvent::Marker { label: "m".into() }).unwrap();
            w.flush_sync().unwrap();
        }
        assert_eq!(read_log(&path).unwrap().len(), 2);
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        fs::write(&path, "{\"t\":1,\"type\":\"marker\",\"label\":\"a\"}\n{\"t\":2,\"type\":\"mar").unwrap();
        assert_eq!(read_log(&path).unwrap().len(), 1);
        assert!(trim_incomplete_tail(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"t\":1,\"type\":\"marker\",\"label\":\"a\"}\n");
        assert!(!trim_incomplete_tail(&path).unwrap(), "trimming twice changes nothing");
    }
}
