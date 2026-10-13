// SPDX-License-Identifier: Apache-2.0
//! Segments: the crash-safe pieces a take is written in.
//!
//! A segment file starts with the 8 bytes `OSCSEG0\n`, followed by records. Each record is a
//! 16-byte header (length of the payload as 4 bytes, a CRC-32 of the payload as 4 bytes, the
//! media time in milliseconds as 8 bytes, all little-endian) and then the payload.
//!
//! While being written a segment is called `seg-000001.part`. It is renamed to
//! `seg-000001.seg` once complete. A file left as `.part` after a crash is repaired by
//! [`salvage_part`], which keeps every complete, checksummed record and drops the rest.

use crc32fast::Hasher;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"OSCSEG0\n";
const HEADER_LEN: u64 = 16;
/// A record larger than this is treated as damage, not data.
const MAX_RECORD_BYTES: u32 = 256 * 1024 * 1024;

/// `seg-000001.part`
pub fn part_path(dir: &Path, index: u32) -> PathBuf {
    dir.join(format!("seg-{index:06}.part"))
}

/// `seg-000001.seg`
pub fn final_path(dir: &Path, index: u32) -> PathBuf {
    dir.join(format!("seg-{index:06}.seg"))
}

fn crc(payload: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(payload);
    hasher.finalize()
}

/// What a finished segment contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentInfo {
    pub index: u32,
    pub records: u32,
    pub last_t_ms: u64,
    pub bytes: u64,
}

/// One record read back from a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub t_ms: u64,
    pub payload: Vec<u8>,
}

/// Writes one segment.
pub struct SegmentWriter {
    out: BufWriter<File>,
    part: PathBuf,
    fin: PathBuf,
    index: u32,
    records: u32,
    last_t_ms: u64,
    bytes: u64,
}

impl SegmentWriter {
    /// Start `seg-NNNNNN.part` in `dir`. Refuses to overwrite an existing file.
    pub fn create(dir: &Path, index: u32) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let part = part_path(dir, index);
        let file = OpenOptions::new().write(true).create_new(true).open(&part)?;
        let mut out = BufWriter::new(file);
        out.write_all(MAGIC)?;
        Ok(Self { out, part, fin: final_path(dir, index), index, records: 0, last_t_ms: 0, bytes: MAGIC.len() as u64 })
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn records(&self) -> u32 {
        self.records
    }

    /// Add one record.
    pub fn append(&mut self, t_ms: u64, payload: &[u8]) -> io::Result<()> {
        if payload.len() as u64 > MAX_RECORD_BYTES as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "record too large"));
        }
        self.out.write_all(&(payload.len() as u32).to_le_bytes())?;
        self.out.write_all(&crc(payload).to_le_bytes())?;
        self.out.write_all(&t_ms.to_le_bytes())?;
        self.out.write_all(payload)?;
        self.records += 1;
        self.last_t_ms = t_ms;
        self.bytes += HEADER_LEN + payload.len() as u64;
        Ok(())
    }

    /// Push everything written so far to the disk. A crash after this loses nothing before it.
    pub fn flush_sync(&mut self) -> io::Result<()> {
        self.out.flush()?;
        self.out.get_ref().sync_data()
    }

    /// Complete the segment: flush it to disk and rename `.part` to `.seg`.
    pub fn finish(self) -> io::Result<SegmentInfo> {
        let SegmentWriter { mut out, part, fin, index, records, last_t_ms, bytes } = self;
        out.flush()?;
        let file = out.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&part, &fin)?;
        Ok(SegmentInfo { index, records, last_t_ms, bytes })
    }
}

/// What a scan of a segment file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salvage {
    /// Whether the file starts with the segment marker.
    pub has_magic: bool,
    /// Complete, checksummed records.
    pub records: u32,
    pub last_t_ms: u64,
    /// The length of the good part of the file.
    pub valid_len: u64,
    /// Whether anything after the good part had to be ignored.
    pub truncated: bool,
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn walk(path: &Path, mut on_record: impl FnMut(u64, Vec<u8>)) -> io::Result<Salvage> {
    let file = File::open(path)?;
    let total = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 8];
    if read_full(&mut reader, &mut magic)? < 8 || &magic != MAGIC {
        return Ok(Salvage { has_magic: false, records: 0, last_t_ms: 0, valid_len: 0, truncated: total > 0 });
    }
    let mut valid_len = MAGIC.len() as u64;
    let mut records = 0u32;
    let mut last_t_ms = 0u64;
    loop {
        let mut header = [0u8; HEADER_LEN as usize];
        let got = read_full(&mut reader, &mut header)?;
        if got == 0 {
            break;
        }
        if got < header.len() {
            break;
        }
        let len = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let expected_crc = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let t_ms = u64::from_le_bytes(header[8..16].try_into().unwrap());
        if len > MAX_RECORD_BYTES {
            break;
        }
        let mut payload = vec![0u8; len as usize];
        if read_full(&mut reader, &mut payload)? < payload.len() || crc(&payload) != expected_crc {
            break;
        }
        valid_len += HEADER_LEN + len as u64;
        records += 1;
        last_t_ms = t_ms;
        on_record(t_ms, payload);
    }
    Ok(Salvage { has_magic: true, records, last_t_ms, valid_len, truncated: valid_len != total })
}

