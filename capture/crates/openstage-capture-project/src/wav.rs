// SPDX-License-Identifier: Apache-2.0
//! Sound files that are always playable, even after a crash.
//!
//! A take keeps the microphone and the computer's own sound as separate WAV files (16-bit, any
//! number of channels, usually 48,000 samples per second) beside the mixed sound inside the MP4.
//! The editor needs them separate, for example to remove an echo.
//!
//! A WAV file normally gets its length written into its header only when it is finished, so one cut
//! off by a crash would claim to be empty. [`WavWriter`] avoids that: every time it is synced (about
//! once a second) it writes the data and only then updates the length in the header. So the header is
//! never ahead of the data, and a program that is killed costs at most the sound since the last sync.
//! Now and then it also waits for the disk, which a power cut needs. [`repair_wav`] fixes a file whose
//! header is behind or wrong.

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

    /// Hand everything written so far to the operating system, then record its length in the header.
    /// After this the file is playable up to here even if the program is killed. It is quick: it does not
    /// wait for the disk (see [`WavWriter::sync_to_disk`]).
    pub fn sync(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let data_bytes = (self.frames * self.channels as u64 * 2) as u32;
        let file = self.file.get_mut();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.sample_rate, self.channels, data_bytes))?;
        file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    /// Like [`WavWriter::sync`], and then wait until the disk itself has the sound. That can take a
    /// noticeable moment on a busy or slow disk, so it is done now and then, not every second. Only a
    /// power cut or a crash of the whole computer needs it: a program that is killed leaves what it
    /// handed to the operating system.
    pub fn sync_to_disk(&mut self) -> io::Result<()> {
        self.sync()?;
        self.file.get_ref().sync_data()
    }

    /// Complete the file, on the disk. Returns the number of frames in it.
    pub fn finish(mut self) -> io::Result<u64> {
        self.sync_to_disk()?;
        Ok(self.frames)
    }
}

/// What the writer thread is asked to do, in order.
enum Job {
    Append(Vec<i16>),
    /// Record the length in the header (quick).
    Sync,
    /// The same, and wait until the disk itself has the sound.
    SyncToDisk,
}

/// A sound file written by a thread of its own.
///
/// Waiting on the disk must never hold up the recording. On a real computer a single write to one of
/// these files waited two seconds, behind the disk being made to take the same file's earlier sound,
/// and the recorder's main loop stood still with it: it lost two seconds of the computer's sound and
/// the picture stuttered. So the recorder never touches the file: it hands the sound to this thread
/// and carries on, and every wait for the disk happens here, in order, while the sound queues up
/// (a few hundred kilobytes a second, however long the wait).
///
/// A write that fails is remembered, and the next call says so.
pub struct WavThread {
    tx: Option<std::sync::mpsc::Sender<Job>>,
    thread: Option<std::thread::JoinHandle<io::Result<u64>>>,
    failure: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl WavThread {
    /// Take over a sound file that is being written.
    pub fn start(mut writer: WavWriter) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let failure = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let for_thread = std::sync::Arc::clone(&failure);
        let thread = std::thread::spawn(move || {
            for job in rx {
                if for_thread.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                    continue; // after a failure, do nothing more with the file; the first failure is the one reported
                }
                let result = match job {
                    Job::Append(samples) => writer.append(&samples),
                    Job::Sync => writer.sync(),
                    Job::SyncToDisk => writer.sync_to_disk(),
                };
                if let Err(e) = result {
                    *for_thread.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.to_string());
                }
            }
            // Whether the program finished or went away, complete the file with what it was given.
            writer.finish()
        });
        Self { tx: Some(tx), thread: Some(thread), failure }
    }

    fn check(&self) -> io::Result<()> {
        match self.failure.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(why) => Err(io::Error::other(why.clone())),
            None => Ok(()),
        }
    }

    fn send(&self, job: Job) -> io::Result<()> {
        self.check()?;
        self.tx
            .as_ref()
            .ok_or_else(|| io::Error::other("The sound file was already closed."))?
            .send(job)
            .map_err(|_| io::Error::other("The thread that writes the sound file has stopped."))
    }

    /// Queue sound to be written. Returns at once.
    pub fn append(&self, samples: Vec<i16>) -> io::Result<()> {
        self.send(Job::Append(samples))
    }

    /// Queue a record of the length so far in the file's header. Returns at once.
    pub fn sync(&self) -> io::Result<()> {
        self.send(Job::Sync)
    }

    /// Queue the same and a wait for the disk itself. Returns at once; the wait is the thread's.
    pub fn sync_to_disk(&self) -> io::Result<()> {
        self.send(Job::SyncToDisk)
    }

    /// Write everything that is queued, complete the file on the disk, and say how many frames it holds.
    pub fn finish(mut self) -> io::Result<u64> {
        drop(self.tx.take());
        let result = match self.thread.take() {
            Some(thread) => {
                thread.join().map_err(|_| io::Error::other("The thread that writes the sound file failed."))?
            }
            None => Ok(0),
        };
        self.check()?;
        result
    }
}

