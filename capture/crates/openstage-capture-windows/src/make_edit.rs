// SPDX-License-Identifier: Apache-2.0
//! An edit made from a recorded take, so a recording can be exported without an editor (story F1).
//!
//! The plain edit is the take as it was recorded: the screen, the voice, the computer's sound and the webcam (when
//! the take has them), each whole, one clip on its own track. It is what the editor starts from, so what is exported
//! is exactly what an author would see when they open the take.
//!
//! The demo edit adds one of every kind of overlay at fixed fractions of the take's length, so a person can see
//! with their own eyes, in one short video, that text, a box, an arrow, a blur, a blacked-out box and a zoom are all
//! drawn where they should be. It is for testing the exporter, not for real lessons.

use openstage_capture_project::{parse_edit, read_take, Edit, TakeState};
use serde_json::{json, Value};
use std::path::Path;

/// What to put in the edit besides the take as recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditOptions {
    /// Add one of every kind of overlay, and a zoom.
    pub demo: bool,
}

fn source_kind(kind: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // (the edit's kind of source, the track's kind, the track's name)
    match kind {
        "screen" => Some(("screen", "screen", "Screen")),
        "webcam" => Some(("webcam", "webcam", "Webcam")),
        "microphone" => Some(("microphone", "audio", "Voice")),
        "system-audio" => Some(("system-audio", "audio", "Computer sound")),
        _ => None,
    }
}

fn name_of(kind: &str, take: u32) -> String {
    match kind {
        "screen" => format!("Screen of take {take}"),
        "webcam" => format!("Webcam of take {take}"),
        "microphone" => format!("Microphone of take {take}"),
        _ => format!("Computer sound of take {take}"),
    }
}

/// Make the edit for take `id` of the project at `project`. `Err` says in words why not.
pub fn edit_from_take(project: &Path, id: u32, options: EditOptions) -> Result<Edit, String> {
    let take = read_take(project, id).map_err(|_| {
        format!("There is no take {id} in {}. Look in its takes folder for the numbers there are.", project.display())
    })?;
    if !matches!(take.state, TakeState::Complete | TakeState::Recovered) {
        return Err(format!(
            "Take {id} is {}, not finished. A recording that stopped without finishing is repaired with the recover command first.",
            match take.state {
                TakeState::Recording => "still marked as being recorded",
                TakeState::SetAside => "one that was set aside by starting again",
                _ => "not finished",
            }
        ));
    }
    let Some(length) = take.duration_ms.filter(|d| *d >= 100) else {
        return Err(format!("Take {id} has no length written down, so there is nothing to export."));
    };
    if !take.media.iter().any(|m| m.kind == "screen") {
        return Err(format!(
            "Take {id} has no screen recording in it (it was made by the test source), so there is nothing to export."
        ));
    }

    let mut sources = Vec::new();
    let mut tracks = Vec::new();
    let mut next = 1u32;
    let mut take_id = |prefix: &str| {
        let id = format!("{prefix}-{next}");
        next += 1;
        id
    };
    for media in &take.media {
        let Some((kind, track_kind, track_name)) = source_kind(&media.kind) else { continue };
        let source = json!({"kind": "take", "take": id, "media": media.file});
        sources.push(json!({"ref": source, "kind": kind, "name": name_of(kind, id), "durationMs": length}));
        tracks.push(json!({
            "id": take_id("track"),
            "kind": track_kind,
            "name": track_name,
            "clips": [{
                "id": take_id("clip"),
                "name": track_name,
                "source": source,
                "sourceStartMs": 0,
                "sourceEndMs": length,
                "startMs": 0,
                "muted": false,
                "volumeDb": 0,
                "fadeInMs": 0,
                "fadeOutMs": 0
            }],
            "items": []
        }));
    }
    if options.demo {
        let (overlay, zoom) = demo_tracks(length, &mut take_id);
        tracks.push(overlay);
        tracks.push(zoom);
    }
    let document = json!({
        "format": "openstage-capture-edit",
        "schemaVersion": "0.3.0",
        "nextId": next,
        "sources": sources,
        "tracks": tracks,
    });
    let parsed = parse_edit(&document.to_string())
        .map_err(|e| format!("The edit made from take {id} could not be read back: {e}"))?;
    if let Some(problem) = parsed.problems.iter().find(|p| p.severity == openstage_capture_project::EditSeverity::Error)
    {
        return Err(format!("The edit made from take {id} is not sound: {}", problem.message));
    }
    Ok(parsed.edit)
}

