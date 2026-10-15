// SPDX-License-Identifier: Apache-2.0
//! Reading back, in words, what was noted of the author's mouse and keyboard, and deleting it (story A6).
//!
//! The interaction log can hold private things (the title of every window in front, the names of the things clicked,
//! the keys pressed), so the author must be able to see exactly what was kept and to delete it. Deleting removes every
//! line that records what the author did and keeps the author's own markers. A line that cannot be read is removed
//! too, because a half-written line is as likely to be a click as anything else.

use crate::edit::describe_time;
use crate::error::ProjectError;
use crate::events_log::{read_log, LogEvent, LogLine};
use crate::fsutil::write_atomic;
use crate::take::{read_take, take_dir, write_take, TakeState};
use std::fs;
use std::path::Path;

/// How many windows and clicked items a summary names before it says "and so many more".
const NAMED_AT_MOST: usize = 12;

/// What a log holds, counted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogSummary {
    pub moves: u64,
    pub clicks: u64,
    pub keys: u64,
    pub windows: u64,
    pub elements: u64,
    pub markers: u64,
    /// The time of the last line, in milliseconds of recording.
    pub last_ms: u64,
    /// The windows that were in front, each once, in the order they first came to the front, as (title, program).
    pub windows_seen: Vec<(String, String)>,
    /// The items clicked, each once, in the order first clicked, as (name, kind, how many times).
    pub clicked: Vec<(String, String, u64)>,
}

impl LogSummary {
    /// How many lines record what the author did (everything but markers).
    pub fn interaction_lines(&self) -> u64 {
        self.moves + self.clicks + self.keys + self.windows + self.elements
    }
}