impl Drop for WavThread {
    fn drop(&mut self) {
        // If the recording was abandoned, still let the thread complete the file.
        drop(self.tx.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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

/// A sound file open for reading any stretch of it, in order or not (the exporter mixes sound this way). It reads
/// a second at a time and keeps what it read, so asking for the next few hundredths of a second again and again is
/// not a read of the disk each time. Two channels are read as they are; one channel is made into two.
pub struct WavFile {
    file: File,
    sample_rate: u32,
    channels: u16,
    data_start: u64,
    frames: u64,
    /// The frames kept: the first one's number, and the frames themselves (always two channels).
    kept_from: u64,
    kept: Vec<i16>,
}

impl WavFile {
    /// How many frames are read from the disk at a time.
    const CHUNK: u64 = 48_000;

    pub fn open(path: &Path) -> io::Result<WavFile> {
        let mut file = File::open(path)?;
        let layout = read_layout(&mut file)?;
        if layout.channels > 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The sound file has {} channels; this program reads one or two.", layout.channels),
            ));
        }
        let len = file.metadata()?.len();
        let block = layout.channels as u64 * 2;
        let frames = len.saturating_sub(layout.data_start) / block;
        Ok(WavFile {
            file,
            sample_rate: layout.sample_rate,
            channels: layout.channels,
            data_start: layout.data_start,
            frames,
            kept_from: 0,
            kept: Vec::new(),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whole frames really in the file.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Up to `count` frames from frame `first`, two channels interleaved. Fewer, or none, at the end of the file.
    pub fn read(&mut self, first: u64, count: usize) -> io::Result<Vec<i16>> {
        let end = (first + count as u64).min(self.frames);
        let mut out = Vec::with_capacity(count.min(self.frames.saturating_sub(first) as usize) * 2);
        let mut at = first;
        while at < end {
            let kept_frames = self.kept.len() as u64 / 2;
            if at < self.kept_from || at >= self.kept_from + kept_frames {
                self.fill(at)?;
            }
            let from = (at - self.kept_from) as usize;
            let take = ((end - at) as usize).min(self.kept.len() / 2 - from);
            out.extend_from_slice(&self.kept[from * 2..(from + take) * 2]);
            at += take as u64;
        }
        Ok(out)
    }

    fn fill(&mut self, from: u64) -> io::Result<()> {
        let count = Self::CHUNK.min(self.frames - from);
        let block = self.channels as u64 * 2;
        self.file.seek(SeekFrom::Start(self.data_start + from * block))?;
        let mut bytes = vec![0u8; (count * block) as usize];
        self.file.read_exact(&mut bytes)?;
        self.kept.clear();
        for frame in bytes.chunks_exact(block as usize) {
            let left = i16::from_le_bytes([frame[0], frame[1]]);
            let right = if self.channels == 2 { i16::from_le_bytes([frame[2], frame[3]]) } else { left };
            self.kept.push(left);
            self.kept.push(right);
        }
        self.kept_from = from;
        Ok(())
    }
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
    fn any_stretch_of_a_sound_file_can_be_read_in_any_order_across_the_pieces_it_is_read_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        // 3 seconds in which frame n holds the values (n, -n) as far as they fit.
        let samples: Vec<i16> = (0..144_000i32).flat_map(|n| [(n % 30_000) as i16, -((n % 30_000) as i16)]).collect();
        w.append(&samples).unwrap();
        w.finish().unwrap();

        let mut f = WavFile::open(&path).unwrap();
        assert_eq!((f.sample_rate(), f.frames()), (48_000, 144_000));
        // A stretch inside one piece, one across the edge of a piece, one back before the first, and one at the end.
        for (first, count) in [(100u64, 50usize), (47_990, 40), (5, 3), (143_990, 100)] {
            let got = f.read(first, count).unwrap();
            let want_frames = count.min((144_000 - first) as usize);
            assert_eq!(got.len(), want_frames * 2, "{first}+{count}");
            for (i, pair) in got.chunks(2).enumerate() {
                let n = (first as i32 + i as i32) % 30_000;
                assert_eq!((pair[0] as i32, pair[1] as i32), (n, -n), "frame {} of the read at {first}", i);
            }
        }
        assert!(f.read(144_000, 10).unwrap().is_empty(), "nothing past the end");
        assert!(f.read(999_999, 10).unwrap().is_empty());
    }

    #[test]
    fn a_single_channel_file_is_read_as_two_and_more_than_two_channels_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mono.wav");
        let mut w = WavWriter::create(&path, 44_100, 1).unwrap();
        w.append(&[7, 8, 9]).unwrap();
        w.finish().unwrap();
        let mut f = WavFile::open(&path).unwrap();
        assert_eq!((f.sample_rate(), f.read(0, 3).unwrap()), (44_100, vec![7, 7, 8, 8, 9, 9]));

        let path = dir.path().join("surround.wav");
        let mut w = WavWriter::create(&path, 48_000, 6).unwrap();
        w.append(&[0; 12]).unwrap();
        w.finish().unwrap();
        assert!(WavFile::open(&path).err().unwrap().to_string().contains("6 channels"));
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
    fn a_sound_file_written_by_its_thread_ends_up_exactly_as_if_written_directly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let thread = WavThread::start(WavWriter::create(&path, 48_000, 2).unwrap());
        for round in 0..20 {
            thread.append(tone(480, 2, 9)).unwrap();
            if round % 5 == 0 {
                thread.sync().unwrap();
            }
            if round == 10 {
                thread.sync_to_disk().unwrap();
            }
        }
        assert_eq!(thread.finish().unwrap(), 9600);
        let info = wav_info(&path).unwrap();
        assert_eq!((info.frames, info.peak > 0.0), (9600, true));
        // The same samples, in the same order, as writing them directly.
        let direct = dir.path().join("direct.wav");
        let mut w = WavWriter::create(&direct, 48_000, 2).unwrap();
        for _ in 0..20 {
            w.append(&tone(480, 2, 9)).unwrap();
        }
        w.finish().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(&direct).unwrap());
    }

