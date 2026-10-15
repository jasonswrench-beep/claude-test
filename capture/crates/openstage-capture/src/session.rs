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
//! * Data is forced to disk at least every `flush_ms`, so a crash loses at most that much. (The notes file is written
//!   from a thread of its own, so a drive that is slow to confirm a save cannot hold this loop up; on such a drive a
//!   crash can lose more of the notes, never of the video or sound.)
//! * **Start again never erases anything.** The take is marked as set aside and a new one begins.
//! * Stop is graceful: every file is made complete before the result is reported.
//! * A write problem is reported as a fatal error and the take is finished as far as it can be.
//! * If the program that is listening goes away, the recording stops gracefully rather than
//!   running on with nobody watching.
//! * The sounds that tell the author what is happening (`cues`) are played only while recording time
//!   is not running: in the countdown, while paused, and after the stop. The start and resume sounds
//!   are waited for, so recording carries on only once they have ended, and none is ever recorded.

use crate::clock::Clock;
use crate::cues::{Cue, CuePlayer, NoCues, CUE_TAIL_MS};
use crate::media::{MediaFactory, TakeMedia};
use openstage_capture_project::{
    create_project, create_take_with, finish_take, next_take_id, open_project, set_aside_take, take_dir,
    EventLogThread, LogEvent, ProjectError,
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

/// Saving that takes this long is worth a warning.
const SLOW_SAVE: std::time::Duration = std::time::Duration::from_millis(100);

struct TakeRun {
    id: u32,
    media: Option<Box<dyn TakeMedia>>,
    events: EventLogThread,
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
    cues: &'a dyn CuePlayer,
    /// How many countdown ticks have sounded in the countdown that is running.
    ticks_played: u64,
    /// Whether the media note what the author does, which every state report while recording says.
    watching: bool,
}

impl<'a> Runner<'a> {
    /// Sound a cue and carry on at once.
    fn cue(&self, cue: Cue) {
        self.cues.play(cue);
    }

    /// Sound a cue and wait until it has ended, so what follows does not overlap it.
    fn cue_and_wait(&self, cue: Cue) {
        let ms = self.cues.play(cue);
        if ms > 0 {
            self.clock.sleep_ms(ms + CUE_TAIL_MS);
        }
    }

    /// Sound the countdown ticks that are due: one as the countdown starts and one more each second.
    fn countdown_ticks(&mut self) {
        let elapsed = self.clock.now_ms().saturating_sub(self.countdown_started);
        while self.ticks_played * 1000 < self.cfg.countdown_ms && self.ticks_played * 1000 <= elapsed {
            self.cue(Cue::Tick);
            self.ticks_played += 1;
        }
    }

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
        let watching = self.watching && self.state == State::Recording;
        self.emit(Event::State { state: self.state, elapsed_ms, take, watching });
        self.last_state_emit = now;
    }

    fn warn(&mut self, code: &str, message: &str) {
        self.emit(Event::Warning { code: code.to_string(), message: message.to_string() });
    }

    /// When the media will note what the author does, say so in words and with a sound, before anything is noted.
    /// This is never done while recording time runs, so the sound is not in the recording.
    fn announce_watching(&mut self) {
        if let Some(message) = self.factory.watching() {
            self.emit(Event::Check { check: "interactions-on".into(), ok: true, message });
            self.cue_and_wait(Cue::Watching);
        }
    }

    fn start_countdown_or_take(&mut self) -> Result<(), SessionError> {
        if self.cfg.countdown_ms > 0 {
            self.state = State::Countdown;
            self.countdown_started = self.clock.now_ms();
            self.ticks_played = 0;
            self.emit_state();
            self.announce_watching();
            self.countdown_ticks();
            // The countdown is time to get ready in. The time warming up takes is part of the countdown,
            // not added to it.
            self.factory.warm_up(self.cfg)?;
            Ok(())
        } else {
            self.announce_watching();
            self.begin_take()
        }
    }

    fn begin_take(&mut self) -> Result<(), SessionError> {
        // The start sound ends before recording begins. Whatever the recorder heard meanwhile is not kept.
        self.cue_and_wait(Cue::Start);
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
        let events = self.factory.open_event_log(&dir.join("events.jsonl"))?;
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

    /// Put what the media noted about what the author did into the take's log.
    fn write_log_lines(&mut self) -> Result<(), SessionError> {
        if let Some(run) = self.run.as_mut() {
            if let Some(media) = run.media.as_mut() {
                for (t, event) in media.log_lines() {
                    run.events.append(t, event)?;
                }
            }
        }
        Ok(())
    }

    /// Make every file of the current take complete. Returns the duration and the number of media files.
    fn close_run(&mut self) -> Result<(u64, u32), SessionError> {
        self.write_log_lines()?;
        let mut run = self.run.take().expect("a take is open");
        let media_ms = self.media_ms;
        let files = match run.media.take() {
            Some(media) => media.finish(media_ms)?,
            None => 0,
        };
        // The log is written from a thread of its own; this waits for it to put everything on the disk.
        let saves = run.events.finish()?;
        if saves.slow_saves > 0 {
            let message = format!(
                "The drive was slow to confirm {} of {} saves of this take's notes (the longest took {} ms). \
                 The recording did not wait for them. If the video still stutters, record onto a faster drive, \
                 such as an SSD.",
                saves.slow_saves, saves.saves, saves.longest_ms
            );
            self.warn("slow-disk", &message);
        }
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
        // Everything is closed, so the stop sound is not recorded.
        self.cue_and_wait(Cue::Stop);
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
                    if let Some(media) = self.run.as_mut().and_then(|r| r.media.as_mut()) {
                        media.paused();
                    }
                    self.state = State::Paused;
                    self.emit_state();
                    self.cue(Cue::Pause);
                } else {
                    self.warn("not-recording", "Pause only works while recording.");
                }
            }
            Command::Resume => {
                if self.state == State::Paused {
                    // Still paused while the resume sound plays, so it is not recorded.
                    self.cue_and_wait(Cue::Resume);
                    if let Some(media) = self.run.as_mut().and_then(|r| r.media.as_mut()) {
                        media.resumed();
                    }
                    // The time spent waiting for the sound is not recording time.
                    self.last_tick = self.clock.now_ms();
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
                    self.ticks_played = 0;
                    self.emit_state();
                    self.countdown_ticks();
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
                self.cue_and_wait(Cue::Stop);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn flush_now(&mut self) -> Result<(), SessionError> {
        let now = self.clock.now_ms();
        let started = std::time::Instant::now();
        if let Some(run) = self.run.as_mut() {
            if let Some(media) = run.media.as_mut() {
                media.flush()?;
            }
            run.events.flush_sync()?;
            run.last_flush_wall = now;
        }
        // Waiting for the disk holds up everything else in this loop. Say so when it takes long, so a slow
        // disk can be told apart from other causes of a stutter.
        let took = started.elapsed();
        if took >= SLOW_SAVE {
            let message = format!(
                "Saving to the disk took {} ms just now. A slow disk can make the video stutter.",
                took.as_millis()
            );
            self.warn("slow-save", &message);
        }
        Ok(())
    }

    fn write_recording(&mut self) -> Result<(), SessionError> {
        let media_ms = self.media_ms;
        let run = self.run.as_mut().expect("recording has a take");
        let media = run.media.as_mut().expect("recording has media");
        media.pump(media_ms)?;
        // Something the media wants the author to know now (a webcam that stopped): said as a warning, once.
        for (code, message) in media.notices() {
            self.warn(&code, &message);
        }
        self.write_log_lines()
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
                self.countdown_ticks();
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
        self.cue_and_wait(Cue::Error);
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
    run_session_with_cues(cfg, factory, input, sink, clock, &NoCues)
}

/// Like [`run_session`], and also makes the sounds that tell the author what the recorder is doing.
pub fn run_session_with_cues(
    cfg: &SessionConfig,
    factory: &dyn MediaFactory,
    input: &mut dyn InputSource,
    sink: &mut dyn EventSink,
    clock: &dyn Clock,
    cues: &dyn CuePlayer,
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
        cues,
        ticks_played: 0,
        watching: factory.watching().is_some(),
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
    fn the_media_is_told_when_recording_pauses_and_resumes() {
        use crate::media::{MediaFactory, TakeMedia};
        use std::cell::RefCell;
        use std::rc::Rc;

        struct Noting(Rc<RefCell<Vec<&'static str>>>);
        impl TakeMedia for Noting {
            fn pump(&mut self, _: u64) -> Result<(), SessionError> {
                let mut calls = self.0.borrow_mut();
                if calls.last() != Some(&"pump") {
                    calls.push("pump");
                }
                Ok(())
            }
            fn flush(&mut self) -> Result<(), SessionError> {
                Ok(())
            }
            fn levels(&self, _: u64) -> Vec<(&'static str, f32)> {
                Vec::new()
            }
            fn paused(&mut self) {
                self.0.borrow_mut().push("paused");
            }
            fn resumed(&mut self) {
                self.0.borrow_mut().push("resumed");
            }
            fn finish(self: Box<Self>, _: u64) -> Result<u32, SessionError> {
                Ok(1)
            }
        }
        struct Factory(Rc<RefCell<Vec<&'static str>>>);
        impl MediaFactory for Factory {
            fn source_id(&self) -> String {
                "noting".into()
            }
            fn begin(&self, _: &std::path::Path, _: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                Ok(Box::new(Noting(Rc::clone(&self.0))))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut input = ScriptedInput::new(vec![
            (1000, Input::Command(Command::Pause)),
            (2000, Input::Command(Command::Resume)),
            (3000, Input::Command(Command::Stop)),
        ]);
        let out =
            run_session(&cfg, &Factory(Rc::clone(&calls)), &mut input, &mut VecSink::default(), &ManualClock::new())
                .unwrap();
        assert!(out.ok);
        assert_eq!(
            *calls.borrow(),
            vec!["pump", "paused", "resumed", "pump"],
            "it is told about the pause once, and about the resume once, in order"
        );
    }

    #[test]
    fn something_the_media_wants_said_while_recording_goes_on_is_passed_on_as_a_warning_and_does_not_end_it() {
        use crate::media::{MediaFactory, TakeMedia};

        struct Chatty(u32);
        impl TakeMedia for Chatty {
            fn pump(&mut self, media_ms: u64) -> Result<(), SessionError> {
                self.0 = media_ms as u32;
                Ok(())
            }
            fn flush(&mut self) -> Result<(), SessionError> {
                Ok(())
            }
            fn levels(&self, _: u64) -> Vec<(&'static str, f32)> {
                Vec::new()
            }
            fn notices(&mut self) -> Vec<(String, String)> {
                // Only between 1.0 and 1.1 seconds, and then no more.
                if self.0 >= 1000 && self.0 < 1100 {
                    vec![("webcam-stopped".into(), "The webcam stopped.".into())]
                } else {
                    Vec::new()
                }
            }
            fn finish(self: Box<Self>, _: u64) -> Result<u32, SessionError> {
                Ok(1)
            }
        }
        struct Factory;
        impl MediaFactory for Factory {
            fn source_id(&self) -> String {
                "chatty".into()
            }
            fn begin(&self, _: &std::path::Path, _: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                Ok(Box::new(Chatty(0)))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![(3000, Input::Command(Command::Stop))]);
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &Factory, &mut input, &mut sink, &ManualClock::new()).unwrap();
        assert!(out.ok, "a notice does not end the recording");
        let said: Vec<_> = sink
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Warning { code, message } if code == "webcam-stopped" => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert!(!said.is_empty() && said.iter().all(|m| m == "The webcam stopped."), "{said:?}");
        assert!(
            said.len() <= 25,
            "said only while the media offered it, not on every look of the whole take: {}",
            said.len()
        );
    }

    /// A factory that notes when it is warmed up and when a take begins.
    struct Warm {
        notes: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
        fail_warm_up: bool,
    }
    impl crate::media::MediaFactory for Warm {
        fn source_id(&self) -> String {
            "warm".into()
        }
        fn warm_up(&self, _: &SessionConfig) -> Result<(), SessionError> {
            self.notes.borrow_mut().push("warm");
            if self.fail_warm_up {
                Err(SessionError::Media("The screen would not start.".into()))
            } else {
                Ok(())
            }
        }
        fn begin(
            &self,
            take_dir: &std::path::Path,
            cfg: &SessionConfig,
        ) -> Result<Box<dyn crate::media::TakeMedia>, SessionError> {
            self.notes.borrow_mut().push("begin");
            Ok(Box::new(crate::media::SegmentMedia::start(
                Box::new(SyntheticSource::new()),
                take_dir.join("segments"),
                cfg.segment_ms,
            )?))
        }
    }

    #[test]
    fn the_media_is_warmed_up_as_each_countdown_starts_and_not_when_there_is_none() {
        let notes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 1000;
        let factory = Warm { notes: std::rc::Rc::clone(&notes), fail_warm_up: false };
        // Record a take, start again (a second countdown), and stop.
        let mut input =
            ScriptedInput::new(vec![(2500, Input::Command(Command::Restart)), (6000, Input::Command(Command::Stop))]);
        let out = run_session(&cfg, &factory, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert!(out.ok && out.take == 2);
        assert_eq!(
            *notes.borrow(),
            vec!["warm", "begin", "warm", "begin"],
            "each countdown warms up first, then the take begins with what was prepared"
        );

        // Without a countdown there is nothing to warm up during.
        notes.borrow_mut().clear();
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(&dir);
        let mut input = ScriptedInput::new(vec![(500, Input::Command(Command::Stop))]);
        run_session(&cfg, &factory, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(*notes.borrow(), vec!["begin"]);
    }

    #[test]
    fn starting_the_countdown_again_does_not_warm_up_again() {
        let notes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 3000;
        let factory = Warm { notes: std::rc::Rc::clone(&notes), fail_warm_up: false };
        // "Start again" during the countdown only restarts the count; what was prepared is still ready.
        let mut input =
            ScriptedInput::new(vec![(1000, Input::Command(Command::Restart)), (6000, Input::Command(Command::Stop))]);
        run_session(&cfg, &factory, &mut input, &mut VecSink::default(), &ManualClock::new()).unwrap();
        assert_eq!(*notes.borrow(), vec!["warm", "begin"]);
    }

    #[test]
    fn a_failure_while_warming_up_ends_the_session_before_any_take_with_a_plain_message() {
        let notes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(&dir);
        cfg.countdown_ms = 3000;
        let factory = Warm { notes: std::rc::Rc::clone(&notes), fail_warm_up: true };
        let mut sink = VecSink::default();
        let out = run_session(&cfg, &factory, &mut ScriptedInput::none(), &mut sink, &ManualClock::new()).unwrap();
        assert!(!out.ok);
        assert_eq!(*notes.borrow(), vec!["warm"], "no take was started");
        assert!(sink.events.iter().any(|e| matches!(e, Event::Error { code, message, fatal: true }
            if code == "media-failed" && message.contains("The screen would not start."))));
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

    // ---- the sounds that tell the author what is happening ----

    mod cue_tests {
        use super::*;
        use crate::cues::{Cue, CuePlayer};
        use crate::media::{MediaFactory, TakeMedia};
        use std::cell::RefCell;
        use std::rc::Rc;

        /// Everything that happened, in order, with the time it happened at.
        #[derive(Clone)]
        struct Log {
            clock: Rc<ManualClock>,
            entries: Rc<RefCell<Vec<(String, u64)>>>,
        }
        impl Log {
            fn new() -> Self {
                Self { clock: Rc::new(ManualClock::new()), entries: Rc::new(RefCell::new(Vec::new())) }
            }
            fn note(&self, what: impl Into<String>) {
                self.entries.borrow_mut().push((what.into(), self.clock.now_ms()));
            }
            fn names(&self) -> Vec<String> {
                self.entries.borrow().iter().map(|e| e.0.clone()).collect()
            }
            fn index_of(&self, what: &str) -> usize {
                self.names().iter().position(|n| n == what).unwrap_or_else(|| panic!("no {what} in {:?}", self.names()))
            }
            fn time_of(&self, what: &str) -> u64 {
                self.entries.borrow()[self.index_of(what)].1
            }
        }

        /// Notes each change of state.
        struct StateSink(Log, Option<State>);
        impl EventSink for StateSink {
            fn emit(&mut self, event: Event) -> bool {
                if let Event::State { state, .. } = event {
                    if self.1 != Some(state) {
                        self.0.note(format!("state:{state:?}"));
                        self.1 = Some(state);
                    }
                }
                true
            }
        }

        /// Notes each change of state, and keeps every event.
        struct BothSink(StateSink, Vec<Event>);
        impl EventSink for BothSink {
            fn emit(&mut self, event: Event) -> bool {
                self.1.push(event.clone());
                self.0.emit(event)
            }
        }

        struct LogCues(Log, u64);
        impl CuePlayer for LogCues {
            fn play(&self, cue: Cue) -> u64 {
                self.0.note(format!("cue:{}", cue.name()));
                // A made-up length, so the waiting can be checked without depending on the real design.
                self.1
            }
        }

        struct LogMedia {
            log: Log,
            fail_after_pumps: Option<u32>,
            pumps: u32,
            /// What each call to `log_lines` hands over, in turn.
            lines: Vec<Vec<(u64, LogEvent)>>,
            asked: usize,
            /// Handed over only to a call made beyond one a pump: the one just before the take is finished.
            late: Vec<(u64, LogEvent)>,
        }
        impl TakeMedia for LogMedia {
            fn pump(&mut self, _: u64) -> Result<(), SessionError> {
                self.pumps += 1;
                if self.fail_after_pumps.is_some_and(|n| self.pumps > n) {
                    return Err(SessionError::Media("The disk stopped answering.".into()));
                }
                Ok(())
            }
            fn flush(&mut self) -> Result<(), SessionError> {
                Ok(())
            }
            fn levels(&self, _: u64) -> Vec<(&'static str, f32)> {
                Vec::new()
            }
            fn paused(&mut self) {
                self.log.note("media:paused");
            }
            fn resumed(&mut self) {
                self.log.note("media:resumed");
            }
            fn log_lines(&mut self) -> Vec<(u64, LogEvent)> {
                self.asked += 1;
                if self.asked > self.pumps as usize {
                    return self.late.clone();
                }
                self.lines.get(self.asked - 1).cloned().unwrap_or_default()
            }
            fn finish(self: Box<Self>, _: u64) -> Result<u32, SessionError> {
                self.log.note("media:finish");
                Ok(1)
            }
        }
        struct LogFactory {
            log: Log,
            fail_after_pumps: Option<u32>,
            watching: Option<String>,
            lines: Vec<Vec<(u64, LogEvent)>>,
            late: Vec<(u64, LogEvent)>,
        }
        impl MediaFactory for LogFactory {
            fn source_id(&self) -> String {
                "log".into()
            }
            fn watching(&self) -> Option<String> {
                self.watching.clone()
            }
            fn begin(&self, _: &std::path::Path, _: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                self.log.note("media:begin");
                Ok(Box::new(LogMedia {
                    log: self.log.clone(),
                    fail_after_pumps: self.fail_after_pumps,
                    pumps: 0,
                    lines: self.lines.clone(),
                    asked: 0,
                    late: self.late.clone(),
                }))
            }
        }

        const CUE_MS: u64 = 300;

        fn run(
            countdown_ms: u64,
            script: Vec<(u64, Command)>,
            cues_ms: Option<u64>,
            fail_after_pumps: Option<u32>,
        ) -> (Log, Outcome) {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = config(&dir);
            cfg.countdown_ms = countdown_ms;
            let log = Log::new();
            let mut input = ScriptedInput::new(script.into_iter().map(|(at, c)| (at, Input::Command(c))).collect());
            let factory =
                LogFactory { log: log.clone(), fail_after_pumps, watching: None, lines: Vec::new(), late: Vec::new() };
            let mut sink = StateSink(log.clone(), None);
            let outcome = match cues_ms {
                Some(ms) => {
                    run_session_with_cues(&cfg, &factory, &mut input, &mut sink, &*log.clock, &LogCues(log.clone(), ms))
                }
                None => run_session(&cfg, &factory, &mut input, &mut sink, &*log.clock),
            }
            .unwrap();
            (log, outcome)
        }

        /// The names of the cues that sounded, in order.
        fn cues_of(log: &Log) -> Vec<String> {
            log.names().into_iter().filter(|n| n.starts_with("cue:")).collect()
        }

        fn run_watching(
            countdown_ms: u64,
            script: Vec<(u64, Command)>,
            lines: Vec<Vec<(u64, LogEvent)>>,
        ) -> (Log, Outcome, Vec<Event>, tempfile::TempDir) {
            run_watching_late(countdown_ms, script, lines, Vec::new())
        }

        fn run_watching_late(
            countdown_ms: u64,
            script: Vec<(u64, Command)>,
            lines: Vec<Vec<(u64, LogEvent)>>,
            late: Vec<(u64, LogEvent)>,
        ) -> (Log, Outcome, Vec<Event>, tempfile::TempDir) {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = config(&dir);
            cfg.countdown_ms = countdown_ms;
            let log = Log::new();
            let mut input = ScriptedInput::new(script.into_iter().map(|(at, c)| (at, Input::Command(c))).collect());
            let factory = LogFactory {
                log: log.clone(),
                fail_after_pumps: None,
                watching: Some("Interaction recording is ON.".into()),
                lines,
                late,
            };
            let mut sink = BothSink(StateSink(log.clone(), None), Vec::new());
            let outcome = run_session_with_cues(
                &cfg,
                &factory,
                &mut input,
                &mut sink,
                &*log.clock,
                &LogCues(log.clone(), CUE_MS),
            )
            .unwrap();
            // The caller keeps the folder, and reads the log.
            (log, outcome, sink.1, dir)
        }

        #[test]
        fn when_what_the_author_does_is_noted_it_is_said_in_words_and_sounded_as_each_countdown_begins() {
            let (log, _, events, _) = run_watching(3000, vec![(8000, Command::Stop)], Vec::new());
            let names = log.names();
            assert_eq!(cues_of(&log).iter().filter(|c| *c == "cue:interactions").count(), 1);
            assert!(log.index_of("state:Countdown") < log.index_of("cue:interactions"));
            assert!(log.index_of("cue:interactions") < log.index_of("cue:start"), "{names:?}");
            assert!(log.index_of("cue:interactions") < log.index_of("media:begin"), "before anything is recorded");
            let said = events.iter().position(|e| {
                matches!(e, Event::Check { check, ok: true, message }
                    if check == "interactions-on" && message == "Interaction recording is ON.")
            });
            let first_recording = events.iter().position(|e| matches!(e, Event::State { state: State::Recording, .. }));
            assert!(said.is_some() && said < first_recording, "said in words before recording begins");
        }

        #[test]
        fn without_a_countdown_it_is_still_said_and_sounded_before_the_start() {
            let (log, _, events, _) = run_watching(0, vec![(2000, Command::Stop)], Vec::new());
            assert!(log.index_of("cue:interactions") < log.index_of("cue:start"));
            assert!(log.index_of("cue:start") < log.index_of("media:begin"));
            assert!(events.iter().any(|e| matches!(e, Event::Check { check, .. } if check == "interactions-on")));
        }

        #[test]
        fn starting_again_says_it_again_for_the_new_take() {
            let (log, _, events, _) =
                run_watching(2000, vec![(4000, Command::Restart), (9000, Command::Stop)], Vec::new());
            assert_eq!(cues_of(&log).iter().filter(|c| *c == "cue:interactions").count(), 2);
            let said = events.iter().filter(|e| matches!(e, Event::Check { check, .. } if check == "interactions-on"));
            assert_eq!(said.count(), 2);
        }

        #[test]
        fn nothing_is_said_or_sounded_when_nothing_is_noted() {
            let (log, _) = run(3000, vec![(6000, Command::Stop)], Some(CUE_MS), None);
            assert!(!cues_of(&log).contains(&"cue:interactions".to_string()));
        }

        #[test]
        fn the_sound_is_never_played_while_recording_time_is_running() {
            let (log, _, _, _) = run_watching(
                2000,
                vec![(4000, Command::Pause), (5000, Command::Resume), (7000, Command::Restart), (12000, Command::Stop)],
                Vec::new(),
            );
            let mut running = false;
            for name in log.names() {
                match name.as_str() {
                    "media:begin" | "media:resumed" => running = true,
                    "media:paused" | "media:finish" => running = false,
                    "cue:interactions" => assert!(!running, "the watching sound played during recording"),
                    _ => {}
                }
            }
        }

        #[test]
        fn every_state_report_while_recording_says_that_it_is_being_noted_and_no_other_does() {
            let (_, _, events, _) = run_watching(
                1000,
                vec![(3000, Command::Pause), (4000, Command::Resume), (6000, Command::Stop)],
                Vec::new(),
            );
            let mut recording_reports = 0;
            for event in &events {
                if let Event::State { state, watching, .. } = event {
                    assert_eq!(*watching, *state == State::Recording, "{state:?}");
                    recording_reports += usize::from(*state == State::Recording);
                }
            }
            assert!(recording_reports >= 3, "reported repeatedly, each time saying so: {recording_reports}");
        }

        #[test]
        fn what_the_media_noted_is_written_to_the_log_as_it_comes_and_once_more_before_the_take_is_finished() {
            let click = |t: u64| (t, LogEvent::Click { x: 1, y: 2, button: "left".into() });
            // The first two calls come during recording; the last line is only given to the call made just
            // before finishing.
            let lines = vec![vec![click(10), click(20)]];
            let (_, outcome, _, dir) = run_watching_late(0, vec![(1000, Command::Stop)], lines, vec![click(990)]);
            assert!(outcome.ok);
            let project = dir.path().join("lesson.capture");
            let log = read_log(&take_dir(&project, 1).join("events.jsonl")).unwrap();
            assert_eq!(log.iter().map(|l| l.t).collect::<Vec<_>>(), [10, 20, 990], "{log:?}");
        }

        #[test]
        fn a_whole_recording_sounds_each_cue_where_it_belongs() {
            let (log, _) = run(
                3000,
                vec![(5000, Command::Pause), (6000, Command::Resume), (8000, Command::Stop)],
                Some(CUE_MS),
                None,
            );
            assert_eq!(
                log.names(),
                [
                    "state:Countdown",
                    "cue:countdown",
                    "cue:countdown",
                    "cue:countdown",
                    "cue:start",
                    "media:begin",
                    "state:Recording",
                    "media:paused",
                    "state:Paused",
                    "cue:pause",
                    "cue:resume",
                    "media:resumed",
                    "state:Recording",
                    "state:Stopping",
                    "media:finish",
                    "state:Stopped",
                    "cue:stop",
                ]
            );
        }

        #[test]
        fn no_cue_ever_sounds_while_recording_time_is_running() {
            let (log, _) = run(
                2000,
                vec![(4000, Command::Pause), (5000, Command::Resume), (7000, Command::Restart), (12000, Command::Stop)],
                Some(CUE_MS),
                None,
            );
            // Recording time runs from the media beginning (or resuming) until it is paused or finished.
            let mut running = false;
            for name in log.names() {
                match name.as_str() {
                    "media:begin" | "media:resumed" => running = true,
                    "media:paused" | "media:finish" => running = false,
                    n if n.starts_with("cue:") => assert!(!running, "{n} sounded while recording: {:?}", log.names()),
                    _ => {}
                }
            }
            assert!(!running);
            assert!(cues_of(&log).len() >= 8, "{:?}", cues_of(&log));
        }

        #[test]
        fn recording_begins_only_after_the_start_sound_and_resumes_after_the_resume_sound() {
            let (log, _) = run(
                2000,
                vec![(4000, Command::Pause), (5000, Command::Resume), (6000, Command::Stop)],
                Some(CUE_MS),
                None,
            );
            assert!(log.time_of("media:begin") >= log.time_of("cue:start") + CUE_MS, "the start sound is waited for");
            assert!(
                log.time_of("media:resumed") >= log.time_of("cue:resume") + CUE_MS,
                "the resume sound is waited for"
            );
            assert!(log.time_of("cue:start") >= 2000, "the start sound comes when the countdown ends");
        }

        #[test]
        fn waiting_for_a_sound_is_not_recording_time() {
            let script = || vec![(1000, Command::Pause), (2000, Command::Resume), (4000, Command::Stop)];
            let (_, with) = run(0, script(), Some(CUE_MS), None);
            let (_, without) = run(0, script(), None, None);
            assert!(without.duration_ms.abs_diff(3000) <= 60, "1 s and 2 s of the 4 s: {}", without.duration_ms);
            // The script's clock starts once the start sound has ended, so only the wait for the resume
            // sound comes out of the recording. Nothing else may be added or lost.
            let wait = CUE_MS + 60;
            let expected = 3000 - wait;
            assert!(
                with.duration_ms.abs_diff(expected) <= 60,
                "recording time is what is left after the waits: {} against {expected}",
                with.duration_ms
            );
        }

        #[test]
        fn without_cues_nothing_is_played_and_nothing_is_waited_for() {
            let (log, _) = run(2000, vec![(3000, Command::Stop)], None, None);
            assert!(cues_of(&log).is_empty());
            assert!(log.time_of("media:begin") < 2100, "recording begins as the countdown ends");
        }

        #[test]
        fn stopping_during_the_countdown_sounds_the_stop_but_no_start() {
            let (log, outcome) = run(3000, vec![(1500, Command::Stop)], Some(CUE_MS), None);
            assert_eq!(outcome.state, "none");
            assert_eq!(cues_of(&log), ["cue:countdown", "cue:countdown", "cue:stop"]);
            assert!(!log.names().contains(&"media:begin".to_string()));
        }

        #[test]
        fn starting_the_countdown_again_starts_the_ticks_again() {
            let (log, _) = run(3000, vec![(1500, Command::Restart), (9000, Command::Stop)], Some(CUE_MS), None);
            let ticks = cues_of(&log).iter().filter(|c| *c == "cue:countdown").count();
            assert_eq!(ticks, 2 + 3, "two before the restart, three after: {:?}", cues_of(&log));
        }

        #[test]
        fn starting_again_while_recording_counts_down_only_once_the_old_take_is_closed() {
            let (log, _) = run(2000, vec![(4000, Command::Restart), (9000, Command::Stop)], Some(CUE_MS), None);
            let names = log.names();
            let finish = names.iter().position(|n| n == "media:finish").unwrap();
            let ticks_after: Vec<usize> = names
                .iter()
                .enumerate()
                .filter(|(i, n)| *i > finish && *n == "cue:countdown")
                .map(|(i, _)| i)
                .collect();
            assert!(!ticks_after.is_empty(), "{names:?}");
            assert!(names[..finish].iter().filter(|n| *n == "cue:countdown").count() == 2);
        }

        #[test]
        fn a_failure_sounds_the_error_once_the_files_are_closed() {
            let (log, outcome) = run(0, vec![], Some(CUE_MS), Some(3));
            assert!(!outcome.ok);
            let names = log.names();
            assert!(log.index_of("media:finish") < log.index_of("cue:error"), "{names:?}");
            assert!(log.index_of("state:Error") < log.index_of("cue:error"), "{names:?}");
            assert_eq!(cues_of(&log).iter().filter(|c| *c == "cue:error").count(), 1);
        }

        #[test]
        fn a_marker_makes_no_sound() {
            let (log, _) = run(
                0,
                vec![(1000, Command::Marker { label: Some("here".into()) }), (2000, Command::Stop)],
                Some(CUE_MS),
                None,
            );
            assert_eq!(cues_of(&log), ["cue:start", "cue:stop"]);
        }
    }

    #[test]
    fn saving_that_takes_long_is_said_so_the_disk_can_be_blamed_or_cleared() {
        use crate::media::{MediaFactory, TakeMedia};
        struct Slow(std::time::Duration);
        impl TakeMedia for Slow {
            fn pump(&mut self, _: u64) -> Result<(), SessionError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), SessionError> {
                std::thread::sleep(self.0);
                Ok(())
            }
            fn levels(&self, _: u64) -> Vec<(&'static str, f32)> {
                Vec::new()
            }
            fn finish(self: Box<Self>, _: u64) -> Result<u32, SessionError> {
                Ok(1)
            }
        }
        struct Factory(std::time::Duration);
        impl MediaFactory for Factory {
            fn source_id(&self) -> String {
                "slow".into()
            }
            fn begin(&self, _: &std::path::Path, _: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                Ok(Box::new(Slow(self.0)))
            }
        }
        for (wait_ms, warned) in [(150u64, true), (0, false)] {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = config(&dir);
            cfg.max_duration_ms = Some(1200);
            let mut sink = VecSink::default();
            run_session(
                &cfg,
                &Factory(std::time::Duration::from_millis(wait_ms)),
                &mut ScriptedInput::none(),
                &mut sink,
                &ManualClock::new(),
            )
            .unwrap();
            let slow = sink.events.iter().find_map(|e| match e {
                Event::Warning { code, message } if code == "slow-save" => Some(message.clone()),
                _ => None,
            });
            assert_eq!(slow.is_some(), warned, "{wait_ms} ms: {slow:?}");
            if let Some(message) = slow {
                assert!(message.contains("Saving to the disk took 1") && message.contains("slow disk"), "{message}");
            }
        }
    }

    #[test]
    fn a_drive_that_is_slow_to_confirm_the_notes_does_not_hold_up_the_loop() {
        use crate::media::{MediaFactory, TakeMedia};
        use openstage_capture_project::{EventLogThread, LogSink};
        struct Quiet;
        impl TakeMedia for Quiet {
            fn pump(&mut self, _: u64) -> Result<(), SessionError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), SessionError> {
                Ok(())
            }
            fn levels(&self, _: u64) -> Vec<(&'static str, f32)> {
                Vec::new()
            }
            fn finish(self: Box<Self>, _: u64) -> Result<u32, SessionError> {
                Ok(1)
            }
        }
        /// A log whose forced saves each take this long, like the owner's drive in the fourth long test.
        struct SlowDrive(std::time::Duration);
        impl LogSink for SlowDrive {
            fn append(&mut self, _: u64, _: LogEvent) -> io::Result<()> {
                Ok(())
            }
            fn flush_sync(&mut self) -> io::Result<()> {
                std::thread::sleep(self.0);
                Ok(())
            }
        }
        struct Factory(std::time::Duration);
        impl MediaFactory for Factory {
            fn source_id(&self) -> String {
                "slow-notes".into()
            }
            fn begin(&self, _: &std::path::Path, _: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
                Ok(Box::new(Quiet))
            }
            fn open_event_log(&self, _: &std::path::Path) -> io::Result<EventLogThread> {
                Ok(EventLogThread::start(SlowDrive(self.0)))
            }
        }
        let code_of = |sink: &VecSink, wanted: &str| {
            sink.events.iter().find_map(|e| match e {
                Event::Warning { code, message } if code == wanted => Some(message.clone()),
                _ => None,
            })
        };
        for (wait_ms, told) in [(300u64, true), (0, false)] {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = config(&dir);
            cfg.max_duration_ms = Some(2500);
            let mut sink = VecSink::default();
            run_session(
                &cfg,
                &Factory(std::time::Duration::from_millis(wait_ms)),
                &mut ScriptedInput::none(),
                &mut sink,
                &ManualClock::new(),
            )
            .unwrap();
            assert!(code_of(&sink, "slow-save").is_none(), "the loop itself never waited for the drive");
            let slow = code_of(&sink, "slow-disk");
            assert_eq!(slow.is_some(), told, "{wait_ms} ms: {slow:?}");
            if let Some(message) = slow {
                assert!(
                    message.contains("slow to confirm") && message.contains("did not wait") && message.contains("SSD"),
                    "{message}"
                );
            }
        }
    }
}
