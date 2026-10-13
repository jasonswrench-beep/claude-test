// SPDX-License-Identifier: Apache-2.0
//! The recording session: countdown, record, pause, resume, stop, and start again.
//!
//! A session runs one loop. Each turn it reads any commands that arrived, moves the recording
//! forward, writes what the source produced into crash-safe segments, and reports what is
//! happening as protocol events. Time comes from a [`Clock`], so tests can run a ten-minute
//! recording instantly.
//!
//! Rules this loop keeps (see `docs/prd.md`, stories A3, A4, A8, A10):
//!
//! * Pausing stops recording time. The recording is exactly as long as the time spent recording.
//! * Data is forced to disk at least every `flush_ms`, so a crash loses at most that much.
//! * **Start again never erases anything.** The take is marked as set aside and a new one begins.
//! * Stop is graceful: every file is made complete before the result is reported.
//! * A write problem is reported as a fatal error and the take is finished as far as it can be.
//! * If the program that is listening goes away, the recording stops gracefully rather than
//!   running on with nobody watching.

use crate::clock::Clock;
use crate::media::{MediaFactory, TakeMedia};
use openstage_capture_project::{
    create_project, create_take_with, finish_take, next_take_id, open_project, set_aside_take, take_dir,
    EventLogWriter, LogEvent, ProjectError,
};
use openstage_capture_protocol::{Command, Event, State};
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// How a session is set up.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// The capture project folder (created if it does not exist).
    pub project: PathBuf,
    pub title: String,
    pub language: String,
    /// Time between "record" and the recording actually starting. 0 turns the countdown off.
    pub countdown_ms: u64,
    /// How much recording time goes into one segment file.
    pub segment_ms: u64,
    /// How often data is forced to disk. A crash can lose at most this much.
    pub flush_ms: u64,
    /// How often the loop wakes up.
    pub tick_ms: u64,
    /// Stop by itself after this much recording time (used by tests and demos).
    pub max_duration_ms: Option<u64>,
    /// Whether to stop gracefully when the command input closes.
    pub stop_when_input_closes: bool,
}

impl SessionConfig {
    pub fn new(project: impl Into<PathBuf>) -> Self {
        Self {
            project: project.into(),
            title: String::new(),
            language: "en-US".to_string(),
            countdown_ms: 3000,
            segment_ms: 2000,
            flush_ms: 1000,
            tick_ms: 20,
            max_duration_ms: None,
            stop_when_input_closes: true,
        }
    }
}

/// Something that arrived on the command input.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Command(Command),
    /// A line that could not be understood, with a plain-language reason.
    Invalid(String),
    /// The command input closed (for example, OpenStage went away).
    Closed,
}

/// Where commands come from.
pub trait InputSource {
    /// Everything that arrived since the last call.
    fn poll(&mut self, now_ms: u64) -> Vec<Input>;
}

/// Where events go.
pub trait EventSink {
    /// Send one event. Returns `false` if nobody is listening any more.
    fn emit(&mut self, event: Event) -> bool;
}

/// What a finished session amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub ok: bool,
    /// The last take number, or 0 if no take was ever started.
    pub take: u32,
    /// `complete`, or `none` if the session ended before any take began.
    pub state: String,
    pub duration_ms: u64,
    pub segments: u32,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("file problem: {0}")]
    Io(#[from] io::Error),
    /// Capturing or encoding failed; the text says why, in plain language.
    #[error("{0}")]
    Media(String),
}

/// A plain-language description of a sound level, for people who cannot see a meter.
pub fn describe_level(peak: f32) -> &'static str {
    if peak < 0.01 {
        "silent"
    } else if peak < 0.1 {
        "quiet"
    } else if peak < 0.7 {
        "good"
    } else if peak < 0.95 {
        "loud"
    } else {
        "too loud"
    }
}

struct TakeRun {
    id: u32,
    media: Option<Box<dyn TakeMedia>>,
    events: EventLogWriter,
    last_flush_wall: u64,
}

