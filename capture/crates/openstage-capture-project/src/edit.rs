// SPDX-License-Identifier: Apache-2.0
//! The edit decision list, `edit.json`: reading it and checking it.
//!
//! The editor (TypeScript, in `capture-edit/`) makes and changes this file; the renderer (Rust) reads
//! it. Reading is strict about what the file is and lenient about what is in it: a file that is not an
//! edit, or has a version this program does not know, is refused; a file that reads but contains
//! mistakes comes back with its problems, so the author can be told in words.
//!
//! The checks, and the codes that name them, are the same as in the TypeScript editor
//! (`capture-edit/src/validate.ts`). The files in `capture-edit/fixtures/` are shared examples that
//! both must agree on; a test here reads them. The format is described in `docs/capture-edit-format.md`.

use serde::Deserialize;
use std::collections::HashSet;

pub const EDIT_FORMAT: &str = "openstage-capture-edit";
/// The shortest a clip may be, in milliseconds.
pub const MIN_CLIP_MS: f64 = 100.0;
/// Imported video clips can be at most this long.
pub const MAX_IMPORTED_VIDEO_MS: f64 = 30_000.0;
pub const MIN_VOLUME_DB: f64 = -40.0;
pub const MAX_VOLUME_DB: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrackKind {
    Screen,
    Webcam,
    Audio,
    Media,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    Screen,
    Webcam,
    Microphone,
    SystemAudio,
    Image,
    Audio,
    Video,
}

impl TrackKind {
    fn allows(self, source: SourceKind) -> bool {
        use SourceKind as S;
        match self {
            TrackKind::Screen => source == S::Screen,
            TrackKind::Webcam => source == S::Webcam,
            TrackKind::Audio => matches!(source, S::Microphone | S::SystemAudio | S::Audio),
            TrackKind::Media => matches!(source, S::Image | S::Video | S::Audio),
        }
    }

    fn name(self) -> &'static str {
        match self {
            TrackKind::Screen => "screen",
            TrackKind::Webcam => "webcam",
            TrackKind::Audio => "audio",
            TrackKind::Media => "imported media",
        }
    }
}

/// Where a clip's picture or sound comes from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    /// A file of a recording: take number and file name.
    Take { take: u32, media: String },
    /// A file from the course's assets.
    Asset { asset: String },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(rename = "ref")]
    pub source: SourceRef,
    pub kind: SourceKind,
    pub name: String,
    /// Its natural length; `None` for a still image.
    pub duration_ms: Option<f64>,
    pub has_text_alternative: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub start_ms: f64,
    #[serde(default)]
    pub muted: bool,
    pub volume_db: f64,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
    pub alt_text: Option<String>,
    pub decorative: Option<bool>,
}

impl Clip {
    pub fn length_ms(&self) -> f64 {
        self.source_end_ms - self.source_start_ms
    }
    pub fn end_ms(&self) -> f64 {
        self.start_ms + self.length_ms()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub clips: Vec<Clip>,
}

/// `edit.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub format: String,
    pub schema_version: String,
    pub next_id: f64,
    pub sources: Vec<SourceInfo>,
    pub tracks: Vec<Track>,
}

impl Edit {
    /// How long the whole edit is: where the last clip ends.
    pub fn length_ms(&self) -> f64 {
        self.tracks.iter().flat_map(|t| &t.clips).map(Clip::end_ms).fold(0.0, f64::max)
    }

    fn source(&self, r: &SourceRef) -> Option<&SourceInfo> {
        self.sources.iter().find(|s| &s.source == r)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The edit is not sound and must not be rendered.
    Error,
    /// The edit is sound, but something OpenStage requires is still missing.
    NeedsAttention,
}

impl Severity {
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::NeedsAttention => "needs-attention",
        }
    }
}

/// One finding about an edit, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The same names as in the TypeScript editor, such as `overlap` or `needs-alt-text`.
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub track_id: Option<String>,
    pub clip_id: Option<String>,
}

