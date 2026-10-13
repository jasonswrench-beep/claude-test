// SPDX-License-Identifier: Apache-2.0
//! The capture project folder.
//!
//! A recording lives in its own folder (named `something.capture`) beside the
//! course, never inside it. The folder holds a small manifest (`capture.json`)
//! and one folder per *take* under `takes/`. A take holds its media either as
//! one crash-safe fragmented MP4 written by Windows (`screen.mp4`, listed in
//! `take.json`) plus separate WAV sound files for the microphone and the computer's own
//! sound, or, for the test source, in small checksummed pieces called
//! segments, plus an append-only log of interactions.
//!
//! Everything here is built so that a crash costs seconds, not the take:
//!
//! * a segment is written as `seg-000001.part` and renamed to `.seg` only when
//!   it is complete, and every record carries a checksum;
//! * the state of a take lives in exactly one place, its own `take.json`, and
//!   the manifest never lists takes, so the two cannot disagree after a crash;
//! * nothing is ever erased: a take the author starts over is only marked as
//!   set aside.
//!
//! The format is described in `docs/capture-project-format.md`.

mod error;
mod events_log;
pub mod fmp4;
mod fsutil;
mod manifest;
mod recover;
mod segment;
mod take;
pub mod wav;

pub use error::ProjectError;
pub use events_log::{read_log, trim_incomplete_tail, EventLogWriter, LogEvent, LogLine};
pub use fsutil::{new_id, utc_now_iso, write_atomic};
pub use manifest::{create_project, open_project, Manifest, Tracks, FORMAT_NAME, SCHEMA_VERSION};
pub use recover::{recover_project, RecoverReport, RecoveredTake};
pub use segment::{
    final_path, part_path, read_records, salvage_part, scan, Record, Salvage, SegmentInfo, SegmentWriter,
};
pub use take::{
    create_take, create_take_with, finish_take, list_takes, next_take_id, read_take, set_aside_take, take_dir,
    write_take, MediaFile, MixInfo, RegionInfo, Take, TakeEntry, TakeState,
};
pub use wav::{read_wav_in_pieces, repair_wav, wav_info, WavInfo, WavRepair, WavWriter};
