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

/// One media file of a take, named relative to the take's folder.
///
/// Takes of the test source keep their media in `segments/` and list nothing here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    /// For example `screen.mp4`.
    pub file: String,
    /// What it holds, for example `screen`.
    pub kind: String,
    /// How it is stored. `fragmented-mp4` is a crash-safe MP4: a setup box and then short,
    /// self-contained fragments, so a file cut off by a crash plays up to the last whole fragment.
    pub container: String,
    /// For the video file: how its sound was mixed from the separate sound files. Absent when the
    /// file holds no mixed sound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mix: Option<MixInfo>,
    /// For the video file: which part of the display it shows, when that is less than the whole
    /// display. Absent when the whole display was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionInfo>,
    /// For the webcam video (`kind` is `webcam`): which camera it came from. Absent on other files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraInfo>,
    /// For the interaction log (`kind` is `interactions`): what it holds and how to read its positions. Absent on
    /// other files, and when the author did not turn interaction recording on there is no such entry at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactions: Option<InteractionInfo>,
}

/// What an interaction log holds. Its positions (the cursor, a click, a window's place) are pixels of one display,
/// counted from that display's top left corner, the way [`RegionInfo`] counts, so the editor can put them on the
/// video by subtracting the region's `x` and `y` (or, for a recorded window, that window's place at the time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionInfo {
    /// The display the positions are measured on (1 is the main display).
    pub display: u32,
    /// Whether the keys that were pressed are in the log. When false the author chose to leave them out.
    pub keys: bool,
}

/// Which camera a webcam video came from. The size and rate of the picture are in the video itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraInfo {
    /// The camera's name as Windows gives it, such as "NexiGo Iris 4K Webcam".
    pub name: String,
}

/// The part of a display that a video shows. Pixels are measured from the display's top left
/// corner, so the editor can map anything recorded in display coordinates (the cursor, a click) onto
/// the video by subtracting `x` and `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionInfo {
    /// The display's number (1 is the main display), as in `sources`.
    pub display: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// How the sound inside a video file was mixed. The separate sound files are never changed: these
/// adjustments apply only to the mix, so the editor can start again from the untouched originals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixInfo {
    /// Louder (positive) or quieter (negative) by this many decibels when mixed. 0 means unchanged.
    pub microphone_gain_db: i32,
    pub system_gain_db: i32,
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
    /// Files, or the number of segment files for the test source.
    pub segments: u32,
    /// The take's media files (new in format 0.2). Absent in older projects.
    #[serde(default)]
    pub media: Vec<MediaFile>,
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

/// Make the folder for a new take (with a `segments` folder, for the test source) and write its
/// `take.json` in the `Recording` state.
pub fn create_take(
    project: &Path,
    id: u32,
    sources: Vec<String>,
    target: Option<serde_json::Value>,
    flush_ms: u64,
) -> Result<Take, ProjectError> {
    create_take_with(project, id, sources, target, flush_ms, Vec::new())
}

