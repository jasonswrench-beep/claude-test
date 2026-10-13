// SPDX-License-Identifier: Apache-2.0
//! Reading just enough of a fragmented MP4 file to tell how much of it survived a crash.
//!
//! A fragmented MP4 is a setup box (`moov`) followed by many short fragments, each a `moof` box
//! (what is in the fragment) and an `mdat` box (the video itself). Every fragment is complete on
//! its own, so a file that stops in the middle of a fragment is still good up to the one before.
//! This module finds that point and can cut the file back to it. It looks only at the outermost
//! boxes (their 8-byte headers), never inside them.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// One outermost box of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopBox {
    /// The four-letter kind, such as `moof`.
    pub kind: String,
    pub start: u64,
    /// The whole box, header included. For an unfinished box, how much of it exists.
    pub len: u64,
    /// Whether the whole box is in the file.
    pub complete: bool,
}

/// What a scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp4Scan {
    pub file_len: u64,
    pub boxes: Vec<TopBox>,
    /// The length of the longest start of the file that ends exactly at a safe point: after the setup
    /// box, or after a whole fragment. Cutting the file here loses nothing that could be played.
    pub safe_len: u64,
    /// Complete fragments (a `moof` followed by its complete `mdat`).
    pub fragments: u32,
}

impl Mp4Scan {
    /// Bytes after the safe point: an unfinished fragment or damage.
    pub fn unsafe_tail(&self) -> u64 {
        self.file_len - self.safe_len
    }
}

fn kind_text(bytes: [u8; 4]) -> String {
    bytes.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '?' }).collect()
}

/// Walk the outermost boxes of the file at `path`.
pub fn scan_file(path: &Path) -> io::Result<Mp4Scan> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    scan_reader(&mut file, file_len)
}

pub(crate) fn scan_reader(source: &mut (impl Read + Seek), file_len: u64) -> io::Result<Mp4Scan> {
    let mut boxes = Vec::new();
    let mut safe_len = 0u64;
    let mut fragments = 0u32;
    let mut pos = 0u64;
    let mut after_moof = false;

    while pos < file_len {
        let left = file_len - pos;
        if left < 8 {
            boxes.push(TopBox { kind: "????".into(), start: pos, len: left, complete: false });
            break;
        }
        source.seek(SeekFrom::Start(pos))?;
        let mut header = [0u8; 8];
        source.read_exact(&mut header)?;
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let kind_bytes = [header[4], header[5], header[6], header[7]];
        let kind = kind_text(kind_bytes);

        let len = match size32 {
            // The box runs to the end of the file.
            0 => left,
            // A 64-bit length follows the header.
            1 => {
                if left < 16 {
                    boxes.push(TopBox { kind, start: pos, len: left, complete: false });
                    break;
                }
                let mut big = [0u8; 8];
                source.read_exact(&mut big)?;
                u64::from_be_bytes(big)
            }
            n => n,
        };
        if len < 8 || (size32 == 1 && len < 16) {
            // Not a real box: the file is damaged here.
            boxes.push(TopBox { kind, start: pos, len: left, complete: false });
            break;
        }
        if len > left {
            boxes.push(TopBox { kind, start: pos, len: left, complete: false });
            break;
        }

        boxes.push(TopBox { kind: kind.clone(), start: pos, len, complete: true });
        let end = pos + len;
        match kind.as_str() {
            "moof" => after_moof = true,
            "mdat" if after_moof => {
                fragments += 1;
                after_moof = false;
                safe_len = end;
            }
            // Setup and bookkeeping boxes are safe stopping points only between fragments.
            _ if !after_moof => safe_len = end,
            _ => {}
        }
        pos = end;
    }

    Ok(Mp4Scan { file_len, boxes, safe_len, fragments })
}

/// What a repair did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    pub before: u64,
    pub after: u64,
}

