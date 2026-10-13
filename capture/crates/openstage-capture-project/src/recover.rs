// SPDX-License-Identifier: Apache-2.0
use crate::error::ProjectError;
use crate::events_log::trim_incomplete_tail;
use crate::fmp4::{repair_file, scan_file};
use crate::fsutil::utc_now_iso;
use crate::manifest::open_project;
use crate::segment::{final_path, salvage_part, scan};
use crate::take::{list_takes, take_dir, write_take, Take, TakeState};
use std::fs;
use std::path::Path;

/// A take that was repaired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredTake {
    pub take: u32,
    pub duration_ms: u64,
    /// The most recording time this repair could have lost: how often data was forced to disk.
    pub max_loss_ms: u64,
    pub segments: u32,
}

/// What `recover` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoverReport {
    pub recovered: Vec<RecoveredTake>,
    /// Takes that needed nothing.
    pub untouched: Vec<u32>,
    /// Things worth telling the author, in plain language.
    pub problems: Vec<String>,
}

fn segment_index(name: &str, extension: &str) -> Option<u32> {
    name.strip_prefix("seg-")?.strip_suffix(extension)?.parse().ok()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

/// How long a fragment of a crash-safe MP4 is, as Windows' writer is asked to make them. Used to
/// estimate how long a recovered video is without opening it; the exact length is read when it is played.
const FRAGMENT_MS: u64 = 1000;

/// Repair a take whose media is crash-safe MP4 files: cut each file back to its last whole fragment.
fn recover_mp4_take(project: &Path, mut take: Take, report: &mut RecoverReport) -> Result<(), ProjectError> {
    let dir = take_dir(project, take.id);
    let mut longest_ms = 0u64;
    let mut usable_files = 0u32;
    for media in take.media.clone().iter().filter(|m| m.container == "fragmented-mp4") {
        let path = dir.join(&media.file);
        if !path.is_file() {
            report.problems.push(format!(
                "Take {}: the video file {} was never created, so there was nothing to recover from it.",
                take.id, media.file
            ));
            continue;
        }
        let scan = scan_file(&path)?;
        if scan.safe_len == 0 {
            report.problems.push(format!(
                "Take {}: the video file {} has no usable start, so it could not be repaired. It was left as it is.",
                take.id, media.file
            ));
            continue;
        }
        repair_file(&path)?;
        usable_files += 1;
        longest_ms = longest_ms.max(scan.fragments as u64 * FRAGMENT_MS);
    }

    let log = dir.join("events.jsonl");
    if log.exists() {
        trim_incomplete_tail(&log)?;
    }

    take.state = TakeState::Recovered;
    take.ended_at = Some(utc_now_iso());
    take.duration_ms = Some(longest_ms);
    take.max_loss_ms = Some(take.flush_ms);
    take.segments = usable_files;
    write_take(project, &take)?;
    report.recovered.push(RecoveredTake {
        take: take.id,
        duration_ms: longest_ms,
        max_loss_ms: take.flush_ms,
        segments: usable_files,
    });
    Ok(())
}

fn recover_one(project: &Path, mut take: Take, report: &mut RecoverReport) -> Result<(), ProjectError> {
    if take.media.iter().any(|m| m.container == "fragmented-mp4") {
        return recover_mp4_take(project, take, report);
    }
    let dir = take_dir(project, take.id);
    let segments = dir.join("segments");

    // 1. Repair any segment that was still being written.
    for name in names_in(&segments) {
        if let Some(index) = segment_index(&name, ".part") {
            let kept = salvage_part(&segments.join(&name), &final_path(&segments, index), index)?;
            if kept.is_none() {
                report.problems.push(format!(
                    "Take {}: a piece of the recording ({name}) held no usable data and was set aside.",
                    take.id
                ));
            }
        }
    }

    // 2. Count the finished segments and find where the recording ended.
    let mut indices: Vec<u32> = names_in(&segments).iter().filter_map(|n| segment_index(n, ".seg")).collect();
    indices.sort_unstable();
    let duration_ms = match indices.last() {
        Some(last) => scan(&final_path(&segments, *last))?.last_t_ms,
        None => 0,
    };

    // 3. Trim an interaction log line that was cut short.
    let log = dir.join("events.jsonl");
    if log.exists() {
        trim_incomplete_tail(&log)?;
    }

    // 4. Record the result in take.json.
    take.state = TakeState::Recovered;
    take.ended_at = Some(utc_now_iso());
    take.duration_ms = Some(duration_ms);
    take.max_loss_ms = Some(take.flush_ms);
    take.segments = indices.len() as u32;
    write_take(project, &take)?;
    report.recovered.push(RecoveredTake {
        take: take.id,
        duration_ms,
        max_loss_ms: take.flush_ms,
        segments: take.segments,
    });
    Ok(())
}

/// Repair every take that was interrupted. Safe to run again: takes that are already fine are
/// left alone. Nothing is ever deleted except a segment file that held no data at all.
pub fn recover_project(project: &Path) -> Result<RecoverReport, ProjectError> {
    open_project(project)?;
    let mut report = RecoverReport::default();
    for entry in list_takes(project)? {
        match entry.take {
            Some(take) if take.state == TakeState::Recording => recover_one(project, take, &mut report)?,
            Some(_) => report.untouched.push(entry.id),
            None => {
                let has_media = names_in(&entry.path.join("segments"))
                    .iter()
                    .any(|n| segment_index(n, ".part").is_some() || segment_index(n, ".seg").is_some());
                if has_media {
                    report.problems.push(format!(
                        "Take {} had no readable take.json, so its details were rebuilt from the recording itself.",
                        entry.id
                    ));
                    let rebuilt = Take {
                        id: entry.id,
                        state: TakeState::Recording,
                        started_at: utc_now_iso(),
                        ended_at: None,
                        set_aside_at: None,
                        sources: Vec::new(),
                        target: None,
                        flush_ms: 1000,
                        duration_ms: None,
                        max_loss_ms: None,
                        segments: 0,
                        media: Vec::new(),
                    };
                    recover_one(project, rebuilt, &mut report)?;
                } else {
                    report.problems.push(format!(
                        "Take {} has no readable take.json and no recording in it, so it was left alone.",
                        entry.id
                    ));
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::create_project;
    use crate::segment::{part_path, SegmentWriter};
    use crate::take::{create_take, finish_take, read_take};
    use std::fs::OpenOptions;

    /// A take that "crashed": two finished segments, one torn one, and a torn log line.
    fn crashed_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec!["synthetic".into()], None, 1000).unwrap();
        let segs = take_dir(dir.path(), 1).join("segments");
        for index in 1..=2u32 {
            let mut w = SegmentWriter::create(&segs, index).unwrap();
            for i in 0..5u64 {
                w.append((index as u64 - 1) * 2000 + i * 400, &[1u8; 32]).unwrap();
            }
            w.finish().unwrap();
        }
        let mut w = SegmentWriter::create(&segs, 3).unwrap();
        for i in 0..5u64 {
            w.append(4000 + i * 400, &[2u8; 32]).unwrap();
        }
        w.flush_sync().unwrap();
        drop(w);
        let part = part_path(&segs, 3);
        let len = fs::metadata(&part).unwrap().len();
        OpenOptions::new().write(true).open(&part).unwrap().set_len(len - 10).unwrap();
        fs::write(
            take_dir(dir.path(), 1).join("events.jsonl"),
            "{\"t\":1,\"type\":\"marker\",\"label\":\"a\"}\n{\"t\":2,\"type\":\"ma",
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_crashed_take_is_repaired_and_keeps_everything_complete() {
        let dir = crashed_project();
        let report = recover_project(dir.path()).unwrap();
        assert_eq!(report.recovered.len(), 1);
        let r = &report.recovered[0];
        assert_eq!((r.take, r.segments, r.max_loss_ms), (1, 3, 1000));
        // The third segment kept its first four records (the fifth was torn): last time 4000 + 3*400.
        assert_eq!(r.duration_ms, 5200);

        let take = read_take(dir.path(), 1).unwrap();
        assert_eq!(take.state, TakeState::Recovered);
        assert_eq!(take.duration_ms, Some(5200));
        let segs = take_dir(dir.path(), 1).join("segments");
        assert!(final_path(&segs, 3).exists() && !part_path(&segs, 3).exists());
        let log = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 1);
    }

    fn boxed(kind: &str, payload_len: usize) -> Vec<u8> {
        let mut out = ((payload_len + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind.as_bytes());
        out.extend(std::iter::repeat_n(0u8, payload_len));
        out
    }

    /// A take whose video is a crash-safe MP4 that was cut off in the middle of its sixth fragment.
    fn crashed_mp4_project() -> (tempfile::TempDir, std::path::PathBuf, Vec<u8>) {
        use crate::take::{create_take_with, MediaFile};
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let media =
            vec![MediaFile { file: "screen.mp4".into(), kind: "screen".into(), container: "fragmented-mp4".into() }];
        create_take_with(dir.path(), 1, vec!["screen:1".into()], None, 2000, media).unwrap();
        let mut good = boxed("ftyp", 16);
        good.extend(boxed("moov", 300));
        for _ in 0..5 {
            good.extend(boxed("moof", 60));
            good.extend(boxed("mdat", 4000));
        }
        let mut torn = good.clone();
        torn.extend(boxed("moof", 60));
        torn.extend_from_slice(&boxed("mdat", 4000)[..1234]);
        let path = take_dir(dir.path(), 1).join("screen.mp4");
        fs::write(&path, &torn).unwrap();
        fs::write(
            take_dir(dir.path(), 1).join("events.jsonl"),
            "{\"t\":1,\"type\":\"marker\",\"label\":\"a\"}\n{\"t\":2,",
        )
        .unwrap();
        (dir, path, good)
    }

    #[test]
    fn a_crashed_mp4_take_is_cut_back_to_its_last_whole_fragment() {
        let (dir, path, good) = crashed_mp4_project();
        let report = recover_project(dir.path()).unwrap();
        assert_eq!(report.recovered.len(), 1);
        let r = &report.recovered[0];
        assert_eq!((r.take, r.segments, r.max_loss_ms, r.duration_ms), (1, 1, 2000, 5000));
        assert_eq!(fs::read(&path).unwrap(), good, "only the unfinished fragment was removed");

        let take = read_take(dir.path(), 1).unwrap();
        assert_eq!((take.state, take.duration_ms, take.max_loss_ms), (TakeState::Recovered, Some(5000), Some(2000)));
        assert_eq!(take.media.len(), 1, "the take still names its video file");
        let log = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 1, "the half-written log line is trimmed too");
        assert!(report.problems.is_empty());
    }

    #[test]
    fn recovering_a_recovered_mp4_take_changes_nothing() {
        let (dir, path, good) = crashed_mp4_project();
        recover_project(dir.path()).unwrap();
        let again = recover_project(dir.path()).unwrap();
        assert!(again.recovered.is_empty());
        assert_eq!(again.untouched, vec![1]);
        assert_eq!(fs::read(&path).unwrap(), good);
    }

    #[test]
    fn an_mp4_take_whose_video_was_never_written_is_reported_not_hidden() {
        let (dir, path, _good) = crashed_mp4_project();
        fs::remove_file(&path).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert_eq!(report.recovered.len(), 1);
        assert_eq!(report.recovered[0].segments, 0);
        assert!(report.problems.iter().any(|p| p.contains("was never created")), "{:?}", report.problems);
    }

    #[test]
    fn an_mp4_file_with_no_usable_start_is_left_exactly_as_it_is() {
        let (dir, path, _good) = crashed_mp4_project();
        fs::write(&path, [7u8; 40]).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert!(report.problems.iter().any(|p| p.contains("no usable start")));
        assert_eq!(fs::read(&path).unwrap(), vec![7u8; 40], "nothing is ever erased");
    }

    #[test]
    fn recovering_twice_changes_nothing_the_second_time() {
        let dir = crashed_project();
        recover_project(dir.path()).unwrap();
        let again = recover_project(dir.path()).unwrap();
        assert!(again.recovered.is_empty());
        assert_eq!(again.untouched, vec![1]);
    }

    #[test]
    fn finished_takes_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        finish_take(dir.path(), 1, 100, 0).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert!(report.recovered.is_empty());
        assert_eq!(report.untouched, vec![1]);
    }

    #[test]
    fn a_take_with_no_take_json_is_rebuilt_from_its_media() {
        let dir = crashed_project();
        fs::remove_file(take_dir(dir.path(), 1).join("take.json")).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert_eq!(report.recovered.len(), 1);
        assert!(report.problems.iter().any(|p| p.contains("rebuilt")));
        assert_eq!(read_take(dir.path(), 1).unwrap().state, TakeState::Recovered);
    }

    #[test]
    fn an_empty_unreadable_take_is_left_alone_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        fs::create_dir_all(dir.path().join("takes/take-001")).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert!(report.recovered.is_empty());
        assert_eq!(report.problems.len(), 1);
        assert!(dir.path().join("takes/take-001").exists());
    }

    #[test]
    fn a_take_with_no_usable_media_still_becomes_a_recovered_empty_take() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        let report = recover_project(dir.path()).unwrap();
        assert_eq!(report.recovered[0].duration_ms, 0);
        assert_eq!(report.recovered[0].segments, 0);
    }
}