/// The overlay track and the zoom track of the demo, at fractions of `length` (whole milliseconds, items never shorter than 0.1 second).
fn demo_tracks(length: u64, id: &mut dyn FnMut(&str) -> String) -> (Value, Value) {
    let at = |fraction: f64| -> u64 { (length as f64 * fraction).round() as u64 };
    let span = |from: f64, to: f64| -> (u64, u64) {
        let start = at(from).min(length.saturating_sub(100));
        (start, at(to).saturating_sub(start).max(100).min(length - start))
    };
    let item = |kind: &str, name: &str, from: f64, to: f64, id: String, rest: Value| -> Value {
        let (start, duration) = span(from, to);
        let mut v = json!({"id": id, "kind": kind, "name": name, "startMs": start, "durationMs": duration});
        if let (Some(base), Some(extra)) = (v.as_object_mut(), rest.as_object()) {
            for (k, x) in extra {
                base.insert(k.clone(), x.clone());
            }
        }
        v
    };
    let mut items = vec![
        item(
            "text",
            "Title",
            0.05,
            0.45,
            id("item"),
            json!({"text": "OpenStage Capture export test", "rect": {"x": 500, "y": 500, "width": 5200, "height": 1000},
                   "fontSizeBp": 500, "color": "#FFFFFF", "background": "#14283C", "bold": true, "align": "left"}),
        ),
        item(
            "shape",
            "Box",
            0.25,
            0.55,
            id("item"),
            json!({"shape": "rectangle", "rect": {"x": 3500, "y": 3500, "width": 3000, "height": 2400},
                   "outline": "#FF0000", "fill": null, "lineWidthBp": 120, "decorative": true}),
        ),
        item(
            "arrow",
            "Arrow",
            0.25,
            0.55,
            id("item"),
            json!({"from": {"x": 1500, "y": 8200}, "to": {"x": 3800, "y": 5600}, "color": "#00B050", "lineWidthBp": 100, "decorative": true}),
        ),
        item(
            "shape",
            "Ellipse",
            0.25,
            0.55,
            id("item"),
            json!({"shape": "ellipse", "rect": {"x": 7000, "y": 3000, "width": 2200, "height": 2200},
                   "outline": "#FFC000", "fill": null, "lineWidthBp": 120, "decorative": true}),
        ),
        item(
            "blur",
            "Blur",
            0.45,
            0.8,
            id("item"),
            json!({"rect": {"x": 6500, "y": 1500, "width": 3000, "height": 1500}}),
        ),
        item(
            "redact",
            "Blackout",
            0.45,
            0.8,
            id("item"),
            json!({"rect": {"x": 6500, "y": 3500, "width": 3000, "height": 1000}, "color": "#000000"}),
        ),
    ];
    items.sort_by_key(|v| v["startMs"].as_u64().unwrap_or(0));
    let overlay = json!({"id": id("track"), "kind": "overlay", "name": "Overlays", "clips": [], "items": items});
    let (start, duration) = span(0.6, 0.9);
    let transition = 500.min(duration / 3);
    let zoom_item = json!({
        "id": id("item"), "kind": "zoom", "name": "Zoom", "startMs": start, "durationMs": duration,
        "rect": {"x": 0, "y": 0, "width": 5000, "height": 5000}, "transitionMs": transition,
        "description": "The top left quarter of the screen."
    });
    let zoom = json!({"id": id("track"), "kind": "zoom", "name": "Zoom", "clips": [], "items": [zoom_item]});
    (overlay, zoom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstage_capture_project::{
        create_project, create_take_with, finish_take, set_aside_take, validate_edit, EditItem, EditSeverity,
        EditTrackKind, MediaFile,
    };

    fn media(file: &str, kind: &str, container: &str) -> MediaFile {
        MediaFile {
            file: file.into(),
            kind: kind.into(),
            container: container.into(),
            mix: None,
            region: None,
            camera: None,
        }
    }

    /// A project holding take 1 with the given media, finished after `length` milliseconds.
    fn project_with(files: &[(&str, &str, &str)], length: u64) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("p.capture");
        create_project(&project, "t", "en-US").unwrap();
        let list = files.iter().map(|(f, k, c)| media(f, k, c)).collect();
        create_take_with(&project, 1, vec!["screen:1".into()], None, 2000, list).unwrap();
        finish_take(&project, 1, length, files.len() as u32).unwrap();
        (dir, project)
    }

    const FULL_TAKE: [(&str, &str, &str); 4] = [
        ("screen.mp4", "screen", "fragmented-mp4"),
        ("mic.wav", "microphone", "wav"),
        ("system.wav", "system-audio", "wav"),
        ("webcam.mp4", "webcam", "fragmented-mp4"),
    ];

    #[test]
    fn a_take_becomes_an_edit_with_a_track_for_each_thing_recorded_whole() {
        let (_dir, project) = project_with(&FULL_TAKE, 12_000);
        let edit = edit_from_take(&project, 1, EditOptions::default()).unwrap();
        let kinds: Vec<(EditTrackKind, &str)> = edit.tracks.iter().map(|t| (t.kind, t.name.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (EditTrackKind::Screen, "Screen"),
                (EditTrackKind::Audio, "Voice"),
                (EditTrackKind::Audio, "Computer sound"),
                (EditTrackKind::Webcam, "Webcam")
            ]
        );
        assert!(edit
            .tracks
            .iter()
            .all(|t| t.clips.len() == 1 && t.clips[0].start_ms == 0.0 && t.clips[0].end_ms() == 12_000.0));
        assert_eq!(edit.length_ms(), 12_000.0);
        assert!(validate_edit(&edit).iter().all(|p| p.severity != EditSeverity::Error));
    }

    #[test]
    fn a_take_with_only_a_screen_gives_only_a_screen_track() {
        let (_dir, project) = project_with(&[("screen.mp4", "screen", "fragmented-mp4")], 5000);
        let edit = edit_from_take(&project, 1, EditOptions::default()).unwrap();
        assert_eq!(edit.tracks.len(), 1);
        assert!(validate_edit(&edit).is_empty());
    }

    #[test]
    fn the_demo_adds_one_of_every_overlay_and_a_zoom_all_inside_the_take_and_all_finished() {
        let (_dir, project) = project_with(&FULL_TAKE, 12_000);
        let edit = edit_from_take(&project, 1, EditOptions { demo: true }).unwrap();
        let overlay = edit.tracks.iter().find(|t| t.kind == EditTrackKind::Overlay).unwrap();
        let kinds: Vec<&str> = overlay.items.iter().map(|i| i.kind_name()).collect();
        for want in ["text overlay", "shape", "arrow", "blur", "redaction"] {
            assert!(kinds.contains(&want), "{want} is missing from {kinds:?}");
        }
        assert_eq!(kinds.iter().filter(|k| **k == "shape").count(), 2, "a box and an ellipse");
        let zoom = edit.tracks.iter().find(|t| t.kind == EditTrackKind::Zoom).unwrap();
        assert_eq!(zoom.items.len(), 1);
        for item in overlay.items.iter().chain(zoom.items.iter()) {
            assert!(item.start_ms() >= 0.0 && item.end_ms() <= 12_000.0, "{} is outside the take", item.name());
            assert!(item.duration_ms() >= 100.0);
        }
        let problems = validate_edit(&edit);
        assert!(
            problems.iter().all(|p| p.severity == EditSeverity::Warning),
            "the demo needs nothing more from the author: {:?}",
            problems.iter().map(|p| (p.code, p.severity)).collect::<Vec<_>>()
        );
        let texts: Vec<&str> = overlay
            .items
            .iter()
            .filter_map(|i| if let EditItem::Text { text, .. } = i { Some(text.as_str()) } else { None })
            .collect();
        assert_eq!(texts, ["OpenStage Capture export test"]);
    }

    #[test]
    fn the_demo_fits_a_short_take_too() {
        let (_dir, project) = project_with(&FULL_TAKE, 1500);
        let edit = edit_from_take(&project, 1, EditOptions { demo: true }).unwrap();
        assert!(validate_edit(&edit).iter().all(|p| p.severity == EditSeverity::Warning));
    }

    #[test]
    fn a_take_that_cannot_be_exported_is_explained() {
        let (_dir, project) = project_with(&FULL_TAKE, 12_000);
        assert!(edit_from_take(&project, 7, EditOptions::default()).unwrap_err().contains("There is no take 7"));

        // Still marked as recording (the program stopped without finishing).
        create_take_with(
            &project,
            2,
            vec!["screen:1".into()],
            None,
            2000,
            vec![media("screen.mp4", "screen", "fragmented-mp4")],
        )
        .unwrap();
        let e = edit_from_take(&project, 2, EditOptions::default()).unwrap_err();
        assert!(e.contains("still marked as being recorded") && e.contains("recover"), "{e}");

        // Set aside by starting again.
        create_take_with(
            &project,
            3,
            vec!["screen:1".into()],
            None,
            2000,
            vec![media("screen.mp4", "screen", "fragmented-mp4")],
        )
        .unwrap();
        set_aside_take(&project, 3, 4000, 1).unwrap();
        assert!(edit_from_take(&project, 3, EditOptions::default())
            .unwrap_err()
            .contains("set aside by starting again"));

        // A take of the test source has no screen.
        create_take_with(&project, 4, vec!["synthetic".into()], None, 1000, vec![]).unwrap();
        finish_take(&project, 4, 4000, 2).unwrap();
        assert!(edit_from_take(&project, 4, EditOptions::default()).unwrap_err().contains("no screen recording"));

        // Finished, but with no length.
        let (_d, short) = project_with(&FULL_TAKE, 50);
        assert!(edit_from_take(&short, 1, EditOptions::default()).unwrap_err().contains("no length written down"));
    }
}
