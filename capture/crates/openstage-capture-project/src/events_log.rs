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

/// Where log lines go. [`EventLogWriter`] is the real one; tests use ones that are slow or that fail.
pub trait LogSink: Send + 'static {
    fn append(&mut self, t: u64, event: LogEvent) -> io::Result<()>;
    fn flush_sync(&mut self) -> io::Result<()>;
}

impl LogSink for EventLogWriter {
    fn append(&mut self, t: u64, event: LogEvent) -> io::Result<()> {
        EventLogWriter::append(self, t, event)
    }

    fn flush_sync(&mut self) -> io::Result<()> {
        EventLogWriter::flush_sync(self)
    }
}

/// A forced save to the disk that takes this long counts as slow.
pub const SLOW_SAVE_MS: u64 = 100;

/// How the forced saves of a log went, for telling the author about a slow drive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveStats {
    /// Forced saves made.
    pub saves: u32,
    /// Of those, how many took [`SLOW_SAVE_MS`] or more.
    pub slow_saves: u32,
    /// The longest one, in milliseconds.
    pub longest_ms: u64,
}

enum LogJob {
    Line(u64, LogEvent),
    Sync,
}

#[derive(Default)]
struct Shared {
    failure: Option<String>,
    stats: SaveStats,
}

/// Writes `events.jsonl` on a thread of its own.
///
/// Forcing a file to the disk can wait a long time on a busy or slow drive (the owner's long test
/// measured 0.5 to 0.8 seconds, several times a minute), and while the recording loop waits it looks at
/// neither the screen nor the sound. So the loop only hands lines and save requests over, which takes
/// no time, and this thread does the writing and the waiting. The cost of a slow drive is that a crash
/// can lose more than a second of log lines; the video and sound files are not affected, and their
/// own writing is not done here.
///
/// A write that fails is remembered, and the next call says so.
pub struct EventLogThread {
    tx: Option<std::sync::mpsc::Sender<LogJob>>,
    thread: Option<std::thread::JoinHandle<io::Result<()>>>,
    shared: std::sync::Arc<std::sync::Mutex<Shared>>,
}

impl EventLogThread {
    /// Open (or create) the log at `path` and write it from a thread.
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self::start(EventLogWriter::create(path)?))
    }

    /// Write to `sink` from a thread.
    pub fn start(mut sink: impl LogSink) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<LogJob>();
        let shared = std::sync::Arc::new(std::sync::Mutex::new(Shared::default()));
        let for_thread = std::sync::Arc::clone(&shared);
        let thread = std::thread::spawn(move || {
            let save = |sink: &mut dyn LogSink| {
                let started = std::time::Instant::now();
                let result = sink.flush_sync();
                let took = started.elapsed().as_millis() as u64;
                let mut shared = for_thread.lock().unwrap_or_else(|e| e.into_inner());
                shared.stats.saves += 1;
                shared.stats.longest_ms = shared.stats.longest_ms.max(took);
                if took >= SLOW_SAVE_MS {
                    shared.stats.slow_saves += 1;
                }
                result
            };
            for job in rx {
                if for_thread.lock().unwrap_or_else(|e| e.into_inner()).failure.is_some() {
                    continue; // after a failure, do nothing more; the first failure is the one reported
                }
                let result = match job {
                    LogJob::Line(t, event) => sink.append(t, event),
                    LogJob::Sync => save(&mut sink),
                };
                if let Err(e) = result {
                    for_thread.lock().unwrap_or_else(|p| p.into_inner()).failure = Some(e.to_string());
                }
            }
            // Whether the program finished or went away, put what was given to the disk.
            save(&mut sink)
        });
        Self { tx: Some(tx), thread: Some(thread), shared }
    }

    fn check(&self) -> io::Result<()> {
        match self.shared.lock().unwrap_or_else(|e| e.into_inner()).failure.as_ref() {
            Some(why) => Err(io::Error::other(why.clone())),
            None => Ok(()),
        }
    }

    fn send(&self, job: LogJob) -> io::Result<()> {
        self.check()?;
        self.tx
            .as_ref()
            .ok_or_else(|| io::Error::other("The log was already closed."))?
            .send(job)
            .map_err(|_| io::Error::other("The thread that writes the log has stopped."))
    }

    /// Queue a line. Returns at once.
    pub fn append(&self, t: u64, event: LogEvent) -> io::Result<()> {
        self.send(LogJob::Line(t, event))
    }

    /// Queue a forced save to the disk. Returns at once; the wait is the thread's.
    pub fn flush_sync(&self) -> io::Result<()> {
        self.send(LogJob::Sync)
    }

    /// Write everything that is queued, put it on the disk, and say how the forced saves went.
    pub fn finish(mut self) -> io::Result<SaveStats> {
        drop(self.tx.take());
        let last = match self.thread.take() {
            Some(thread) => thread.join().map_err(|_| io::Error::other("The thread that writes the log failed."))?,
            None => Ok(()),
        };
        self.check()?;
        last?;
        let stats = self.shared.lock().unwrap_or_else(|e| e.into_inner()).stats;
        Ok(stats)
    }
}