/// An edit that was read, and what is wrong with it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEdit {
    pub edit: Edit,
    pub problems: Vec<Problem>,
}

impl ParsedEdit {
    /// Whether the edit can be rendered: it has no errors (things still needed are allowed here).
    pub fn is_sound(&self) -> bool {
        !self.problems.iter().any(|p| p.severity == Severity::Error)
    }
}

/// Time in words, the same wording as the editor: `72400` is "1 minute 12.4 seconds".
pub fn describe_time(ms: f64) -> String {
    let total = if ms.is_finite() && ms > 0.0 { ms.round() as u64 } else { 0 };
    let hours = total / 3_600_000;
    let minutes = total / 60_000 % 60;
    let rest = total % 60_000;
    let plural = |n: u64, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(plural(hours, "hour"));
    }
    if minutes > 0 {
        parts.push(plural(minutes, "minute"));
    }
    if rest > 0 || parts.is_empty() {
        let text = format!("{:.3}", rest as f64 / 1000.0);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        let text = if text.is_empty() { "0" } else { text };
        parts.push(format!("{text} {}", if text == "1" { "second" } else { "seconds" }));
    }
    parts.join(" ")
}

fn whole(n: f64) -> bool {
    n.is_finite() && n >= 0.0 && n.fract() == 0.0
}

