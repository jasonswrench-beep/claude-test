// SPDX-License-Identifier: Apache-2.0
use crate::error::ProjectError;
use crate::fsutil::{new_id, utc_now_iso, write_atomic};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FORMAT_NAME: &str = "openstage-capture-project";
pub const SCHEMA_VERSION: &str = "0.1.0";

/// The tracks that carry accessibility information. They exist from the first version of the
/// format, so captions, transcripts, audio description, and alt text are never an afterthought.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tracks {
    pub captions: Vec<serde_json::Value>,
    pub transcript: Option<serde_json::Value>,
    pub audio_description: Vec<serde_json::Value>,
    pub alt_text: Vec<serde_json::Value>,
}

/// `capture.json`. It deliberately does not list takes; see the crate documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub language: String,
    pub created: String,
    pub tracks: Tracks,
    /// Reserved for the edit decision list (trims, splits, overlays, zooms).
    pub edit: Option<serde_json::Value>,
}

fn manifest_path(dir: &Path) -> std::path::PathBuf {
    dir.join("capture.json")
}

/// Make a new, empty capture project in `dir`.
pub fn create_project(dir: &Path, title: &str, language: &str) -> Result<Manifest, ProjectError> {
    if manifest_path(dir).exists() {
        return Err(ProjectError::AlreadyExists(dir.to_path_buf()));
    }
    fs::create_dir_all(dir.join("takes"))?;
    let manifest = Manifest {
        format: FORMAT_NAME.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        id: new_id("cap"),
        title: title.to_string(),
        language: language.to_string(),
        created: utc_now_iso(),
        tracks: Tracks::default(),
        edit: None,
    };
    let mut json = serde_json::to_string_pretty(&manifest)?;
    json.push('\n');
    write_atomic(&manifest_path(dir), json.as_bytes())?;
    Ok(manifest)
}

/// Open an existing capture project, checking that this program can read it.
pub fn open_project(dir: &Path) -> Result<Manifest, ProjectError> {
    let path = manifest_path(dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ProjectError::NotAProject(dir.to_path_buf())),
        Err(e) => return Err(e.into()),
    };
    let manifest: Manifest = serde_json::from_str(&text)?;
    if manifest.format != FORMAT_NAME {
        return Err(ProjectError::BadFormat(format!(
            "expected format \"{FORMAT_NAME}\" but found \"{}\"",
            manifest.format
        )));
    }
    // Version 0.x.y: any 0.1 file is readable. A future version ships a migration here.
    if !manifest.schema_version.starts_with("0.1.") {
        return Err(ProjectError::UnsupportedVersion(manifest.schema_version));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_project_can_be_opened_again() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lesson.capture");
        let made = create_project(&project, "Lesson", "en-US").unwrap();
        assert_eq!(open_project(&project).unwrap(), made);
        assert!(project.join("takes").is_dir());
    }

    #[test]
    fn accessibility_tracks_exist_from_the_start() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        let text = fs::read_to_string(dir.path().join("capture.json")).unwrap();
        for key in ["captions", "transcript", "audioDescription", "altText", "language"] {
            assert!(text.contains(key), "capture.json should mention {key}");
        }
    }

    #[test]
    fn creating_over_an_existing_project_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        create_project(dir.path(), "t", "en-US").unwrap();
        assert!(matches!(create_project(dir.path(), "t", "en-US"), Err(ProjectError::AlreadyExists(_))));
    }

    #[test]
    fn a_folder_without_a_manifest_is_not_a_project() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(open_project(dir.path()), Err(ProjectError::NotAProject(_))));
    }

    #[test]
    fn a_project_from_a_newer_format_is_refused_politely() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = create_project(dir.path(), "t", "en-US").unwrap();
        m.schema_version = "9.0.0".into();
        fs::write(dir.path().join("capture.json"), serde_json::to_string(&m).unwrap()).unwrap();
        assert!(matches!(open_project(dir.path()), Err(ProjectError::UnsupportedVersion(_))));
    }
}
