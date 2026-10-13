// SPDX-License-Identifier: Apache-2.0
//! The recording engine.
//!
//! * [`source`]: where sound and picture come from. Phase 1 has one fake source for testing; the
//!   Windows sources arrive in Phase 2.
//! * [`media`]: where a take's media is stored (segments for the test source; a crash-safe MP4 on Windows).
//! * [`session`]: the recording state machine (countdown, record, pause, resume, stop, start
//!   again), which writes crash-safe takes and reports everything as protocol events.
//! * [`clock`]: time, which tests can fake so a three-second recording takes no real time.

pub mod clock;
pub mod cues;
pub mod media;
pub mod session;
pub mod source;

pub use clock::{Clock, ManualClock, SystemClock};
pub use cues::{Cue, CuePlayer, NoCues, DEFAULT_VOLUME as DEFAULT_CUE_VOLUME};
pub use media::{MediaFactory, SegmentMedia, TakeMedia};
pub use session::{
    describe_level, run_session, run_session_with_cues, EventSink, Input, InputSource, Outcome, ScriptedInput,
    SessionConfig, SessionError, VecSink,
};
pub use source::{Chunk, ChunkKind, Source, SyntheticSource};