/// Every problem in an edit.
pub fn validate(edit: &Edit) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for track in &edit.tracks {
        if !seen.insert(&track.id) {
            out.push(Problem {
                code: "duplicate-id",
                severity: Severity::Error,
                message: format!("Two tracks have the id {}. Every id must be different.", track.id),
                track_id: Some(track.id.clone()),
                clip_id: None,
            });
        }
        let mut previous: Option<&Clip> = None;
        for clip in &track.clips {
            if !seen.insert(&clip.id) {
                out.push(Problem {
                    code: "duplicate-id",
                    severity: Severity::Error,
                    message: format!("Two clips have the id {}. Every id must be different.", clip.id),
                    track_id: Some(track.id.clone()),
                    clip_id: None,
                });
            }
            let here = |code: &'static str, severity: Severity, message: String| Problem {
                code,
                severity,
                message,
                track_id: Some(track.id.clone()),
                clip_id: Some(clip.id.clone()),
            };
            let label = format!("The clip \"{}\" on the {} track", clip.name, track.name);

            if ![clip.start_ms, clip.source_start_ms, clip.source_end_ms, clip.fade_in_ms, clip.fade_out_ms]
                .iter()
                .all(|n| whole(*n))
            {
                out.push(here(
                    "bad-time",
                    Severity::Error,
                    format!("{label} has a time that is not a whole number of milliseconds, or is below zero."),
                ));
                previous = Some(clip);
                continue;
            }
            if clip.length_ms() < MIN_CLIP_MS {
                out.push(here(
                    "too-short",
                    Severity::Error,
                    format!(
                        "{label} is {} long. A clip must be at least {}.",
                        describe_time(clip.length_ms().max(0.0)),
                        describe_time(MIN_CLIP_MS)
                    ),
                ));
            }

            match edit.source(&clip.source) {
                None => out.push(here(
                    "unknown-source",
                    Severity::Error,
                    format!("{label} uses a recording or file that this edit does not know about."),
                )),
                Some(source) => {
                    if !track.kind.allows(source.kind) {
                        out.push(here(
                            "source-not-allowed",
                            Severity::Error,
                            format!("{label} holds {}, which cannot go on a {} track.", source.name, track.kind.name()),
                        ));
                    }
                    if let Some(duration) = source.duration_ms {
                        if clip.source_end_ms > duration {
                            out.push(here(
                                "beyond-source",
                                Severity::Error,
                                format!(
                                    "{label} uses up to {} of {}, which is only {} long.",
                                    describe_time(clip.source_end_ms),
                                    source.name,
                                    describe_time(duration)
                                ),
                            ));
                        }
                    }
                    let is_asset = matches!(source.source, SourceRef::Asset { .. });
                    if is_asset && source.kind == SourceKind::Video && clip.length_ms() > MAX_IMPORTED_VIDEO_MS {
                        out.push(here(
                            "imported-video-too-long",
                            Severity::Error,
                            format!(
                                "{label} is {} long. Imported video clips can be at most {}.",
                                describe_time(clip.length_ms()),
                                describe_time(MAX_IMPORTED_VIDEO_MS)
                            ),
                        ));
                    }
                    if source.kind == SourceKind::Image
                        && clip.decorative != Some(true)
                        && clip.alt_text.as_deref().unwrap_or("").trim().is_empty()
                    {
                        out.push(here(
                            "needs-alt-text",
                            Severity::NeedsAttention,
                            format!("{label} is an image with no alt text. Describe it, or mark it as decorative."),
                        ));
                    }
                    if is_asset
                        && matches!(source.kind, SourceKind::Video | SourceKind::Audio)
                        && source.has_text_alternative != Some(true)
                    {
                        out.push(here(
                            "needs-text-alternative",
                            Severity::NeedsAttention,
                            if source.kind == SourceKind::Video {
                                format!("{label} is an imported video with no captions or narration script. Add one.")
                            } else {
                                format!("{label} is imported sound with no captions, script or transcript. Add one, or describe it if it is wordless music.")
                            },
                        ));
                    }
                }
            }

            if clip.fade_in_ms + clip.fade_out_ms > clip.length_ms() {
                out.push(here(
                    "fade-too-long",
                    Severity::Error,
                    format!(
                        "{label} fades in for {} and out for {}, which is longer than the clip.",
                        describe_time(clip.fade_in_ms),
                        describe_time(clip.fade_out_ms)
                    ),
                ));
            }
            if !(clip.volume_db >= MIN_VOLUME_DB && clip.volume_db <= MAX_VOLUME_DB) || clip.volume_db.fract() != 0.0 {
                out.push(here(
                    "volume-range",
                    Severity::Error,
                    format!(
                        "{label} has its volume changed by {} decibels. Use a whole number from {MIN_VOLUME_DB} to {MAX_VOLUME_DB}.",
                        clip.volume_db
                    ),
                ));
            }

            if let Some(before) = previous {
                if clip.start_ms < before.start_ms {
                    out.push(here(
                        "unsorted",
                        Severity::Error,
                        format!("{label} comes after a clip that starts later. Clips must be in order of start."),
                    ));
                } else if clip.start_ms < before.end_ms() {
                    out.push(here(
                        "overlap",
                        Severity::Error,
                        format!(
                            "{label} starts at {}, before the clip \"{}\" before it ends at {}.",
                            describe_time(clip.start_ms),
                            before.name,
                            describe_time(before.end_ms())
                        ),
                    ));
                }
            }
            previous = Some(clip);
        }
    }
    out
}