pub fn summarize(lines: &[LogLine]) -> LogSummary {
    let mut s = LogSummary::default();
    for line in lines {
        s.last_ms = s.last_ms.max(line.t);
        match &line.event {
            LogEvent::CursorMove { .. } => s.moves += 1,
            LogEvent::Click { .. } => s.clicks += 1,
            LogEvent::Key { .. } => s.keys += 1,
            LogEvent::Window { title, process, .. } => {
                s.windows += 1;
                let seen = (title.clone(), process.clone());
                if !s.windows_seen.contains(&seen) {
                    s.windows_seen.push(seen);
                }
            }
            LogEvent::Element { name, role } => {
                s.elements += 1;
                match s.clicked.iter_mut().find(|(n, r, _)| n == name && r == role) {
                    Some(entry) => entry.2 += 1,
                    None => s.clicked.push((name.clone(), role.clone(), 1)),
                }
            }
            LogEvent::Marker { .. } => s.markers += 1,
        }
    }
    s
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The summary as sentences. It counts the keys but does not print them, so reading it aloud or showing it to others
/// does not spill what was typed; `describe_lines` prints everything.
pub fn describe_summary(take: u32, s: &LogSummary) -> String {
    if s.interaction_lines() == 0 {
        return format!(
            "Take {take} holds no record of what you did with the mouse and keyboard.{}",
            if s.markers > 0 {
                format!(" It holds {} of your own.", plural(s.markers, "marker", "markers"))
            } else {
                String::new()
            }
        );
    }
    let mut text = format!(
        "Take {take} holds a record of what you did for {}: {}, {}, {}, {}, {}{}.",
        describe_time(s.last_ms as f64),
        plural(s.moves, "cursor position", "cursor positions"),
        plural(s.clicks, "click", "clicks"),
        plural(s.elements, "clicked item name", "clicked item names"),
        plural(s.windows, "window change", "window changes"),
        plural(s.keys, "key press", "key presses"),
        if s.markers > 0 {
            format!(", and {}", plural(s.markers, "marker of your own", "markers of your own"))
        } else {
            String::new()
        }
    );
    if !s.windows_seen.is_empty() {
        // By program, each with the first title it had. A program that keeps changing its title (Notepad shows the
        // first words of what you type) would otherwise fill the list, and print what was typed.
        let mut programs: Vec<(&str, Vec<&str>)> = Vec::new();
        for (title, process) in &s.windows_seen {
            match programs.iter_mut().find(|(p, _)| p == process) {
                Some((_, titles)) => titles.push(title),
                None => programs.push((process, vec![title])),
            }
        }
        text.push_str(" Windows that were in front: ");
        let shown: Vec<String> = programs
            .iter()
            .take(NAMED_AT_MOST)
            .map(|(process, titles)| match titles.len() {
                1 => format!("\"{}\" ({process})", titles[0]),
                n => {
                    format!("\"{}\" and {} other title{} ({process})", titles[0], n - 1, if n == 2 { "" } else { "s" })
                }
            })
            .collect();
        text.push_str(&shown.join("; "));
        if programs.len() > NAMED_AT_MOST {
            text.push_str(&format!("; and {} more programs", programs.len() - NAMED_AT_MOST));
        }
        text.push('.');
    }
    if !s.clicked.is_empty() {
        text.push_str(" Items you clicked: ");
        let shown: Vec<String> =
            s.clicked
                .iter()
                .take(NAMED_AT_MOST)
                .map(|(name, role, n)| {
                    if *n > 1 {
                        format!("the {role} \"{name}\" ({n} times)")
                    } else {
                        format!("the {role} \"{name}\"")
                    }
                })
                .collect();
        text.push_str(&shown.join("; "));
        if s.clicked.len() > NAMED_AT_MOST {
            text.push_str(&format!("; and {} more", s.clicked.len() - NAMED_AT_MOST));
        }
        text.push('.');
    }
    if s.keys > 0 {
        text.push_str(" The keys are not printed here; use --list to read every line, including the keys.");
    }
    if !s.windows_seen.is_empty() {
        text.push_str(
            " Only the first title of each program is shown, because some programs put what you type, or the page you are on, in their title; --list shows every title.",
        );
    }
    text
}

fn key_phrase(key: &str, modifiers: &[String]) -> String {
    let mut parts: Vec<String> = modifiers
        .iter()
        .map(|m| {
            let mut chars = m.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        })
        .collect();
    parts.push(key.to_string());
    parts.join("+")
}

/// One line of the log as a sentence.
pub fn describe_line(line: &LogLine) -> String {
    let at = describe_time(line.t as f64);
    match &line.event {
        LogEvent::CursorMove { x, y } => format!("At {at} the cursor was at {x}, {y}."),
        LogEvent::Click { x, y, button } => format!("At {at} you clicked the {button} button at {x}, {y}."),
        LogEvent::Key { key, modifiers } => format!("At {at} you pressed {}.", key_phrase(key, modifiers)),
        LogEvent::Window { title, process, bounds } => match bounds {
            Some(b) => format!(
                "At {at} the window in front was \"{title}\" ({process}), {} by {} at {}, {}.",
                b.width, b.height, b.x, b.y
            ),
            None => format!("At {at} the window in front was \"{title}\" ({process})."),
        },
        LogEvent::Element { name, role } => format!("At {at} the item clicked was the {role} \"{name}\"."),
        LogEvent::Marker { label } if label.is_empty() => format!("At {at} you added a marker."),
        LogEvent::Marker { label } => format!("At {at} you added a marker: {label}"),
    }
}

/// Every line as a sentence. The cursor's positions are numerous (about thirty a second while it moves), so they are
/// left out unless `with_moves` is set.
pub fn describe_lines(lines: &[LogLine], with_moves: bool) -> Vec<String> {
    lines.iter().filter(|l| with_moves || !matches!(l.event, LogEvent::CursorMove { .. })).map(describe_line).collect()
}

/// What deleting did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deletion {
    /// Lines removed.
    pub removed: u64,
    /// Lines kept (the author's own markers).
    pub kept: u64,
}