struct Runner<'a> {
    cfg: &'a SessionConfig,
    factory: &'a dyn MediaFactory,
    sink: &'a mut dyn EventSink,
    clock: &'a dyn Clock,
    state: State,
    run: Option<TakeRun>,
    next_take: u32,
    last_take: u32,
    media_ms: u64,
    countdown_started: u64,
    last_tick: u64,
    last_state_emit: u64,
    last_level_emit: u64,
    sink_alive: bool,
    outcome: Option<Outcome>,
}

impl<'a> Runner<'a> {
    fn emit(&mut self, event: Event) {
        if self.sink_alive && !self.sink.emit(event) {
            self.sink_alive = false;
        }
    }

    /// The take a state report belongs to: the open one; else, once finished, the one just
    /// finished (0 if none was ever started); else (a countdown) the one about to begin.
    fn current_take(&self) -> u32 {
        match &self.run {
            Some(run) => run.id,
            None if matches!(self.state, State::Stopped | State::Error) => self.last_take,
            None => self.next_take,
        }
    }

    fn emit_state(&mut self) {
        let now = self.clock.now_ms();
        let elapsed_ms =
            if self.state == State::Countdown { now.saturating_sub(self.countdown_started) } else { self.media_ms };
        let take = self.current_take();
        self.emit(Event::State { state: self.state, elapsed_ms, take });
        self.last_state_emit = now;
    }

    fn warn(&mut self, code: &str, message: &str) {
        self.emit(Event::Warning { code: code.to_string(), message: message.to_string() });
    }

    fn start_countdown_or_take(&mut self) -> Result<(), SessionError> {
        if self.cfg.countdown_ms > 0 {
            self.state = State::Countdown;
            self.countdown_started = self.clock.now_ms();
            self.emit_state();
            Ok(())
        } else {
            self.begin_take()
        }
    }

    fn begin_take(&mut self) -> Result<(), SessionError> {
        let id = self.next_take;
        self.next_take += 1;
        create_take_with(
            &self.cfg.project,
            id,
            vec![self.factory.source_id()],
            None,
            self.factory.declared_flush_ms(self.cfg),
            self.factory.media_files(),
        )?;
        let dir = take_dir(&self.cfg.project, id);
        let media = self.factory.begin(&dir, self.cfg)?;
        let events = EventLogWriter::create(&dir.join("events.jsonl"))?;
        self.run = Some(TakeRun { id, media: Some(media), events, last_flush_wall: self.clock.now_ms() });
        self.last_take = id;
        self.media_ms = 0;
        // Recording time starts when the media is ready, not before: starting a real screen capture
        // and video encoder can take a moment, and that moment is not part of the recording.
        self.last_tick = self.clock.now_ms();
        self.state = State::Recording;
        self.emit_state();
        Ok(())
    }

    /// Make every file of the current take complete. Returns the duration and the number of media files.
    fn close_run(&mut self) -> Result<(u64, u32), SessionError> {
        let mut run = self.run.take().expect("a take is open");
        let media_ms = self.media_ms;
        let files = match run.media.take() {
            Some(media) => media.finish(media_ms)?,
            None => 0,
        };
        run.events.flush_sync()?;
        Ok((media_ms, files))
    }

    fn finish_current(&mut self) -> Result<(), SessionError> {
        let id = self.current_take();
        self.state = State::Stopping;
        self.emit_state();
        let (duration_ms, segments) = self.close_run()?;
        finish_take(&self.cfg.project, id, duration_ms, segments)?;
        self.state = State::Stopped;
        self.emit_state();
        self.outcome = Some(Outcome { ok: true, take: id, state: "complete".into(), duration_ms, segments });
        Ok(())
    }

    fn set_aside_current(&mut self) -> Result<(), SessionError> {
        let id = self.current_take();
        let (duration_ms, segments) = self.close_run()?;
        set_aside_take(&self.cfg.project, id, duration_ms, segments)?;
        self.emit(Event::TakeSetAside { take: id, next_take: self.next_take });
        Ok(())
    }