/// Check a segment file and report how much of it is good, without changing it.
pub fn scan(path: &Path) -> io::Result<Salvage> {
    walk(path, |_, _| {})
}

/// Read every good record in a segment file.
pub fn read_records(path: &Path) -> io::Result<Vec<Record>> {
    let mut out = Vec::new();
    walk(path, |t_ms, payload| out.push(Record { t_ms, payload }))?;
    Ok(out)
}

/// Repair a segment left as `.part`: keep every good record, cut off the damaged end, and rename
/// it to `.seg`. Returns `None` if nothing usable was in it (the file is then set aside as
/// `.unreadable`, or removed if it held no data at all).
pub fn salvage_part(part: &Path, fin: &Path, index: u32) -> io::Result<Option<SegmentInfo>> {
    let found = scan(part)?;
    if !found.has_magic {
        let mut aside = part.as_os_str().to_owned();
        aside.push(".unreadable");
        fs::rename(part, PathBuf::from(aside))?;
        return Ok(None);
    }
    if found.records == 0 {
        fs::remove_file(part)?;
        return Ok(None);
    }
    if found.truncated {
        let file = OpenOptions::new().write(true).open(part)?;
        file.set_len(found.valid_len)?;
        file.sync_all()?;
    }
    fs::rename(part, fin)?;
    Ok(Some(SegmentInfo { index, records: found.records, last_t_ms: found.last_t_ms, bytes: found.valid_len }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_segment(dir: &Path, index: u32, count: u32) -> SegmentInfo {
        let mut w = SegmentWriter::create(dir, index).unwrap();
        for i in 0..count {
            w.append(i as u64 * 10, format!("payload-{i}").as_bytes()).unwrap();
        }
        w.finish().unwrap()
    }

    #[test]
    fn a_finished_segment_reads_back_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let info = write_segment(dir.path(), 1, 5);
        assert_eq!((info.records, info.last_t_ms), (5, 40));
        assert!(final_path(dir.path(), 1).exists());
        assert!(!part_path(dir.path(), 1).exists());
        let records = read_records(&final_path(dir.path(), 1)).unwrap();
        assert_eq!(records.len(), 5);
        assert_eq!(records[2], Record { t_ms: 20, payload: b"payload-2".to_vec() });
        let s = scan(&final_path(dir.path(), 1)).unwrap();
        assert!(s.has_magic && !s.truncated);
    }

    #[test]
    fn a_segment_in_progress_is_a_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = SegmentWriter::create(dir.path(), 1).unwrap();
        w.append(0, b"x").unwrap();
        w.flush_sync().unwrap();
        assert!(part_path(dir.path(), 1).exists());
        assert!(!final_path(dir.path(), 1).exists());
    }

    #[test]
    fn an_existing_segment_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        write_segment(dir.path(), 1, 1);
        let _kept = SegmentWriter::create(dir.path(), 2).unwrap();
        assert!(SegmentWriter::create(dir.path(), 2).is_err());
    }

    #[test]
    fn a_torn_last_record_is_cut_off_and_the_rest_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = SegmentWriter::create(dir.path(), 1).unwrap();
        for i in 0..10u64 {
            w.append(i * 100, &[i as u8; 64]).unwrap();
        }
        w.flush_sync().unwrap();
        drop(w);
        // Simulate a crash in the middle of the last record: cut 20 bytes off the end.
        let part = part_path(dir.path(), 1);
        let len = fs::metadata(&part).unwrap().len();
        OpenOptions::new().write(true).open(&part).unwrap().set_len(len - 20).unwrap();

        let before = scan(&part).unwrap();
        assert_eq!((before.records, before.truncated), (9, true));

        let info = salvage_part(&part, &final_path(dir.path(), 1), 1).unwrap().unwrap();
        assert_eq!((info.records, info.last_t_ms), (9, 800));
        assert!(!part.exists());
        let after = scan(&final_path(dir.path(), 1)).unwrap();
        assert_eq!((after.records, after.truncated), (9, false));
    }

    #[test]
    fn a_flipped_byte_stops_the_read_at_the_damage() {
        let dir = tempfile::tempdir().unwrap();
        write_segment(dir.path(), 1, 6);
        let path = final_path(dir.path(), 1);
        let mut bytes = fs::read(&path).unwrap();
        let at = bytes.len() - 3; // inside the last record's payload
        bytes[at] ^= 0xFF;
        fs::write(&path, bytes).unwrap();
        let s = scan(&path).unwrap();
        assert_eq!((s.records, s.truncated), (5, true));
    }

    #[test]
    fn a_part_file_with_no_data_is_removed_and_a_junk_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        drop(SegmentWriter::create(dir.path(), 1).unwrap()); // only the marker is written
        let p1 = part_path(dir.path(), 1);
        assert_eq!(salvage_part(&p1, &final_path(dir.path(), 1), 1).unwrap(), None);
        assert!(!p1.exists());

        let p2 = part_path(dir.path(), 2);
        fs::write(&p2, b"this is not a segment").unwrap();
        assert_eq!(salvage_part(&p2, &final_path(dir.path(), 2), 2).unwrap(), None);
        assert!(!p2.exists());
        assert!(dir.path().join("seg-000002.part.unreadable").exists());
    }
}
