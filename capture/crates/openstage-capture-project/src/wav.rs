// SPDX-License-Identifier: Apache-2.0
//! Sound files that are always playable, even after a crash.
//!
//! A take keeps the microphone and the computer's own sound as separate WAV files (16-bit, any
//! number of channels, usually 48,000 samples per second) beside the mixed sound inside the MP4.
//! The editor needs them separate, for example to remove an echo.
//!
//! A WAV file normally gets its length written into its header only when it is finished, so one cut
//! off by a crash would claim to be empty. [`WavWriter`] avoids that: every time it is synced (about
//! once a second) it writes the data, forces it to disk, and only then updates the length in the
//! header. So the header is never ahead of the data, and a crash costs at most the sound since the
//! last sync. [`repair_wav`] fixes a file whose header is behind or wrong.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const HEADER_LEN: u64 = 44;
/// A WAV file counts its length in 32 bits: a little under 4 GiB, which is about 6 hours of 48 kHz stereo.
const MAX_DATA_BYTES: u64 = u32::MAX as u64 - 64;

fn header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let block = channels as u32 * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // plain PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * block).to_le_bytes());
    h[32..34].copy_from_slice(&(block as u16).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// A sound file being written. Always playable up to the last [`WavWriter::sync`].
pub struct WavWriter {
    file: BufWriter<File>,
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
    frames: u64,
}

impl WavWriter {
    /// Start a new file (replacing any file of that name).
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> io::Result<Self> {
        let channels = channels.max(1);
        let mut file = File::create(path)?;
        file.write_all(&header(sample_rate, channels, 0))?;
        Ok(Self { file: BufWriter::new(file), path: path.to_path_buf(), sample_rate, channels, frames: 0 })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many frames (one sample for each channel) have been written, synced or not.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Add sound. `samples` holds whole frames, interleaved; a stray sample at the end is ignored.
    pub fn append(&mut self, samples: &[i16]) -> io::Result<()> {
        let whole = samples.len() / self.channels as usize * self.channels as usize;
        let new_bytes = whole as u64 * 2;
        if (self.frames * self.channels as u64 * 2) + new_bytes > MAX_DATA_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A WAV sound file cannot hold more than about 6 hours of sound.",
            ));
        }
        let mut bytes = Vec::with_capacity(whole * 2);
        for s in &samples[..whole] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.file.write_all(&bytes)?;
        self.frames += (whole / self.channels as usize) as u64;
        Ok(())
    }

    /// Force everything written so far to disk, then record its length in the header.
    pub fn sync(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.get_ref().sync_data()?;
        let data_bytes = (self.frames * self.channels as u64 * 2) as u32;
        let file = self.file.get_mut();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.sample_rate, self.channels, data_bytes))?;
        file.seek(SeekFrom::End(0))?;
        file.sync_data()
    }

    /// Complete the file. Returns the number of frames in it.
    pub fn finish(mut self) -> io::Result<u64> {
        self.sync()?;
        Ok(self.frames)
    }
}

/// What a sound file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    /// Whole frames actually present in the file (whatever the header claims).
    pub frames: u64,
    /// The loudest sample, from 0.0 (silence) to 1.0.
    pub peak: f32,
}

impl WavInfo {
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            0
        } else {
            self.frames * 1000 / self.sample_rate as u64
        }
    }
}

/// Where the sound starts and how it is laid out, from a file's first bytes.
struct Layout {
    sample_rate: u32,
    channels: u16,
    data_start: u64,
}

fn read_layout(file: &mut File) -> io::Result<Layout> {
    let bad = |what: &str| {
        io::Error::new(io::ErrorKind::InvalidData, format!("Not a sound file this program wrote: {what}."))
    };
    let mut head = [0u8; 44];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut head).map_err(|_| bad("it is shorter than a sound file header"))?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" || &head[12..16] != b"fmt " || &head[36..40] != b"data" {
        return Err(bad("the header is not the plain layout"));
    }
    let format = u16::from_le_bytes([head[20], head[21]]);
    let channels = u16::from_le_bytes([head[22], head[23]]);
    let sample_rate = u32::from_le_bytes([head[24], head[25], head[26], head[27]]);
    let bits = u16::from_le_bytes([head[34], head[35]]);
    if format != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
        return Err(bad("it is not 16-bit sound"));
    }
    Ok(Layout { sample_rate, channels, data_start: HEADER_LEN })
}