    fn handle(&mut self, input: Input) -> Result<(), SessionError> {
        let command = match input {
            Input::Invalid(message) => {
                self.warn("bad-command", &message);
                return Ok(());
            }
            Input::Closed => {
                if self.cfg.stop_when_input_closes {
                    return self.stop_requested();
                }
                return Ok(());
            }
            Input::Command(command) => command,
        };
        match command {
            Command::Pause => {
                if self.state == State::Recording {
                    self.flush_now()?;
                    self.state = State::Paused;
                    self.emit_state();
                } else {
                    self.warn("not-recording", "Pause only works while recording.");
                }
            }
            Command::Resume => {
                if self.state == State::Paused {
                    self.state = State::Recording;
                    self.emit_state();
                } else {
                    self.warn("not-paused", "Resume only works while paused.");
                }
            }
            Command::Stop => self.stop_requested()?,
            Command::Restart => match self.state {
                State::Recording | State::Paused => {
                    self.set_aside_current()?;
                    self.start_countdown_or_take()?;
                }
                State::Countdown => {
                    self.countdown_started = self.clock.now_ms();
                    self.emit_state();
                }
                _ => self.warn("not-recording", "Start again only works while recording."),
            },
            Command::Marker { label } => {
                if matches!(self.state, State::Recording | State::Paused) {
                    let label = label.unwrap_or_default();
                    let media_ms = self.media_ms;
                    if let Some(run) = self.run.as_mut() {
                        run.events.append(media_ms, LogEvent::Marker { label: label.clone() })?;
                    }
                    self.emit(Event::Marker { label, elapsed_ms: media_ms });
                } else {
                    self.warn("not-recording", "Markers can only be added while recording.");
                }
            }
        }
        Ok(())
    }