/// Like [`create_take`], naming the media files the take will hold. A take with named media files
/// gets no `segments` folder.
pub fn create_take_with(
    project: &Path,
    id: u32,
    sources: Vec<String>,
    target: Option<serde_json::Value>,
    flush_ms: u64,
    media: Vec<MediaFile>,
) -> Result<Take, ProjectError> {
    let dir = take_dir(project, id);
    if media.is_empty() {
        fs::create_dir_all(dir.join("segments"))?;
    } else {
        fs::create_dir_all(&dir)?;
    }
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
        media,
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
    fn a_version_0_1_take_json_without_media_still_reads_and_a_media_take_has_no_segments_folder() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        create_take(dir.path(), 1, vec![], None, 1000).unwrap();
        // An old file: no "media" key at all.
        let path = take_dir(dir.path(), 1).join("take.json");
        let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("media");
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        assert!(read_take(dir.path(), 1).unwrap().media.is_empty());

        let media = vec![MediaFile {
            file: "screen.mp4".into(),
            kind: "screen".into(),
            container: "fragmented-mp4".into(),
            mix: None,
            region: None,
            camera: None,
            interactions: None,
        }];
        let take = create_take_with(dir.path(), 2, vec!["screen:1".into()], None, 2000, media.clone()).unwrap();
        assert_eq!(take.media, media);
        assert!(!take_dir(dir.path(), 2).join("segments").exists());
        assert_eq!(read_take(dir.path(), 2).unwrap().media, media);
    }

    #[test]
    fn which_camera_made_the_webcam_video_is_written_down_and_old_files_without_it_still_read() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let media = vec![
            MediaFile {
                file: "screen.mp4".into(),
                kind: "screen".into(),
                container: "fragmented-mp4".into(),
                mix: None,
                region: None,
                camera: None,
                interactions: None,
            },
            MediaFile {
                file: "webcam.mp4".into(),
                kind: "webcam".into(),
                container: "fragmented-mp4".into(),
                mix: None,
                region: None,
                camera: Some(CameraInfo { name: "NexiGo Iris 4K Webcam".into() }),
                interactions: None,
            },
        ];
        create_take_with(dir.path(), 1, vec!["screen:1".into()], None, 2000, media.clone()).unwrap();
        assert_eq!(read_take(dir.path(), 1).unwrap().media, media);
        let text = std::fs::read_to_string(take_dir(dir.path(), 1).join("take.json")).unwrap();
        assert!(
            text.contains(r#""camera":{"name":"NexiGo Iris 4K Webcam"}"#) || text.contains("\"camera\": {"),
            "{text}"
        );
        assert_eq!(text.matches("\"camera\"").count(), 1, "only the webcam's entry says which camera");
        // An older file lists the same video without it.
        let old = r#"{"take":1,"state":"complete","sources":["screen:1"],"startedAt":"2026-10-14T10:00:00Z","flushMs":2000,
            "media":[{"file":"webcam.mp4","kind":"webcam","container":"fragmented-mp4"}]}"#;
        let parsed: serde_json::Value = serde_json::from_str(old).unwrap();
        let entry: MediaFile = serde_json::from_value(parsed["media"][0].clone()).unwrap();
        assert_eq!(entry.camera, None);
    }

    #[test]
    fn how_the_sound_was_mixed_is_written_down_and_old_files_without_it_still_read() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let media = vec![
            MediaFile {
                file: "screen.mp4".into(),
                kind: "screen".into(),
                container: "fragmented-mp4".into(),
                mix: Some(MixInfo { microphone_gain_db: 3, system_gain_db: -12 }),
                region: None,
                camera: None,
                interactions: None,
            },
            MediaFile {
                file: "mic.wav".into(),
                kind: "microphone".into(),
                container: "wav".into(),
                mix: None,
                region: None,
                camera: None,
                interactions: None,
            },
        ];
        create_take_with(dir.path(), 1, vec!["screen:1".into()], None, 2000, media.clone()).unwrap();
        let text = std::fs::read_to_string(take_dir(dir.path(), 1).join("take.json")).unwrap();
        assert!(text.contains("\"microphoneGainDb\": 3") && text.contains("\"systemGainDb\": -12"), "{text}");
        assert_eq!(text.matches("\"mix\"").count(), 1, "only the file that holds a mix says how it was mixed");
        assert_eq!(read_take(dir.path(), 1).unwrap().media, media);

        // A take.json from before the mix was written down has no "mix" key at all.
        let old = r#"{"id":1,"state":"complete","startedAt":"2026-10-13T12:00:00Z","endedAt":null,"setAsideAt":null,
            "sources":["screen:1"],"target":null,"flushMs":2000,"durationMs":1000,"maxLossMs":null,"segments":1,
            "media":[{"file":"screen.mp4","kind":"screen","container":"fragmented-mp4"}]}"#;
        std::fs::write(take_dir(dir.path(), 1).join("take.json"), old).unwrap();
        assert_eq!(read_take(dir.path(), 1).unwrap().media[0].mix, None);
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

    #[test]
    fn which_part_of_the_display_a_video_shows_is_written_down_and_old_files_without_it_still_read() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let media = vec![MediaFile {
            file: "screen.mp4".into(),
            kind: "screen".into(),
            container: "fragmented-mp4".into(),
            mix: None,
            region: Some(RegionInfo { display: 2, x: 100, y: 200, width: 1280, height: 720 }),
            camera: None,
            interactions: None,
        }];
        create_take_with(dir.path(), 1, vec!["screen:2".into()], None, 2000, media.clone()).unwrap();
        let text = std::fs::read_to_string(take_dir(dir.path(), 1).join("take.json")).unwrap();
        assert!(
            text.contains("\"region\"") && text.contains("\"display\": 2") && text.contains("\"width\": 1280"),
            "{text}"
        );
        assert_eq!(read_take(dir.path(), 1).unwrap().media, media);

        // A video of the whole display says nothing about a region, and files from before regions read as before.
        let whole = vec![MediaFile {
            file: "screen.mp4".into(),
            kind: "screen".into(),
            container: "fragmented-mp4".into(),
            mix: None,
            region: None,
            camera: None,
            interactions: None,
        }];
        create_take_with(dir.path(), 2, vec!["screen:1".into()], None, 2000, whole).unwrap();
        let text = std::fs::read_to_string(take_dir(dir.path(), 2).join("take.json")).unwrap();
        assert!(!text.contains("region"), "{text}");
        let old = r#"{"id":2,"state":"complete","startedAt":"2026-10-13T12:00:00Z","endedAt":null,"setAsideAt":null,
            "sources":["screen:1"],"target":null,"flushMs":2000,"durationMs":1000,"maxLossMs":null,"segments":1,
            "media":[{"file":"screen.mp4","kind":"screen","container":"fragmented-mp4","mix":{"microphoneGainDb":0,"systemGainDb":0}}]}"#;
        std::fs::write(take_dir(dir.path(), 2).join("take.json"), old).unwrap();
        assert_eq!(read_take(dir.path(), 2).unwrap().media[0].region, None);
    }
}