impl Drop for EventLogThread {
    fn drop(&mut self) {
        // If the recording was abandoned, still let the thread put the log on the disk.
        drop(self.tx.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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

    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    type Kept = Arc<Mutex<Vec<(u64, LogEvent)>>>;

    /// A log on a drive that can be made busy: each forced save first waits `hold` and then for the gate to
    /// open (an opened or dropped gate lets every save through at once). What reaches it is kept.
    struct BusyDrive {
        lines: Kept,
        gate: Receiver<()>,
        hold: Duration,
        fail_on: Option<u64>,
    }

    fn busy_drive(hold_ms: u64) -> (BusyDrive, Sender<()>, Kept) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let (gate_tx, gate) = channel();
        let drive = BusyDrive { lines: Arc::clone(&lines), gate, hold: Duration::from_millis(hold_ms), fail_on: None };
        (drive, gate_tx, lines)
    }

    impl LogSink for BusyDrive {
        fn append(&mut self, t: u64, event: LogEvent) -> io::Result<()> {
            if self.fail_on == Some(t) {
                return Err(io::Error::other("the drive refused the line"));
            }
            self.lines.lock().unwrap().push((t, event));
            Ok(())
        }

        fn flush_sync(&mut self) -> io::Result<()> {
            std::thread::sleep(self.hold);
            let _ = self.gate.recv();
            Ok(())
        }
    }

    fn marker(label: &str) -> LogEvent {
        LogEvent::Marker { label: label.into() }
    }

    #[test]
    fn a_log_written_from_its_own_thread_holds_the_same_lines_in_the_same_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let log = EventLogThread::open(&path).unwrap();
        for t in 0..500u64 {
            log.append(t, LogEvent::CursorMove { x: t as i32, y: 2 * t as i32 }).unwrap();
            if t % 100 == 99 {
                log.flush_sync().unwrap();
            }
        }
        let stats = log.finish().unwrap();
        let lines = read_log(&path).unwrap();
        assert_eq!(lines.len(), 500);
        assert!(lines.iter().enumerate().all(|(i, l)| l.t == i as u64));
        assert_eq!(stats.saves, 6, "five asked for and the last one when it was finished");
    }

    #[test]
    fn the_caller_is_not_held_up_while_the_drive_is_busy_and_nothing_is_lost() {
        let (drive, gate_tx, lines) = busy_drive(0);
        let log = Arc::new(EventLogThread::start(drive));
        // The drive does not answer a forced save until the gate opens. The caller asks for several and
        // carries on; if it had to wait, this thread would not finish.
        let (done_tx, done_rx) = channel();
        let for_caller = Arc::clone(&log);
        std::thread::spawn(move || {
            for t in 0..3u64 {
                for_caller.append(t, marker("a")).unwrap();
                for_caller.flush_sync().unwrap();
            }
            done_tx.send(()).unwrap();
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("handing lines and save requests over must not wait for the drive");
        std::thread::sleep(Duration::from_millis(150));
        drop(gate_tx); // the drive answers
        let log = Arc::into_inner(log).expect("the caller is done with it");
        let stats = log.finish().unwrap();
        let kept = lines.lock().unwrap();
        assert_eq!(kept.iter().map(|(t, _)| *t).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(stats.saves, 4);
        assert!(stats.slow_saves >= 1 && stats.longest_ms >= SLOW_SAVE_MS, "{stats:?}");
    }

    #[test]
    fn a_slow_save_is_counted_and_a_quick_one_is_not() {
        let (drive, gate_tx, _) = busy_drive(0);
        drop(gate_tx);
        let log = EventLogThread::start(drive);
        log.flush_sync().unwrap();
        let stats = log.finish().unwrap();
        assert_eq!((stats.saves, stats.slow_saves), (2, 0));
        assert!(stats.longest_ms < SLOW_SAVE_MS);

        let (drive, gate_tx, _) = busy_drive(130);
        drop(gate_tx);
        let log = EventLogThread::start(drive);
        log.flush_sync().unwrap();
        let stats = log.finish().unwrap();
        assert_eq!((stats.saves, stats.slow_saves), (2, 2), "the one asked for and the last one both took 130 ms");
        assert!(stats.longest_ms >= 130, "{stats:?}");
    }

    #[test]
    fn a_write_that_fails_is_reported_by_the_next_call_and_by_finish() {
        let (mut drive, gate_tx, _) = busy_drive(0);
        drop(gate_tx);
        drive.fail_on = Some(2);
        let log = EventLogThread::start(drive);
        log.append(1, marker("a")).unwrap();
        log.append(2, marker("b")).unwrap(); // accepted for queueing; the thread finds it cannot write it
        let mut told = false;
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(5));
            if let Err(e) = log.append(3, marker("c")) {
                assert!(e.to_string().contains("the drive refused the line"), "{e}");
                told = true;
                break;
            }
        }
        assert!(told, "the failure is reported to the caller");
        assert!(log.finish().unwrap_err().to_string().contains("refused"));
    }

    #[test]
    fn a_log_whose_thread_is_dropped_without_finishing_is_still_on_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        {
            let log = EventLogThread::open(&path).unwrap();
            log.append(7, marker("kept")).unwrap();
        }
        assert_eq!(read_log(&path).unwrap().len(), 1);
    }
}
