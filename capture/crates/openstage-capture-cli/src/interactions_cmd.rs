// SPDX-License-Identifier: Apache-2.0
//! The `interactions` command: read back, in words, what was noted of the author's mouse and keyboard (story A6), and
//! delete it. Nothing here records anything.

use openstage_capture_project::{
    delete_take_interactions, describe_lines, describe_summary, list_takes, open_project, read_take_log, summarize_log,
};
use openstage_capture_windows::Check;
use std::path::PathBuf;

/// What the command should do.
pub struct InteractionsOptions {
    pub project: PathBuf,
    /// One take, or every take of the project.
    pub take: Option<u32>,
    /// Print every line of the log as a sentence, not only the counts.
    pub list: bool,
    /// With `list`: the cursor's positions too (about thirty a second while it moves).
    pub with_moves: bool,
    /// Delete the log of what the author did (the markers are kept).
    pub delete: bool,
    /// Really delete. Without it, `delete` only says what it would remove.
    pub yes: bool,
}

fn say(report: &mut dyn FnMut(Check), name: &str, ok: bool, message: impl Into<String>) {
    report(Check { check: name.to_string(), ok, message: message.into() });
}

/// Run the command. Returns whether everything it was asked to do was done.
pub fn run_interactions(opts: &InteractionsOptions, report: &mut dyn FnMut(Check)) -> bool {
    if let Err(e) = open_project(&opts.project) {
        say(
            report,
            "interactions",
            false,
            format!("Could not open the recording folder {}: {e}", opts.project.display()),
        );
        return false;
    }
    let takes: Vec<u32> = match opts.take {
        Some(id) => vec![id],
        None => match list_takes(&opts.project) {
            Ok(list) => list.into_iter().map(|t| t.id).collect(),
            Err(e) => {
                say(report, "interactions", false, format!("Could not read the takes: {e}"));
                return false;
            }
        },
    };
    if takes.is_empty() {
        say(report, "interactions", false, "This recording folder holds no takes.");
        return false;
    }
    let mut all_fine = true;
    let mut pending = Vec::new();
    for id in takes {
        let lines = match read_take_log(&opts.project, id) {
            Ok(lines) => lines,
            Err(e) => {
                say(report, "interactions-log", false, format!("Could not read the log of take {id}: {e}"));
                all_fine = false;
                continue;
            }
        };
        let summary = summarize_log(&lines);
        say(report, "interactions-log", true, describe_summary(id, &summary));
        if opts.list {
            let words = describe_lines(&lines, opts.with_moves);
            if !words.is_empty() {
                say(report, "interactions-list", true, format!("Take {id}, line by line:\n{}", words.join("\n")));
            }
        }
        if opts.delete && summary.interaction_lines() > 0 {
            pending.push((id, summary.interaction_lines()));
        } else if opts.delete {
            say(report, "interactions-delete", true, format!("Take {id} holds nothing to delete."));
        }
    }
    if !pending.is_empty() {
        if !opts.yes {
            let which: Vec<String> = pending
                .iter()
                .map(|(id, n)| format!("take {id} ({n} line{})", if *n == 1 { "" } else { "s" }))
                .collect();
            say(
                report,
                "interactions-delete",
                false,
                format!(
                    "Nothing was deleted. Deleting would remove what was noted of your mouse and keyboard from: {}. Your own markers would be kept. Deleting cannot be undone. To go ahead, run the same command again with --yes added.",
                    which.join(", ")
                ),
            );
            return false;
        }
        for (id, _) in pending {
            match delete_take_interactions(&opts.project, id) {
                Ok(d) => say(
                    report,
                    "interactions-delete",
                    true,
                    format!(
                        "Deleted {} line{} from the log of take {id}. {} of your own marker{} kept. The take's video and sound were not touched.",
                        d.removed,
                        if d.removed == 1 { "" } else { "s" },
                        d.kept,
                        if d.kept == 1 { " was" } else { "s were" }
                    ),
                ),
                Err(e) => {
                    say(report, "interactions-delete", false, format!("Take {id}: {e}"));
                    all_fine = false;
                }
            }
        }
    }
    all_fine
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstage_capture_project::{
        create_project, create_take_with, finish_take, read_take, take_dir, EventLogWriter, InteractionInfo, LogEvent,
        MediaFile,
    };
    use std::fs;

    fn project_with_two_takes(dir: &std::path::Path) {
        create_project(dir, "t", "en-US").unwrap();
        for id in 1..=2 {
            let media = vec![MediaFile {
                file: "events.jsonl".into(),
                kind: "interactions".into(),
                container: "jsonl".into(),
                mix: None,
                region: None,
                camera: None,
                interactions: Some(InteractionInfo { display: 1, keys: true }),
            }];
            create_take_with(dir, id, vec!["screen:1".into()], None, 2000, media).unwrap();
            let mut w = EventLogWriter::create(&take_dir(dir, id).join("events.jsonl")).unwrap();
            w.append(10, LogEvent::Click { x: 1, y: 2, button: "left".into() }).unwrap();
            w.append(11, LogEvent::Element { name: format!("Button {id}"), role: "button".into() }).unwrap();
            w.append(20, LogEvent::Key { key: "q".into(), modifiers: vec![] }).unwrap();
            w.append(30, LogEvent::CursorMove { x: 4, y: 5 }).unwrap();
            w.append(40, LogEvent::Marker { label: "mine".into() }).unwrap();
            w.flush_sync().unwrap();
            finish_take(dir, id, 1000, 1).unwrap();
        }
    }

    fn run(opts: InteractionsOptions) -> (bool, Vec<Check>) {
        let mut checks = Vec::new();
        let ok = run_interactions(&opts, &mut |c| checks.push(c));
        (ok, checks)
    }

    fn options(dir: &std::path::Path) -> InteractionsOptions {
        InteractionsOptions {
            project: dir.to_path_buf(),
            take: None,
            list: false,
            with_moves: false,
            delete: false,
            yes: false,
        }
    }

    #[test]
    fn every_take_is_summarised_in_words_and_the_keys_are_not_printed() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        let (ok, checks) = run(options(dir.path()));
        assert!(ok);
        assert_eq!(checks.len(), 2);
        assert!(checks[0].message.starts_with("Take 1 holds a record of what you did"), "{}", checks[0].message);
        assert!(checks[1].message.contains("Button 2"), "{}", checks[1].message);
        assert!(!checks.iter().any(|c| c.message.contains("\"q\"") || c.message.contains("pressed q")));
    }

    #[test]
    fn one_take_can_be_asked_for_and_listed_line_by_line_with_or_without_the_cursor() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        let (_, checks) = run(InteractionsOptions { take: Some(2), list: true, ..options(dir.path()) });
        assert_eq!(checks.len(), 2, "the summary and the list");
        let list = &checks[1].message;
        assert!(list.starts_with("Take 2, line by line:\n"), "{list}");
        assert!(
            list.contains("you pressed q") && list.contains("Button 2") && list.contains("a marker: mine"),
            "{list}"
        );
        assert!(!list.contains("the cursor was at"), "{list}");
        let (_, checks) =
            run(InteractionsOptions { take: Some(2), list: true, with_moves: true, ..options(dir.path()) });
        assert!(checks[1].message.contains("the cursor was at 4, 5"));
    }

    #[test]
    fn deleting_without_yes_only_says_what_it_would_do_and_fails() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        let before = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        let (ok, checks) = run(InteractionsOptions { delete: true, ..options(dir.path()) });
        assert!(!ok);
        let said = checks.last().unwrap();
        assert_eq!(said.check, "interactions-delete");
        assert!(
            said.message.contains("Nothing was deleted")
                && said.message.contains("take 1 (4 lines), take 2 (4 lines)")
                && said.message.contains("--yes"),
            "{}",
            said.message
        );
        assert_eq!(fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap(), before);
    }

    #[test]
    fn deleting_with_yes_removes_what_was_noted_from_the_chosen_take_only() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        let (ok, checks) = run(InteractionsOptions { take: Some(1), delete: true, yes: true, ..options(dir.path()) });
        assert!(ok, "{checks:?}");
        assert!(
            checks
                .last()
                .unwrap()
                .message
                .contains("Deleted 4 lines from the log of take 1. 1 of your own marker was kept"),
            "{}",
            checks.last().unwrap().message
        );
        let text = fs::read_to_string(take_dir(dir.path(), 1).join("events.jsonl")).unwrap();
        assert!(text.contains("mine") && !text.contains("Button 1") && !text.contains("\"q\""), "{text}");
        let other = fs::read_to_string(take_dir(dir.path(), 2).join("events.jsonl")).unwrap();
        assert!(other.contains("Button 2"), "the other take is untouched");
        assert!(read_take(dir.path(), 1).unwrap().media.is_empty());
        assert_eq!(read_take(dir.path(), 2).unwrap().media.len(), 1);
    }

    #[test]
    fn deleting_a_take_that_holds_nothing_says_so_and_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        run(InteractionsOptions { take: Some(1), delete: true, yes: true, ..options(dir.path()) });
        let (ok, checks) = run(InteractionsOptions { take: Some(1), delete: true, yes: true, ..options(dir.path()) });
        assert!(ok);
        assert!(checks.last().unwrap().message.contains("holds nothing to delete"));
    }

    #[test]
    fn a_folder_that_is_not_a_recording_or_has_no_takes_is_explained() {
        let dir = tempfile::tempdir().unwrap();
        let (ok, checks) = run(options(dir.path()));
        assert!(!ok);
        assert!(checks[0].message.contains("Could not open the recording folder"), "{}", checks[0].message);
        create_project(&dir.path().join("p"), "t", "en-US").unwrap();
        let (ok, checks) = run(options(&dir.path().join("p")));
        assert!(!ok);
        assert_eq!(checks[0].message, "This recording folder holds no takes.");
    }

    #[test]
    fn a_take_that_does_not_exist_is_a_problem_in_words() {
        let dir = tempfile::tempdir().unwrap();
        project_with_two_takes(dir.path());
        let (ok, checks) = run(InteractionsOptions { take: Some(9), delete: true, yes: true, ..options(dir.path()) });
        // Take 9 has no log, which reads as empty; there is nothing to delete.
        assert!(ok);
        assert!(checks.iter().any(|c| c.message.contains("holds no record")), "{checks:?}");
    }
}