    #[test]
    fn handing_over_sound_never_waits_for_the_file() {
        // Take the file's lock away from the thread by making it busy with a very long queue of syncs to disk:
        // the caller must still get its calls back at once.
        let dir = tempfile::tempdir().unwrap();
        let thread = WavThread::start(WavWriter::create(&dir.path().join("mic.wav"), 48_000, 2).unwrap());
        let started = std::time::Instant::now();
        for _ in 0..200 {
            thread.append(tone(4800, 2, 9)).unwrap();
            thread.sync_to_disk().unwrap();
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(1), "queueing took {:?}", started.elapsed());
        assert_eq!(thread.finish().unwrap(), 960_000);
    }

    #[test]
    fn a_write_that_fails_is_reported_by_the_next_call_and_by_finish() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = WavWriter::create(&dir.path().join("mic.wav"), 48_000, 2).unwrap();
        // A file already at its size limit refuses more sound.
        writer.frames = MAX_DATA_BYTES / 4;
        let thread = WavThread::start(writer);
        thread.append(tone(100, 2, 9)).unwrap(); // accepted for queueing; the thread finds it cannot write it
        let mut told = false;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            if let Err(e) = thread.append(tone(10, 2, 9)) {
                assert!(e.to_string().contains("cannot hold more than about 6 hours"), "{e}");
                told = true;
                break;
            }
        }
        assert!(told, "the failure is reported to the caller");
        assert!(thread.finish().is_err());
    }

    #[test]
    fn a_file_whose_thread_is_dropped_without_finishing_is_still_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        {
            let thread = WavThread::start(WavWriter::create(&path, 48_000, 2).unwrap());
            thread.append(tone(4800, 2, 9)).unwrap();
        }
        assert_eq!(wav_info(&path).unwrap().frames, 4800);
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
