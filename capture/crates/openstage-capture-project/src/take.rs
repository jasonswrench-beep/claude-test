// SPDX-License-Identifier: Apache-2.0
use crate::error::ProjectError;
use crate::fsutil::{utc_now_iso, write_atomic};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Where a take stands. This is the only record of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TakeState {
    /// Being recorded, or the program stopped before finishing it (see `recover`).
    Recording,
    /// Finished cleanly.
    Complete,
    /// Repaired after an interruption.
    Recovered,
    /// The author started again. The take is kept, never erased.
    SetAside,
}

/// `take.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    pub id: u32,
    pub state: TakeState,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub set_aside_at: Option<String>,
    pub sources: Vec<String>,
    /// What was being recorded (display, window, or region). Opaque to this crate.
    pub target: Option<serde_json::Value>,
    /// How often data is forced to disk. A crash can lose at most this much.
    pub flush_ms: u64,
    pub duration_ms: Option<u64>,
    pub max_loss_ms: Option<u64>,
    pub segments: u32,
}

/// A take folder found on disk, readable or not.
#[derive(Debug, Clone)]
pub struct TakeEntry {
    pub id: u32,
    pub path: PathBuf,
    /// `None` if `take.json` is missing or unreadable.
    pub take: Option<Take>,
}

/// The folder for a take, such as `takes/take-001`.
pub fn take_dir(project: &Path, id: u32) -> PathBuf {
    project.join("takes").join(format!("take-{id:03}"))
}

fn parse_take_name(name: &str) -> Option<u32> {
    name.strip_prefix("take-")?.parse().ok()
}

/// The number the next take should get.
pub fn next_take_id(project: &Path) -> Result<u32, ProjectError> {
    let mut highest = 0;
    let takes = project.join("takes");
    if takes.is_dir() {
        for entry in fs::read_dir(takes)? {
            let entry = entry?;
            if let Some(id) = entry.file_name().to_str().and_then(parse_take_name) {
                highest = highest.max(id);
            }
        }
    }
    Ok(highest + 1)
}

/// Write `take.json` atomically.
pub fn write_take(project: &Path, take: &Take) -> Result<(), ProjectError> {
    let mut json = serde_json::to_string_pretty(take)?;
    json.push('\n');
    write_atomic(&take_dir(project, take.id).join("take.json"), json.as_bytes())?;
    Ok(())
}

/// Read `take.json`.
pub fn read_take(project: &Path, id: u32) -> Result<Take, ProjectError> {
    let text = fs::read_to_string(take_dir(project, id).join("take.json"))?;
    Ok(serde_json::from_str(&text)?)
}

/// Make the folder for a new take and write its `take.json` in the `Recording` state.
pub fn create_take(
    project: &Path,
    id: u32,
    sources: Vec<String>,
    target: Option<serde_json::Value>,
    flush_ms: u64,
) -> Result<Take, ProjectError> {
    let dir = take_dir(project, id);
    fs::create_dir_all(dir.join("segments"))?;
    let take = Take {
        id,
        state: TakeState::Recording,
        started_at: utc_now_iso(),
        ended_at: None,
        set_aside_at: None,
        sources,
        target,
        flush_ms,
        duration_ms: None,
        max_loss_ms: None,
        segments: 0,
    };
    write_take(project, &take)?;
    Ok(take)
}

/// Mark a take as finished cleanly.
pub fn finish_take(project: &Path, id: u32, duration_ms: u64, segments: u32) -> Result<Take, ProjectError> {
    let mut take = read_take(project, id)?;
    take.state = TakeState::Complete;
    take.ended_at = Some(utc_now_iso());
    take.duration_ms = Some(duration_ms);
    take.segments = segments;
    write_take(project, &take)?;
    Ok(take)
}

/// Mark a take as set aside. Nothing is erased: the media stays where it is.
pub fn set_aside_take(project: &Path, id: u32, duration_ms: u64, segments: u32) -> Result<Take, ProjectError> {
    let mut take = read_take(project, id)?;
    take.state = TakeState::SetAside;
    let now = utc_now_iso();
    take.ended_at = Some(now.clone());
    take.set_aside_at = Some(now);
    take.duration_ms = Some(duration_ms);
    take.segments = segments;
    write_take(project, &take)?;
    Ok(take)
}

/// Every take folder in the project, in order.
pub fn list_takes(project: &Path) -> Result<Vec<TakeEntry>, ProjectError> {
    let mut out = Vec::new();
    let takes = project.join("takes");
    if !takes.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(takes)? {
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().and_then(parse_take_name) else { continue };
        let take = read_take(project, id).ok();
        out.push(TakeEntry { id, path: entry.path(), take });
    }
    out.sort_by_key(|e| e.id);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::create_project;

    #[test]
    fn take_numbers_count_up_and_folders_are_padded() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        assert_eq!(next_take_id(dir.path()).unwrap(), 1);
        create_take(dir.path(), 1, vec!["synthetic".into()], None, 1000).unwrap();
        assert_eq!(next_take_id(dir.path()).unwrap(), 2);
        assert!(dir.path().join("takes/take-001/segments").is_dir());
    }

    #[test]
    fn a_take_set_aside_is_kept_not_erased() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        std::fs::write(dir.path().join("takes/take-001/segments/seg-000001.seg"), b"data").unwrap();
        let t = set_aside_take(dir.path(), 1, 1234, 1).unwrap();
        assert_eq!(t.state, TakeState::SetAside);
        assert!(t.set_aside_at.is_some());
        assert!(dir.path().join("takes/take-001/segments/seg-000001.seg").exists());
        assert_eq!(read_take(dir.path(), 1).unwrap().state, TakeState::SetAside);
    }

    #[test]
    fn finishing_a_take_records_how_long_it_was() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        let t = finish_take(dir.path(), 1, 5000, 3).unwrap();
        assert_eq!((t.state, t.duration_ms, t.segments), (TakeState::Complete, Some(5000), 3));
    }

    #[test]
    fn unreadable_takes_are_listed_not_hidden() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        std::fs::create_dir_all(dir.path().join("takes/take-002")).unwrap();
        let takes = list_takes(dir.path()).unwrap();
        assert_eq!(takes.len(), 2);
        assert!(takes[0].take.is_some());
        assert!(takes[1].take.is_none());
    }
}
