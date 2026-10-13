// SPDX-License-Identifier: Apache-2.0
//! Where recorded data comes from.

/// What kind of data a chunk holds. The number is the first byte of a segment record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Video = 1,
    Mic = 2,
    SystemAudio = 3,
}

impl ChunkKind {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Video),
            2 => Some(Self::Mic),
            3 => Some(Self::SystemAudio),
            _ => None,
        }
    }
}

/// A piece of picture or sound with the recording time it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub kind: ChunkKind,
    pub t_ms: u64,
    pub payload: Vec<u8>,
}

impl Chunk {
    /// The bytes stored in a segment record: the kind, then the data.
    pub fn to_record_payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.payload.len() + 1);
        out.push(self.kind as u8);
        out.extend_from_slice(&self.payload);
        out
    }

    /// The reverse of [`Chunk::to_record_payload`].
    pub fn from_record(t_ms: u64, record_payload: &[u8]) -> Option<Self> {
        let (&first, rest) = record_payload.split_first()?;
        Some(Self { kind: ChunkKind::from_byte(first)?, t_ms, payload: rest.to_vec() })
    }
}

/// Something that produces recorded data.
pub trait Source {
    /// A short name such as `synthetic` or (later) `windows`.
    fn id(&self) -> &'static str;
    /// Everything that should exist up to this recording time and has not been returned yet.
    fn poll(&mut self, media_ms: u64) -> Vec<Chunk>;
    /// Current sound levels from 0.0 (silence) to 1.0 (loudest), by name.
    fn levels(&self, media_ms: u64) -> Vec<(&'static str, f32)>;
}

/// A fake source for tests: 30 video frames and 50 sound chunks per second, with a sound level
/// that rises and falls.
pub struct SyntheticSource {
    next_frame: u64,
    next_audio: u64,
}

impl SyntheticSource {
    const FRAMES_PER_SECOND: u64 = 30;
    const AUDIO_EVERY_MS: u64 = 20;

    pub fn new() -> Self {
        Self { next_frame: 0, next_audio: 0 }
    }

    fn mic_peak(media_ms: u64) -> f32 {
        let phase = (media_ms as f32 / 4000.0) * std::f32::consts::TAU;
        0.5 + 0.4 * phase.sin()
    }
}

impl Default for SyntheticSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for SyntheticSource {
    fn id(&self) -> &'static str {
        "synthetic"
    }

    fn poll(&mut self, media_ms: u64) -> Vec<Chunk> {
        let mut out = Vec::new();
        loop {
            let frame_t = self.next_frame * 1000 / Self::FRAMES_PER_SECOND;
            let audio_t = self.next_audio * Self::AUDIO_EVERY_MS;
            if frame_t > media_ms && audio_t > media_ms {
                break;
            }
            if frame_t <= audio_t && frame_t <= media_ms {
                let mut payload = self.next_frame.to_le_bytes().to_vec();
                payload.extend_from_slice(&[(self.next_frame % 251) as u8; 24]);
                out.push(Chunk { kind: ChunkKind::Video, t_ms: frame_t, payload });
                self.next_frame += 1;
            } else if audio_t <= media_ms {
                let mut mic = Self::mic_peak(audio_t).to_le_bytes().to_vec();
                mic.extend_from_slice(&[0u8; 12]);
                out.push(Chunk { kind: ChunkKind::Mic, t_ms: audio_t, payload: mic });
                let mut system = 0.3f32.to_le_bytes().to_vec();
                system.extend_from_slice(&[0u8; 12]);
                out.push(Chunk { kind: ChunkKind::SystemAudio, t_ms: audio_t, payload: system });
                self.next_audio += 1;
            } else {
                break;
            }
        }
        out
    }

    fn levels(&self, media_ms: u64) -> Vec<(&'static str, f32)> {
        vec![("mic", Self::mic_peak(media_ms)), ("system", 0.3)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_second_makes_thirty_frames_and_fifty_sound_chunks_each() {
        let mut source = SyntheticSource::new();
        let chunks = source.poll(999);
        let count = |kind| chunks.iter().filter(|c| c.kind == kind).count();
        assert_eq!(count(ChunkKind::Video), 30);
        assert_eq!(count(ChunkKind::Mic), 50);
        assert_eq!(count(ChunkKind::SystemAudio), 50);
    }

    #[test]
    fn chunks_come_out_in_time_order_and_never_twice() {
        let mut source = SyntheticSource::new();
        let mut all = source.poll(500);
        all.extend(source.poll(500));
        all.extend(source.poll(1500));
        assert!(all.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));
        let frames: Vec<_> = all.iter().filter(|c| c.kind == ChunkKind::Video).collect();
        assert_eq!(frames.len(), 46, "frames at 0, 33, ..., 1500 ms");
    }

    #[test]
    fn a_chunk_survives_being_stored_in_a_record() {
        let chunk = Chunk { kind: ChunkKind::Mic, t_ms: 40, payload: vec![9, 8, 7] };
        let stored = chunk.to_record_payload();
        assert_eq!(Chunk::from_record(40, &stored), Some(chunk));
        assert_eq!(Chunk::from_record(0, &[]), None);
        assert_eq!(Chunk::from_record(0, &[99, 1]), None);
    }

    #[test]
    fn the_mic_level_stays_between_silence_and_full() {
        let source = SyntheticSource::new();
        for t in (0..8000).step_by(100) {
            let (_, peak) = source.levels(t)[0];
            assert!((0.0..=1.0).contains(&peak));
        }
    }
}