/// Read the text of an `edit.json`. `Err` says in words why it is not an edit this program can read.
pub fn parse_edit(text: &str) -> Result<ParsedEdit, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("This is not a readable edit file: {e}"))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(EDIT_FORMAT) {
        return Err(format!("This is not an OpenStage Capture edit file: its format should be \"{EDIT_FORMAT}\"."));
    }
    match value.get("schemaVersion").and_then(|v| v.as_str()) {
        Some(v) if v.starts_with("0.1.") => {}
        other => {
            return Err(format!(
                "This edit file is version {}, and this program reads version 0.1 only. It was probably made by a newer or older program.",
                other.unwrap_or("(missing)")
            ))
        }
    }
    let edit: Edit = serde_json::from_value(value).map_err(|e| format!("This edit file cannot be read: {e}"))?;
    let problems = validate(&edit);
    Ok(ParsedEdit { edit, problems })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures")
    }

    #[derive(Deserialize)]
    struct Expected {
        fixtures: BTreeMap<String, Want>,
    }
    #[derive(Deserialize)]
    struct Want {
        readable: bool,
        problems: Vec<String>,
    }

    #[test]
    fn time_is_said_in_the_same_words_as_the_editor() {
        for (ms, words) in [
            (0.0, "0 seconds"),
            (1000.0, "1 second"),
            (500.0, "0.5 seconds"),
            (12_400.0, "12.4 seconds"),
            (60_000.0, "1 minute"),
            (61_000.0, "1 minute 1 second"),
            (72_400.0, "1 minute 12.4 seconds"),
            (3_725_000.0, "1 hour 2 minutes 5 seconds"),
            (3_600_000.0, "1 hour"),
            (72_435.0, "1 minute 12.435 seconds"),
            (-100.0, "0 seconds"),
        ] {
            assert_eq!(describe_time(ms), words, "{ms}");
        }
    }

    #[test]
    fn every_shared_example_is_found_to_be_what_it_is_meant_to_show() {
        let expected: Expected =
            serde_json::from_str(&fs::read_to_string(fixtures().join("expected.json")).unwrap()).unwrap();
        let mut on_disk: Vec<String> = fs::read_dir(fixtures())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json") && n != "expected.json")
            .collect();
        on_disk.sort();
        assert_eq!(on_disk, expected.fixtures.keys().cloned().collect::<Vec<_>>(), "expected.json lists every fixture");
        assert!(expected.fixtures.len() >= 15);

        for (name, want) in &expected.fixtures {
            let text = fs::read_to_string(fixtures().join(name)).unwrap();
            match parse_edit(&text) {
                Err(message) => {
                    assert!(!want.readable, "{name} should read, but: {message}");
                    assert!(!message.is_empty());
                }
                Ok(parsed) => {
                    assert!(want.readable, "{name} should not have read");
                    let mut found: Vec<String> =
                        parsed.problems.iter().map(|p| format!("{}:{}", p.code, p.severity.word())).collect();
                    found.sort();
                    let mut wanted = want.problems.clone();
                    wanted.sort();
                    assert_eq!(found, wanted, "{name}");
                    assert_eq!(parsed.is_sound(), !wanted.iter().any(|p| p.ends_with(":error")), "{name}");
                }
            }
        }
    }

    #[test]
    fn a_plain_edit_has_the_length_of_its_longest_track_and_reads_its_tracks_and_clips() {
        let text = fs::read_to_string(fixtures().join("valid-basic.json")).unwrap();
        let parsed = parse_edit(&text).unwrap();
        assert!(parsed.is_sound() && parsed.problems.is_empty());
        assert_eq!(parsed.edit.tracks.len(), 3);
        assert_eq!(parsed.edit.tracks[0].clips.len(), 2, "the screen track was split once");
        assert_eq!(parsed.edit.length_ms(), 60_000.0);
        assert_eq!(parsed.edit.tracks[0].clips[1].start_ms, 20_000.0);
        assert_eq!(parsed.edit.tracks[0].clips[1].source_start_ms, 20_000.0);
    }

    #[test]
    fn what_is_not_an_edit_is_refused_in_words() {
        for (text, fragment) in [
            ("nonsense", "not a readable edit file"),
            ("[]", "not an OpenStage Capture edit file"),
            (r#"{"format":"openstage-capture-edit"}"#, "version (missing)"),
            (r#"{"format":"openstage-capture-edit","schemaVersion":"0.2.0"}"#, "version 0.2.0"),
            (r#"{"format":"openstage-capture-edit","schemaVersion":"0.1.0"}"#, "cannot be read"),
        ] {
            let message = parse_edit(text).unwrap_err();
            assert!(message.contains(fragment), "{text}: {message}");
        }
    }
}