/// Read what a sound file holds. The number of frames is what is really in the file, so a header
/// left behind by a crash does not matter.
pub fn wav_info(path: &Path) -> io::Result<WavInfo> {
    let mut file = File::open(path)?;
    let layout = read_layout(&mut file)?;
    let len = file.metadata()?.len();
    let block = layout.channels as u64 * 2;
    let frames = len.saturating_sub(layout.data_start) / block;

    file.seek(SeekFrom::Start(layout.data_start))?;
    let mut reader = io::BufReader::new(file.take(frames * block));
    let mut buf = vec![0u8; 64 * 1024];
    let mut peak = 0u32;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for pair in buf[..n - n % 2].chunks_exact(2) {
            peak = peak.max((i16::from_le_bytes([pair[0], pair[1]]) as i32).unsigned_abs());
        }
    }
    Ok(WavInfo {
        sample_rate: layout.sample_rate,
        channels: layout.channels,
        frames,
        peak: (peak as f32 / 32768.0).min(1.0),
    })
}

/// Read a sound file a piece at a time (so a long recording never has to fit in memory) and hand each
/// piece to `each`, with the file's sample rate and channel count. Pieces hold whole frames, interleaved. Returns what the file holds, except `peak`,
/// which is worked out from the pieces as they go by.
pub fn read_wav_in_pieces(path: &Path, mut each: impl FnMut(u32, u16, &[i16])) -> io::Result<WavInfo> {
    let mut file = File::open(path)?;
    let layout = read_layout(&mut file)?;
    let len = file.metadata()?.len();
    let block = layout.channels as u64 * 2;
    let frames = len.saturating_sub(layout.data_start) / block;

    file.seek(SeekFrom::Start(layout.data_start))?;
    let mut reader = io::BufReader::new(file.take(frames * block));
    // About 64 KiB, rounded down to whole frames.
    let piece_bytes = (64 * 1024 / block as usize).max(1) * block as usize;
    let mut bytes = vec![0u8; piece_bytes];
    let mut samples: Vec<i16> = Vec::with_capacity(piece_bytes / 2);
    let mut peak = 0u32;
    loop {
        let mut filled = 0;
        while filled < piece_bytes {
            let n = reader.read(&mut bytes[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        let whole = filled - filled % block as usize;
        samples.clear();
        samples.extend(bytes[..whole].chunks_exact(2).map(|p| i16::from_le_bytes([p[0], p[1]])));
        peak = peak.max(samples.iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0));
        each(layout.sample_rate, layout.channels, &samples);
        if filled < piece_bytes {
            break;
        }
    }
    Ok(WavInfo {
        sample_rate: layout.sample_rate,
        channels: layout.channels,
        frames,
        peak: (peak as f32 / 32768.0).min(1.0),
    })
}

/// What a repair did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavRepair {
    pub frames: u64,
    /// Bytes cut from the end because they were less than a whole frame.
    pub bytes_cut: u64,
    /// Whether the header had to be corrected.
    pub header_fixed: bool,
}

/// Make a sound file consistent: cut a half-written frame off the end and set the header's
/// lengths to match what is really in the file. A file that is already fine is left byte for byte.
pub fn repair_wav(path: &Path) -> io::Result<WavRepair> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let layout = read_layout(&mut file)?;
    let len = file.metadata()?.len();
    let block = layout.channels as u64 * 2;
    let data = len.saturating_sub(layout.data_start);
    let frames = data / block;
    let bytes_cut = data - frames * block;
    if bytes_cut > 0 {
        file.set_len(layout.data_start + frames * block)?;
    }
    let wanted = header(layout.sample_rate, layout.channels, (frames * block) as u32);
    let mut current = [0u8; 44];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut current)?;
    let header_fixed = current != wanted;
    if header_fixed {
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&wanted)?;
    }
    file.sync_all()?;
    Ok(WavRepair { frames, bytes_cut, header_fixed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, channels: usize, value: i16) -> Vec<i16> {
        vec![value; frames * channels]
    }

    #[test]
    fn a_finished_file_holds_what_was_written_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.append(&tone(480, 2, 1000)).unwrap();
        w.append(&tone(480, 2, -16384)).unwrap();
        assert_eq!(w.frames(), 960);
        assert_eq!(w.finish().unwrap(), 960);

        let info = wav_info(&path).unwrap();
        assert_eq!((info.sample_rate, info.channels, info.frames), (48_000, 2, 960));
        assert_eq!(info.duration_ms(), 20);
        assert!((info.peak - 0.5).abs() < 0.001, "peak {}", info.peak);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 44 + 960 * 4);
    }

    #[test]
    fn the_header_tells_the_truth_after_every_sync_so_a_crash_leaves_a_playable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("system.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.append(&tone(48_000, 2, 5)).unwrap();
        w.sync().unwrap();
        w.append(&tone(100, 2, 5)).unwrap(); // written after the last sync
                                             // "Crash": the writer is never finished. Look at the file as it is on disk right now.
        let raw = std::fs::read(&path).unwrap();
        let declared = u32::from_le_bytes([raw[40], raw[41], raw[42], raw[43]]) as u64;
        assert_eq!(declared, 48_000 * 4, "the header counts what was synced, never more than exists");
        assert!(raw.len() as u64 >= 44 + declared, "the data the header counts is really there");
        // Player behaviour: read the header's length. Our own reader and repair use what is really there.
        let repaired = repair_wav(&path).unwrap();
        assert!(repaired.frames >= 48_000);
        drop(w);
    }

    #[test]
    fn repair_cuts_a_half_written_frame_and_fixes_a_header_left_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        // A file as a crash can leave it: header says empty, 1000 frames and 3 stray bytes follow.
        let mut raw = header(48_000, 2, 0).to_vec();
        for _ in 0..1000 {
            raw.extend_from_slice(&[1, 0, 2, 0]);
        }
        raw.extend_from_slice(&[9, 9, 9]);
        std::fs::write(&path, &raw).unwrap();

        let r = repair_wav(&path).unwrap();
        assert_eq!(r, WavRepair { frames: 1000, bytes_cut: 3, header_fixed: true });
        let info = wav_info(&path).unwrap();
        assert_eq!(info.frames, 1000);
        let fixed = std::fs::read(&path).unwrap();
        assert_eq!(fixed.len(), 44 + 4000);
        assert_eq!(u32::from_le_bytes([fixed[40], fixed[41], fixed[42], fixed[43]]), 4000);
        assert_eq!(u32::from_le_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]), 4036);
    }

    #[test]
    fn repairing_a_good_file_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let mut w = WavWriter::create(&path, 44_100, 1).unwrap();
        w.append(&tone(300, 1, 77)).unwrap();
        w.finish().unwrap();
        let before = std::fs::read(&path).unwrap();
        let r = repair_wav(&path).unwrap();
        assert_eq!(r, WavRepair { frames: 300, bytes_cut: 0, header_fixed: false });
        assert_eq!(std::fs::read(&path).unwrap(), before, "byte for byte the same");
    }

    #[test]
    fn a_stray_sample_that_is_not_a_whole_frame_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.append(&[1, 2, 3]).unwrap(); // one whole frame and a stray sample
        assert_eq!(w.finish().unwrap(), 1);
    }

    #[test]
    fn something_that_is_not_a_sound_file_is_refused_in_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello, this is not a sound file at all, not even close to one!!").unwrap();
        let e = wav_info(&path).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("Not a sound file"), "{e}");
        let short = dir.path().join("short.wav");
        std::fs::write(&short, b"RIFF").unwrap();
        assert!(wav_info(&short).unwrap_err().to_string().contains("shorter"));
    }

    #[test]
    fn silence_has_a_peak_of_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quiet.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.append(&tone(4800, 2, 0)).unwrap();
        w.finish().unwrap();
        assert_eq!(wav_info(&path).unwrap().peak, 0.0);
    }

    #[test]
    fn a_file_can_be_read_in_pieces_and_the_pieces_add_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        let samples: Vec<i16> = (0..2 * 100_003).map(|i| (i % 2000) as i16 - 1000).collect();
        w.append(&samples).unwrap();
        w.finish().unwrap();

        let mut got: Vec<i16> = Vec::new();
        let mut pieces = 0;
        let info = read_wav_in_pieces(&path, |rate, channels, piece| {
            assert_eq!((rate, channels), (48_000, 2));
            assert_eq!(piece.len() % 2, 0, "pieces hold whole frames");
            got.extend_from_slice(piece);
            pieces += 1;
        })
        .unwrap();
        assert!(pieces > 1, "a long file arrives in several pieces");
        assert_eq!(got, samples, "nothing lost, nothing repeated");
        assert_eq!(info.frames, 100_003);
        assert!((info.peak - 1000.0 / 32768.0).abs() < 0.001);
    }
}