/// Cut the file back to its last safe point. Does nothing if it already ends there. If no safe
/// point exists (not even the setup box), the file is left alone.
pub fn repair_file(path: &Path) -> io::Result<Repair> {
    let scan = scan_file(path)?;
    if scan.safe_len == 0 || scan.safe_len == scan.file_len {
        return Ok(Repair { before: scan.file_len, after: scan.file_len });
    }
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.set_len(scan.safe_len)?;
    file.sync_all()?;
    Ok(Repair { before: scan.file_len, after: scan.safe_len })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn boxed(kind: &str, payload_len: usize) -> Vec<u8> {
        let mut out = ((payload_len + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind.as_bytes());
        out.extend(std::iter::repeat_n(0xAB, payload_len));
        out
    }

    fn file_of(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    fn scan(bytes: &[u8]) -> Mp4Scan {
        scan_reader(&mut Cursor::new(bytes), bytes.len() as u64).unwrap()
    }

    fn init() -> Vec<Vec<u8>> {
        vec![boxed("ftyp", 16), boxed("moov", 100)]
    }

    #[test]
    fn an_empty_file_has_no_safe_point() {
        let s = scan(&[]);
        assert_eq!((s.safe_len, s.fragments, s.boxes.len()), (0, 0, 0));
    }

    #[test]
    fn setup_only_is_a_safe_playable_file_with_no_fragments() {
        let bytes = file_of(&init());
        let s = scan(&bytes);
        assert_eq!(s.safe_len, bytes.len() as u64);
        assert_eq!(s.fragments, 0);
        assert_eq!(s.unsafe_tail(), 0);
    }

    #[test]
    fn whole_fragments_are_all_kept() {
        let mut parts = init();
        for _ in 0..3 {
            parts.push(boxed("moof", 40));
            parts.push(boxed("mdat", 5000));
        }
        let bytes = file_of(&parts);
        let s = scan(&bytes);
        assert_eq!(s.fragments, 3);
        assert_eq!(s.safe_len, bytes.len() as u64);
        assert!(s.boxes.iter().all(|b| b.complete));
    }

    #[test]
    fn a_fragment_cut_in_the_middle_of_its_video_is_dropped() {
        let mut parts = init();
        parts.push(boxed("moof", 40));
        parts.push(boxed("mdat", 5000));
        let good = file_of(&parts).len() as u64;
        parts.push(boxed("moof", 40));
        let mut bytes = file_of(&parts);
        bytes.extend_from_slice(&boxed("mdat", 5000)[..2000]); // the crash
        let s = scan(&bytes);
        assert_eq!(s.fragments, 1);
        assert_eq!(s.safe_len, good, "the unfinished fragment (its moof too) is not safe");
        assert_eq!(s.unsafe_tail(), bytes.len() as u64 - good);
        assert!(!s.boxes.last().unwrap().complete);
    }

    #[test]
    fn a_moof_with_no_mdat_yet_is_not_safe() {
        let mut parts = init();
        parts.push(boxed("moof", 40));
        parts.push(boxed("mdat", 100));
        let good = file_of(&parts).len() as u64;
        parts.push(boxed("moof", 40));
        let s = scan(&file_of(&parts));
        assert_eq!(s.safe_len, good);
        assert_eq!(s.fragments, 1);
    }

    #[test]
    fn a_few_stray_bytes_at_the_end_are_not_safe_but_do_not_break_the_scan() {
        let mut bytes = file_of(&init());
        let good = bytes.len() as u64;
        bytes.extend_from_slice(&[0, 0, 1]);
        let s = scan(&bytes);
        assert_eq!(s.safe_len, good);
        assert_eq!(s.unsafe_tail(), 3);
    }

    #[test]
    fn a_box_with_a_64_bit_length_is_understood() {
        let mut big = 1u32.to_be_bytes().to_vec();
        big.extend_from_slice(b"mdat");
        big.extend_from_slice(&(16u64 + 10).to_be_bytes());
        big.extend(std::iter::repeat_n(7u8, 10));
        let mut parts = init();
        parts.push(boxed("moof", 8));
        parts.push(big);
        let s = scan(&file_of(&parts));
        assert_eq!(s.fragments, 1);
        assert!(s.boxes.last().unwrap().complete);
    }

    #[test]
    fn a_last_box_that_runs_to_the_end_of_the_file_counts_as_whole() {
        let mut to_end = 0u32.to_be_bytes().to_vec();
        to_end.extend_from_slice(b"mdat");
        to_end.extend(std::iter::repeat_n(1u8, 500));
        let mut parts = init();
        parts.push(boxed("moof", 8));
        parts.push(to_end);
        let s = scan(&file_of(&parts));
        assert_eq!(s.fragments, 1);
        assert_eq!(s.unsafe_tail(), 0);
    }

    #[test]
    fn a_nonsense_box_length_stops_the_scan_at_the_damage() {
        let mut bytes = file_of(&init());
        let good = bytes.len() as u64;
        bytes.extend_from_slice(&3u32.to_be_bytes()); // a box cannot be shorter than its own header
        bytes.extend_from_slice(b"moof");
        bytes.extend_from_slice(&[0; 20]);
        let s = scan(&bytes);
        assert_eq!(s.safe_len, good);
    }

    #[test]
    fn repair_cuts_the_file_back_and_leaves_a_clean_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.mp4");
        let mut parts = init();
        parts.push(boxed("moof", 40));
        parts.push(boxed("mdat", 3000));
        let good = file_of(&parts);
        let mut torn = good.clone();
        torn.extend_from_slice(&boxed("moof", 40));
        torn.extend_from_slice(&boxed("mdat", 3000)[..777]);
        fs::write(&path, &torn).unwrap();

        let r = repair_file(&path).unwrap();
        assert_eq!((r.before, r.after), (torn.len() as u64, good.len() as u64));
        assert_eq!(fs::read(&path).unwrap(), good);

        let again = repair_file(&path).unwrap();
        assert_eq!(again.before, again.after, "running it twice changes nothing");
    }

    #[test]
    fn repair_never_empties_a_file_that_has_no_safe_point() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.mp4");
        fs::write(&path, [1, 2, 3, 4, 5]).unwrap();
        let r = repair_file(&path).unwrap();
        assert_eq!((r.before, r.after), (5, 5));
        assert_eq!(fs::read(&path).unwrap().len(), 5);
    }
}