    fn stop_requested(&mut self) -> Result<(), SessionError> {
        match self.state {
            State::Recording | State::Paused => self.finish_current(),
            State::Countdown => {
                // Stopped before recording began: no take was made.
                self.state = State::Stopped;
                self.emit_state();
                self.outcome =
                    Some(Outcome { ok: true, take: self.last_take, state: "none".into(), duration_ms: 0, segments: 0 });
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn flush_now(&mut self) -> Result<(), SessionError> {
        let now = self.clock.now_ms();
        if let Some(run) = self.run.as_mut() {
            if let Some(media) = run.media.as_mut() {
                media.flush()?;
            }
            run.events.flush_sync()?;
            run.last_flush_wall = now;
        }
        Ok(())
    }

    fn write_recording(&mut self) -> Result<(), SessionError> {
        let media_ms = self.media_ms;
        let run = self.run.as_mut().expect("recording has a take");
        let media = run.media.as_mut().expect("recording has media");
        media.pump(media_ms)
    }

    fn step(&mut self, input: &mut dyn InputSource) -> Result<bool, SessionError> {
        // Handling a command can take a moment, so times recorded below can be later than `now`.
        // Every difference between times therefore uses `saturating_sub` and cannot go below zero.
        let now = self.clock.now_ms();
        let elapsed_since_tick = now.saturating_sub(self.last_tick);
        self.last_tick = now;

        for item in input.poll(now) {
            self.handle(item)?;
            if self.outcome.is_some() {
                return Ok(true);
            }
        }

        if !self.sink_alive {
            // Whoever was listening has gone. Finish the take properly instead of recording on.
            if matches!(self.state, State::Recording | State::Paused) {
                self.finish_current()?;
            } else {
                self.outcome =
                    Some(Outcome { ok: true, take: self.last_take, state: "none".into(), duration_ms: 0, segments: 0 });
            }
            return Ok(true);
        }

        match self.state {
            State::Countdown => {
                if now.saturating_sub(self.countdown_started) >= self.cfg.countdown_ms {
                    self.begin_take()?;
                } else if now.saturating_sub(self.last_state_emit) >= 1000 {
                    self.emit_state();
                }
            }
            State::Recording => {
                self.media_ms += elapsed_since_tick;
                if let Some(max) = self.cfg.max_duration_ms {
                    self.media_ms = self.media_ms.min(max);
                }
                self.write_recording()?;
                let flush_due = self
                    .run
                    .as_ref()
                    .map(|r| now.saturating_sub(r.last_flush_wall) >= self.cfg.flush_ms)
                    .unwrap_or(false);
                if flush_due {
                    self.flush_now()?;
                }
                if now.saturating_sub(self.last_level_emit) >= 500 {
                    self.last_level_emit = now;
                    let media_ms = self.media_ms;
                    let levels = self
                        .run
                        .as_ref()
                        .and_then(|r| r.media.as_ref())
                        .map(|m| m.levels(media_ms))
                        .unwrap_or_default();
                    for (source, peak) in levels {
                        self.emit(Event::Level {
                            source: source.to_string(),
                            peak,
                            text: describe_level(peak).to_string(),
                            elapsed_ms: media_ms,
                        });
                    }
                }
                if now.saturating_sub(self.last_state_emit) >= 1000 {
                    self.emit_state();
                }
                if let Some(max) = self.cfg.max_duration_ms {
                    if self.media_ms >= max {
                        self.finish_current()?;
                        return Ok(true);
                    }
                }
            }
            State::Paused if now.saturating_sub(self.last_state_emit) >= 1000 => self.emit_state(),
            _ => {}
        }

        self.clock.sleep_ms(self.cfg.tick_ms);
        Ok(false)
    }

    /// Something went wrong that cannot be worked around: say so, finish what can be finished.
    fn fail(&mut self, error: SessionError) {
        let message = error.to_string();
        let code = if matches!(error, SessionError::Media(_)) { "media-failed" } else { "write-failed" };
        self.emit(Event::Error { code: code.into(), message, fatal: true });
        let mut duration_ms = self.media_ms;
        let mut segments = 0;
        if self.run.is_some() {
            let id = self.current_take();
            if let Ok((d, s)) = self.close_run() {
                duration_ms = d;
                segments = s;
                let _ = finish_take(&self.cfg.project, id, d, s);
            }
        }
        self.state = State::Error;
        self.emit_state();
        self.outcome = Some(Outcome { ok: false, take: self.last_take, state: "error".into(), duration_ms, segments });
    }
}

/// Run a recording session to the end.
///
/// `factory` starts the media for every take, so starting again begins from the start. Any function
/// that builds a test source works as a factory.
pub fn run_session(
    cfg: &SessionConfig,
    factory: &dyn MediaFactory,
    input: &mut dyn InputSource,
    sink: &mut dyn EventSink,
    clock: &dyn Clock,
) -> Result<Outcome, SessionError> {
    if cfg.project.join("capture.json").is_file() {
        open_project(&cfg.project)?;
    } else {
        create_project(&cfg.project, &cfg.title, &cfg.language)?;
    }
    let now = clock.now_ms();
    let next_take = next_take_id(&cfg.project)?;
    let mut runner = Runner {
        cfg,
        factory,
        sink,
        clock,
        state: State::Idle,
        run: None,
        next_take,
        last_take: next_take - 1,
        media_ms: 0,
        countdown_started: now,
        last_tick: now,
        last_state_emit: now,
        last_level_emit: now,
        sink_alive: true,
        outcome: None,
    };
    if let Err(e) = runner.start_countdown_or_take() {
        runner.fail(e);
    }
    while runner.outcome.is_none() {
        match runner.step(input) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => runner.fail(e),
        }
    }
    Ok(runner.outcome.take().expect("the loop only ends with an outcome"))
}

/// Commands handed over in advance, each at a time after the session starts. For tests.
pub struct ScriptedInput {
    start_ms: Option<u64>,
    items: VecDeque<(u64, Input)>,
}

impl ScriptedInput {
    pub fn new(items: Vec<(u64, Input)>) -> Self {
        Self { start_ms: None, items: items.into() }
    }

    pub fn none() -> Self {
        Self::new(Vec::new())
    }
}

impl InputSource for ScriptedInput {
    fn poll(&mut self, now_ms: u64) -> Vec<Input> {
        let start = *self.start_ms.get_or_insert(now_ms);
        let mut out = Vec::new();
        while let Some((at, _)) = self.items.front() {
            if start + *at <= now_ms {
                out.push(self.items.pop_front().expect("front exists").1);
            } else {
                break;
            }
        }
        out
    }
}

/// An event sink that just remembers everything. For tests.
#[derive(Default)]
pub struct VecSink {
    pub events: Vec<Event>,
}

impl EventSink for VecSink {
    fn emit(&mut self, event: Event) -> bool {
        self.events.push(event);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::source::{Chunk, Source, SyntheticSource};
    use openstage_capture_project::{final_path, list_takes, read_log, read_records, read_take, take_dir, TakeState};
    use std::fs;

    fn synthetic() -> Box<dyn Source> {
        Box::new(SyntheticSource::new())
    }

    fn config(dir: &tempfile::TempDir) -> SessionConfig {
        let mut cfg = SessionConfig::new(dir.path().join("lesson.capture"));
        cfg.countdown_ms = 0;
        cfg.segment_ms = 1000;
        cfg
    }

    fn states(sink: &VecSink) -> Vec<State> {
        sink.events
            .iter()
            .filter_map(|e| if let Event::State { state, .. } = e { Some(*state) } else { None })
            .collect()
    }

    #[test]
    fn a_three_second_recording_is_split_into_three_complete_segments() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(3000);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert_eq!(out, Outcome { ok: true, take: 1, state: "complete".into(), duration_ms: 3000, segments: 3 });

        let take = read_take(&cfg.project, 1).unwrap();
        assert_eq!((take.state, take.duration_ms, take.segments), (TakeState::Complete, Some(3000), 3));

        let segs = take_dir(&cfg.project, 1).join("segments");
        let mut last_t = 0;
        let mut total = 0;
        for index in 1..=3 {
            for record in read_records(&final_path(&segs, index)).unwrap() {
                assert!(record.t_ms >= last_t, "time never goes backwards");
                last_t = record.t_ms;
                assert!(Chunk::from_record(record.t_ms, &record.payload).is_some());
                total += 1;
            }
        }
        assert!(total > 300, "about 3 s of video and sound, got {total} records");
        let seg_files = fs::read_dir(&segs).unwrap().count();
        assert_eq!(seg_files, 3, "no leftover .part files");

        let seen = states(&sink);
        assert_eq!(seen.first(), Some(&State::Recording));
        assert_eq!(seen.last(), Some(&State::Stopped));
        assert!(seen.contains(&State::Stopping));
    }

    fn state_takes(sink: &VecSink) -> Vec<(State, u32)> {
        sink.events
            .iter()
            .filter_map(|e| if let Event::State { state, take, .. } = e { Some((*state, *take)) } else { None })
            .collect()
    }

    #[test]
    fn every_state_names_the_take_it_belongs_to_including_after_stopping() {
        // Found on the first Windows run: "stopped" said take 2 after recording only take 1.
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 1000;
        cfg.max_duration_ms = Some(1000);
        let mut sink = VecSink::default();
        run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        let seen = state_takes(&sink);
        assert!(seen.len() >= 4, "countdown, recording, stopping, stopped: {seen:?}");
        assert!(seen.iter().all(|(_, take)| *take == 1), "all of this is take 1: {seen:?}");
        assert_eq!(seen.last(), Some(&(State::Stopped, 1)));

        // Start again: the second take is take 2 all the way through, and so is the final "stopped".
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input =
            ScriptedInput::new(vec![(1000, Input::Command(Command::Restart)), (2000, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        let seen = state_takes(&sink);
        assert_eq!(seen.first(), Some(&(State::Recording, 1)));
        assert_eq!(seen.last(), Some(&(State::Stopped, 2)));
        assert!(seen.contains(&(State::Recording, 2)));
    }

    #[test]
    fn pausing_stops_recording_time() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (2000, Input::Command(Command::Resume)),
            (3000, Input::Command(Command::Stop)),
        ]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert!(out.ok);
        assert!((1980..=2040).contains(&out.duration_ms), "recorded 2 s of 3 s, got {}", out.duration_ms);
        let seen = states(&sink);
        assert!(seen.contains(&State::Paused));
        assert_eq!(seen.last(), Some(&State::Stopped));
    }

    #[test]
    fn start_again_sets_the_take_aside_and_erases_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input =
            ScriptedInput::new(vec![(1500, Input::Command(Command::Restart)), (2500, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert_eq!((out.take, out.ok), (2, true));

        let first = read_take(&cfg.project, 1).unwrap();
        let second = read_take(&cfg.project, 2).unwrap();
        assert_eq!(first.state, TakeState::SetAside);
        assert!(first.set_aside_at.is_some());
        assert_eq!(second.state, TakeState::Complete);
        assert!((1480..=1540).contains(&first.duration_ms.unwrap()));
        assert!((980..=1040).contains(&second.duration_ms.unwrap()));

        // The set-aside take's media is all still there and readable.
        let segs = take_dir(&cfg.project, 1).join("segments");
        let kept: usize = (1..=first.segments).map(|i| read_records(&final_path(&segs, i)).unwrap().len()).sum();
        assert!(kept > 100, "the first take still holds its recording ({kept} records)");
        assert!(sink.events.contains(&Event::TakeSetAside { take: 1, next_take: 2 }));
        assert_eq!(list_takes(&cfg.project).unwrap().len(), 2);
    }

    #[test]
    fn stopping_during_the_countdown_makes_no_take() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 5000;
        let mut input = ScriptedInput::new(vec![(1000, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert_eq!((out.take, out.state.as_str(), out.duration_ms), (0, "none", 0));
        assert!(list_takes(&cfg.project).unwrap().is_empty());
        assert_eq!(states(&sink), vec![State::Countdown, State::Stopped]);
    }

    #[test]
    fn the_countdown_comes_before_recording_and_is_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 2000;
        cfg.max_duration_ms = Some(1000);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert_eq!(out.duration_ms, 1000);
        let seen = states(&sink);
        assert_eq!(seen[0], State::Countdown);
        // The countdown repeats its state about once a second, then recording begins.
        assert_eq!(seen.iter().find(|s| **s != State::Countdown), Some(&State::Recording));
        let countdowns = seen.iter().filter(|s| **s == State::Countdown).count();
        assert!((2..=3).contains(&countdowns), "a 2 second countdown reports 2 or 3 times, got {countdowns}");
    }

    #[test]
    fn a_command_that_cannot_be_understood_is_reported_and_the_recording_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![
            (500, Input::Invalid("not a valid message".into())),
            (600, Input::Command(Command::Resume)),
            (1000, Input::Command(Command::Stop)),
        ]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert!(out.ok);
        let warnings: Vec<_> = sink
            .events
            .iter()
            .filter_map(|e| if let Event::Warning { code, .. } = e { Some(code.as_str()) } else { None })
            .collect();
        assert_eq!(warnings, vec!["bad-command", "not-paused"]);
    }

    #[test]
    fn a_marker_is_logged_and_announced() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![
            (700, Input::Command(Command::Marker { label: Some("oops".into()) })),
            (1000, Input::Command(Command::Stop)),
        ]);
        let mut sink = VecSink::default();
        run_session(&cfg, &synthetic, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert!(sink.events.iter().any(|e| matches!(e, Event::Marker { label, .. } if label == "oops")));
        let log = read_log(&take_dir(&cfg.project, 1).join("events.jsonl")).unwrap();
        assert_eq!(log.len(), 1);
        assert!(matches!(&log[0].event, LogEvent::Marker { label } if label == "oops"));
    }

    #[test]
    fn levels_come_with_a_description_in_words() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(2000);
        let mut sink = VecSink::default();
        run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        let levels: Vec<_> = sink.events.iter().filter(|e| matches!(e, Event::Level { .. })).collect();
        assert!(levels.len() >= 4, "about two levels twice a second, got {}", levels.len());
        for e in levels {
            if let Event::Level { text, .. } = e {
                assert!(["silent", "quiet", "good", "loud", "too loud"].contains(&text.as_str()));
            }
        }
        assert_eq!(describe_level(0.0), "silent");
        assert_eq!(describe_level(0.5), "good");
        assert_eq!(describe_level(1.0), "too loud");
    }

    /// A sink that goes away after a few events, like OpenStage being closed mid-recording.
    struct VanishingSink {
        left: usize,
        events: Vec<Event>,
    }

    impl EventSink for VanishingSink {
        fn emit(&mut self, event: Event) -> bool {
            if self.left == 0 {
                return false;
            }
            self.left -= 1;
            self.events.push(event);
            true
        }
    }

    #[test]
    fn if_the_listener_goes_away_the_take_is_finished_properly() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut sink = VanishingSink { left: 3, events: Vec::new() };
        let out = run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(out.ok);
        let take = read_take(&cfg.project, 1).unwrap();
        assert_eq!(take.state, TakeState::Complete);
        let segs = take_dir(&cfg.project, 1).join("segments");
        assert!(fs::read_dir(segs).unwrap().flatten().all(|e| e.path().extension().unwrap() == "seg"));
    }

    #[test]
    fn closing_the_input_stops_gracefully_unless_told_to_ignore_it() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![(800, Input::Closed)]);
        let out = run_session(&cfg, &synthetic, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(out.ok && out.duration_ms >= 780);

        let dir2 = tempfile::tempdir().unwrap();
        let mut cfg2 = config(&dir2);
        cfg2.stop_when_input_closes = false;
        cfg2.max_duration_ms = Some(1500);
        let mut input2 = ScriptedInput::new(vec![(100, Input::Closed)]);
        let out2 = run_session(&cfg2, &synthetic, &mut input2, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(out2.duration_ms, 1500);
    }

    /// A clock that moves a little every time it is read, like a real one while the program is busy.
    struct TickingClock(std::cell::Cell<u64>);

    impl Clock for TickingClock {
        fn now_ms(&self) -> u64 {
            self.0.set(self.0.get() + 1);
            self.0.get()
        }

        fn sleep_ms(&self, ms: u64) {
            self.0.set(self.0.get() + ms);
        }
    }

    /// Regression: time recorded while handling a command is later than the time the step began
    /// with. Subtracting the two used to go below zero and crash a debug build.
    #[test]
    fn time_moving_while_a_command_is_handled_cannot_break_the_loop() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 300;
        let mut input = ScriptedInput::new(vec![
            (50, Input::Command(Command::Restart)),
            (500, Input::Command(Command::Pause)),
            (700, Input::Command(Command::Resume)),
            (900, Input::Command(Command::Restart)),
            (1700, Input::Command(Command::Stop)),
        ]);
        let out = run_session(&cfg, &synthetic, &mut input, &mut VecSink::default(), &TickingClock(Default::default()))
            .unwrap();
        assert!(out.ok);
    }

    #[test]
    fn a_second_session_in_the_same_project_adds_a_take() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.max_duration_ms = Some(500);
        for expected in [1, 2] {
            let out =
                run_session(&cfg, &synthetic, &mut ScriptedInput::none(), &mut VecSink::default(), &ManualClock::new())
                    .unwrap();
            assert_eq!(out.take, expected);
        }
    }
}