/// Read a take's log. A take that has none (nothing was ever noted, not even a marker) reads as empty.
pub fn read_take_log(project: &Path, take: u32) -> Result<Vec<LogLine>, ProjectError> {
    let path = take_dir(project, take).join("events.jsonl");
    match read_log(&path) {
        Ok(lines) => Ok(lines),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// Delete what was noted of the author's mouse and keyboard in a take: every line of its log except the markers, and
/// the take's own entry for it. Refused while the take is still being recorded (or was cut off before it finished and
/// has not been recovered), because the recorder is still adding to the log, or may have.
pub fn delete_take_interactions(project: &Path, take: u32) -> Result<Deletion, ProjectError> {
    let mut manifest = read_take(project, take)?;
    if manifest.state == TakeState::Recording {
        return Err(ProjectError::BadFormat(format!(
            "Take {take} is still being recorded, or the recorder stopped before it was finished. Let it finish, or repair it with the recover command, and then delete its interaction log."
        )));
    }
    let path = take_dir(project, take).join("events.jsonl");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut kept = String::new();
    let (mut removed, mut kept_count) = (0u64, 0u64);
    for raw in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<LogLine>(raw) {
            Ok(line) if !line.event.is_interaction() => {
                kept.push_str(raw);
                kept.push('\n');
                kept_count += 1;
            }
            _ => removed += 1,
        }
    }
    if path.exists() {
        write_atomic(&path, kept.as_bytes())?;
    }
    // The log no longer holds what its entry says it does.
    let before = manifest.media.len();
    manifest.media.retain(|m| m.kind != "interactions");
    if manifest.media.len() != before {
        manifest.segments = manifest.segments.saturating_sub((before - manifest.media.len()) as u32);
        write_take(project, &manifest)?;
    }
    Ok(Deletion { removed, kept: kept_count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events_log::{EventLogWriter, LogBounds};
    use crate::take::{create_take_with, finish_take, InteractionInfo, MediaFile};

    fn key(k: &str, m: &[&str]) -> LogEvent {
        LogEvent::Key { key: k.into(), modifiers: m.iter().map(|s| s.to_string()).collect() }
    }

    fn sample() -> Vec<LogLine> {
        vec![
            LogLine {
                t: 0,
                event: LogEvent::Window {
                    title: "Untitled - Notepad".into(),
                    process: "notepad.exe".into(),
                    bounds: Some(LogBounds { x: 10, y: 20, width: 800, height: 600 }),
                },
            },
            LogLine { t: 100, event: LogEvent::CursorMove { x: 5, y: 6 } },
            LogLine { t: 1200, event: LogEvent::Click { x: 640, y: 480, button: "left".into() } },
            LogLine { t: 1200, event: LogEvent::Element { name: "Save".into(), role: "button".into() } },
            LogLine { t: 2400, event: LogEvent::Click { x: 640, y: 480, button: "left".into() } },
            LogLine { t: 2400, event: LogEvent::Element { name: "Save".into(), role: "button".into() } },
            LogLine { t: 3000, event: key("s", &["ctrl", "shift"]) },
            LogLine { t: 3500, event: key("enter", &[]) },
            LogLine { t: 4000, event: LogEvent::Marker { label: "look here".into() } },
            LogLine {
                t: 4500,
                event: LogEvent::Window { title: "Browser".into(), process: "browser.exe".into(), bounds: None },
            },
        ]
    }

    #[test]
    fn a_log_is_counted_and_its_windows_and_clicked_items_are_named_once_each() {
        let s = summarize(&sample());
        assert_eq!((s.moves, s.clicks, s.keys, s.windows, s.elements, s.markers, s.last_ms), (1, 2, 2, 2, 2, 1, 4500));
        assert_eq!(s.windows_seen.len(), 2);
        assert_eq!(s.clicked, vec![("Save".to_string(), "button".to_string(), 2)]);
        assert_eq!(s.interaction_lines(), 9);
    }

    #[test]
    fn the_summary_is_in_plain_words_and_does_not_print_the_keys() {
        let text = describe_summary(3, &summarize(&sample()));
        assert!(
            text.starts_with("Take 3 holds a record of what you did for 4.5 seconds: 1 cursor position, 2 clicks"),
            "{text}"
        );
        assert!(text.contains("2 key presses") && text.contains("and 1 marker of your own"), "{text}");
        assert!(text.contains("\"Untitled - Notepad\" (notepad.exe); \"Browser\" (browser.exe)"), "{text}");
        assert!(text.contains("Only the first title of each program is shown"), "{text}");
        assert!(text.contains("the button \"Save\" (2 times)"), "{text}");
        assert!(text.contains("The keys are not printed here; use --list"), "{text}");
        assert!(!text.contains("enter") && !text.contains("Ctrl"), "the typed keys are not in the summary: {text}");
        assert!(!text.contains('\u{2014}'));
    }

    #[test]
    fn a_log_with_only_markers_says_nothing_was_noted_and_keeps_the_markers_in_view() {
        let lines = vec![LogLine { t: 5, event: LogEvent::Marker { label: "x".into() } }];
        let text = describe_summary(1, &summarize(&lines));
        assert_eq!(
            text,
            "Take 1 holds no record of what you did with the mouse and keyboard. It holds 1 marker of your own."
        );
        assert_eq!(
            describe_summary(2, &LogSummary::default()),
            "Take 2 holds no record of what you did with the mouse and keyboard."
        );
    }

    #[test]
    fn a_long_list_of_programs_is_cut_short_and_says_how_many_more() {
        let lines: Vec<LogLine> = (0..20)
            .map(|i| LogLine {
                t: i,
                event: LogEvent::Window { title: format!("w{i}"), process: format!("p{i}.exe"), bounds: None },
            })
            .collect();
        let text = describe_summary(1, &summarize(&lines));
        assert!(text.contains("\"w11\" (p11.exe); and 8 more programs."), "{text}");
        assert!(!text.contains("w12"), "{text}");
    }

    #[test]
    fn a_program_whose_title_keeps_changing_is_shown_once_so_what_was_typed_is_not_printed() {
        // Notepad shows the first words of what is typed in its title.
        let lines: Vec<LogLine> =
            ["Untitled - Notepad", "*He - Notepad", "*Hello - Notepad", "*Hello secret - Notepad"]
                .iter()
                .enumerate()
                .map(|(i, title)| LogLine {
                    t: i as u64,
                    event: LogEvent::Window { title: title.to_string(), process: "notepad.exe".into(), bounds: None },
                })
                .collect();
        let text = describe_summary(1, &summarize(&lines));
        assert!(text.contains("\"Untitled - Notepad\" and 3 other titles (notepad.exe)"), "{text}");
        assert!(!text.contains("secret") && !text.contains("Hello"), "{text}");
        // One other title is "1 other title".
        let two = &lines[..2];
        assert!(describe_summary(1, &summarize(two)).contains("and 1 other title (notepad.exe)"));
        // The full list still shows everything, for the person who asks.
        assert!(describe_lines(&lines, false).iter().any(|l| l.contains("secret")));
    }

    #[test]
    fn every_line_can_be_read_as_a_sentence() {
        let words: Vec<String> = describe_lines(&sample(), true);
        assert_eq!(words.len(), 10);
        assert_eq!(
            words[0],
            "At 0 seconds the window in front was \"Untitled - Notepad\" (notepad.exe), 800 by 600 at 10, 20."
        );
        assert_eq!(words[1], "At 0.1 seconds the cursor was at 5, 6.");
        assert_eq!(words[2], "At 1.2 seconds you clicked the left button at 640, 480.");
        assert_eq!(words[3], "At 1.2 seconds the item clicked was the button \"Save\".");
        assert_eq!(words[6], "At 3 seconds you pressed Ctrl+Shift+s.");
        assert_eq!(words[7], "At 3.5 seconds you pressed enter.");
        assert_eq!(words[8], "At 4 seconds you added a marker: look here");
        assert_eq!(words[9], "At 4.5 seconds the window in front was \"Browser\" (browser.exe).");
        assert_eq!(
            describe_line(&LogLine { t: 1000, event: LogEvent::Marker { label: String::new() } }),
            "At 1 second you added a marker."
        );
    }

    #[test]
    fn cursor_positions_are_left_out_of_a_list_unless_asked_for() {
        assert_eq!(describe_lines(&sample(), false).len(), 9);
        assert_eq!(describe_lines(&sample(), true).len(), 10);
    }

    fn take_with_log(dir: &Path, lines: &[LogLine], finished: bool) {
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
                file: "events.jsonl".into(),
                kind: "interactions".into(),
                container: "jsonl".into(),
                mix: None,
                region: None,
                camera: None,
                interactions: Some(InteractionInfo { display: 1, keys: true }),
            },
        ];
        crate::manifest::create_project(dir, "t", "en-US").unwrap();
        create_take_with(dir, 1, vec!["screen:1".into()], None, 2000, media).unwrap();
        let mut w = EventLogWriter::create(&take_dir(dir, 1).join("events.jsonl")).unwrap();
        for l in lines {
            w.append(l.t, l.event.clone()).unwrap();
        }
        w.flush_sync().unwrap();
        if finished {
            finish_take(dir, 1, 5000, 2).unwrap();
        }
    }

    #[test]
    fn deleting_removes_everything_that_records_what_the_author_did_and_keeps_the_markers() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &sample(), true);
        let outcome = delete_take_interactions(dir.path(), 1).unwrap();
        assert_eq!(outcome, Deletion { removed: 9, kept: 1 });
        let left = read_take_log(dir.path(), 1).unwrap();
        assert_eq!(left, vec![LogLine { t: 4000, event: LogEvent::Marker { label: "look here".into() } }]);
        let text = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        for private in ["notepad", "Save", "ctrl", "enter", "640"] {
            assert!(!text.contains(private), "{private} is still in the file: {text}");
        }
    }

    #[test]
    fn deleting_also_takes_the_entry_for_the_log_out_of_the_takes_file() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &sample(), true);
        delete_take_interactions(dir.path(), 1).unwrap();
        let take = read_take(dir.path(), 1).unwrap();
        assert!(take.media.iter().all(|m| m.kind != "interactions"));
        assert_eq!(take.media.len(), 1, "the video is still listed");
        assert_eq!(take.segments, 1);
        let text = fs::read_to_string(take_dir(dir.path(), 1).join("take.json")).unwrap();
        assert!(!text.contains("interactions"), "{text}");
    }

    #[test]
    fn a_line_that_cannot_be_read_is_removed_too_because_it_may_be_a_click() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &sample()[..2], true);
        let path = take_dir(dir.path(), 1).join("events.jsonl");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("{\"t\":9,\"type\":\"click\",\"x\":1,\"y\":2,\"but");
        fs::write(&path, text).unwrap();
        let outcome = delete_take_interactions(dir.path(), 1).unwrap();
        assert_eq!(outcome, Deletion { removed: 3, kept: 0 });
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn a_take_still_being_recorded_is_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &sample(), false);
        let before = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        let e = delete_take_interactions(dir.path(), 1).unwrap_err().to_string();
        assert!(e.contains("still being recorded") && e.contains("recover"), "{e}");
        assert_eq!(fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap(), before);
    }

    #[test]
    fn a_take_with_no_log_reads_as_empty_and_deleting_it_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &[], true);
        fs::remove_file(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        assert!(read_take_log(dir.path(), 1).unwrap().is_empty());
        assert_eq!(delete_take_interactions(dir.path(), 1).unwrap(), Deletion { removed: 0, kept: 0 });
        assert!(!take_dir(dir.path(), 1).join("events.jsonl").exists(), "no empty file is made");
    }

    #[test]
    fn deleting_twice_changes_nothing_the_second_time() {
        let dir = tempfile::tempdir().unwrap();
        take_with_log(dir.path(), &sample(), true);
        delete_take_interactions(dir.path(), 1).unwrap();
        assert_eq!(delete_take_interactions(dir.path(), 1).unwrap(), Deletion { removed: 0, kept: 1 });
        assert_eq!(read_take(dir.path(), 1).unwrap().segments, 1);
    }
}
