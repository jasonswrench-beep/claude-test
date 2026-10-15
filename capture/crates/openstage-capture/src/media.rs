// SPDX-License-Identifier: Apache-2.0
//! Where a take's picture and sound are stored.
//!
//! The session loop does not care how media reaches the disk. A [`MediaFactory`] starts the media
//! for one take and returns a [`TakeMedia`] that the loop feeds as recording time passes:
//!
//! * [`SegmentMedia`] is the Phase 1 method, used with the test source: small checksummed segment
//!   files, written by this program.
//! * On Windows, `openstage-capture-windows` provides a factory that records a real screen into one
//!   crash-safe fragmented MP4, written by Windows' own video tools.

use crate::session::{SessionConfig, SessionError};
use crate::source::Source;
use openstage_capture_project::{part_path, EventLogThread, LogEvent, MediaFile, SegmentWriter};
use std::fs;
use std::path::{Path, PathBuf};

/// The media of one take, while it is being recorded.
pub trait TakeMedia {
    /// Move whatever the source has produced, up to `media_ms` of recording time, into the files.
    /// `media_ms` does not advance while paused, so a pause never reaches the recording.
    fn pump(&mut self, media_ms: u64) -> Result<(), SessionError>;
    /// Push what has been written to the disk. Called at least every flush interval and before a pause.
    fn flush(&mut self) -> Result<(), SessionError>;
    /// Current sound levels, from 0.0 (silence) to 1.0, by name. Empty if nothing is being listened to.
    fn levels(&self, media_ms: u64) -> Vec<(&'static str, f32)>;
    /// Recording has just been paused. Sources that keep running (a microphone does) should stop
    /// keeping what they hear, because recording time stands still and that sound has no place in it.
    fn paused(&mut self) {}
    /// Recording is about to carry on after a pause. Anything heard during the pause must be thrown
    /// away, so that the sound and the picture carry on from the same moment.
    fn resumed(&mut self) {}
    /// Things the author should be told about now, as (code, message), such as a webcam that has stopped while the
    /// screen and the sound carry on. Asked for after every pump; each is said once. There is no sound for these: a
    /// sound played while recording time runs would be recorded (see `cues.rs`).
    fn notices(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// What the author did since this was last asked (the cursor, clicks, keys, windows), as (recording time in
    /// milliseconds, what), in the order it should be written to the take's log. Asked for after every pump and once
    /// more before the take is finished. Only media that note what the author does (story A6) have anything to say.
    fn log_lines(&mut self) -> Vec<(u64, LogEvent)> {
        Vec::new()
    }
    /// Make every file complete. Returns how many media files the take holds.
    fn finish(self: Box<Self>, media_ms: u64) -> Result<u32, SessionError>;
}

/// Starts the media for each take of a session.
pub trait MediaFactory {
    /// A short name for the source, written into `take.json`, such as `synthetic` or `screen:1`.
    fn source_id(&self) -> String;
    /// When the media will note what the author does (story A6), the sentence that says so. The session says it as
    /// each countdown begins, sounds a cue, and marks every state report while recording with `watching`, so the
    /// author is never unaware of it. `None` (the default) when nothing of the kind is noted.
    fn watching(&self) -> Option<String> {
        None
    }
    /// The media files each take will hold, named relative to the take's folder. Empty for the test
    /// source, whose media lives in `segments/`.
    fn media_files(&self) -> Vec<MediaFile> {
        Vec::new()
    }
    /// The most recording time a crash can cost with this kind of media, which `take.json` records.
    fn declared_flush_ms(&self, cfg: &SessionConfig) -> u64 {
        cfg.flush_ms
    }
    /// Get ready while the countdown runs: start whatever takes a moment and does not need the take's
    /// folder (a screen capture, a microphone, a video encoder), so that recording can begin the instant
    /// the countdown ends. Called once as each countdown starts; anything it prepares is used by the next
    /// [`MediaFactory::begin`]. A failure here ends the session before the countdown finishes.
    fn warm_up(&self, _cfg: &SessionConfig) -> Result<(), SessionError> {
        Ok(())
    }
    /// Start the media for one take, in that take's folder (which already exists).
    fn begin(&self, take_dir: &Path, cfg: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError>;
    /// Open the take's interaction log. It is written from a thread of its own, so a slow drive cannot
    /// hold up the recording loop; tests replace this to give the log a drive that is slow or fails.
    fn open_event_log(&self, path: &Path) -> std::io::Result<EventLogThread> {
        EventLogThread::open(path)
    }
}

/// Any function that builds a test [`Source`] is a factory for segment media.
impl<F> MediaFactory for F
where
    F: Fn() -> Box<dyn Source>,
{
    fn source_id(&self) -> String {
        (self)().id().to_string()
    }

    fn begin(&self, take_dir: &Path, cfg: &SessionConfig) -> Result<Box<dyn TakeMedia>, SessionError> {
        Ok(Box::new(SegmentMedia::start((self)(), take_dir.join("segments"), cfg.segment_ms)?))
    }
}

/// Media stored as checksummed segment files (see `docs/capture-project-format.md`).
pub struct SegmentMedia {
    source: Box<dyn Source>,
    segments_dir: PathBuf,
    segment_ms: u64,
    writer: Option<SegmentWriter>,
    seg_index: u32,
    seg_start_ms: u64,
    finished_segments: u32,
}

impl SegmentMedia {
    pub fn start(source: Box<dyn Source>, segments_dir: PathBuf, segment_ms: u64) -> Result<Self, SessionError> {
        let writer = SegmentWriter::create(&segments_dir, 1)?;
        Ok(Self {
            source,
            segments_dir,
            segment_ms,
            writer: Some(writer),
            seg_index: 1,
            seg_start_ms: 0,
            finished_segments: 0,
        })
    }
}

impl TakeMedia for SegmentMedia {
    fn pump(&mut self, media_ms: u64) -> Result<(), SessionError> {
        let chunks = self.source.poll(media_ms);
        let writer = self.writer.as_mut().expect("recording has a segment");
        for chunk in &chunks {
            writer.append(chunk.t_ms, &chunk.to_record_payload())?;
        }
        if media_ms.saturating_sub(self.seg_start_ms) >= self.segment_ms {
            if let Some(full) = self.writer.take() {
                full.finish()?;
                self.finished_segments += 1;
            }
            self.seg_index += 1;
            self.writer = Some(SegmentWriter::create(&self.segments_dir, self.seg_index)?);
            self.seg_start_ms = media_ms;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SessionError> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush_sync()?;
        }
        Ok(())
    }

    fn levels(&self, media_ms: u64) -> Vec<(&'static str, f32)> {
        self.source.levels(media_ms)
    }

    fn finish(mut self: Box<Self>, _media_ms: u64) -> Result<u32, SessionError> {
        if let Some(writer) = self.writer.take() {
            if writer.records() == 0 {
                // Nothing was written to this piece, so there is nothing to keep.
                drop(writer);
                fs::remove_file(part_path(&self.segments_dir, self.seg_index))?;
            } else {
                writer.finish()?;
                self.finished_segments += 1;
            }
        }
        Ok(self.finished_segments)
    }
}
