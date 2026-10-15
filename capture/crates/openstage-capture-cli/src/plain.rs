// SPDX-License-Identifier: Apache-2.0
//! Events as sentences, for a person reading a console or listening to a screen reader.
//!
//! The protocol is for programs (one JSON object per line). `--plain` is for people: the same
//! events, said in words, with the once-a-second repeats of a state left out.

use openstage_capture::EventSink;
use openstage_capture_protocol::{Event, State};
use std::collections::HashMap;
use std::io::{self, Write};

fn secs(ms: u64) -> String {
    format!("{:.1} seconds", ms as f64 / 1000.0)
}

/// Prints events as sentences.
#[derive(Default)]
pub struct PlainSink {
    last_state: Option<(State, u32)>,
    last_level_text: HashMap<String, String>,
    /// When, in recording time, each sound's level was last said.
    last_level_said_ms: HashMap<String, u64>,
    /// Advice already said as a note (`balance`, `echo`), so its warning is not said a second time.
    advice_said: Vec<String>,
}

/// Findings that are advice for the author, not faults of the recorder. They never change the verdict of
/// a recording, so they are not called a PROBLEM; a line saying PROBLEM above a verdict of PASS only confuses.
const ADVICE_CHECKS: [&str; 3] = ["balance", "echo", "export-note"];

/// The least recording time between two spoken levels of the same sound, in milliseconds.
const LEVEL_EVERY_MS: u64 = 5000;

impl PlainSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// The sentence for an event, or `None` if it adds nothing (a repeat, or a greeting).
    pub fn describe(&mut self, event: &Event) -> Option<String> {
        match event {
            Event::Hello { .. } | Event::Probe { .. } | Event::Recovered { .. } => None,
            Event::State { state, elapsed_ms, take } => {
                if self.last_state == Some((*state, *take)) {
                    return None;
                }
                let was_paused = self.last_state == Some((State::Paused, *take));
                self.last_state = Some((*state, *take));
                match state {
                    State::Idle => None,
                    State::Countdown => Some("Get ready: the countdown has started.".to_string()),
                    State::Recording if was_paused => {
                        Some("Resumed. Recording take ".to_string() + &take.to_string() + " again.")
                    }
                    State::Recording => Some(format!("Recording take {take}.")),
                    State::Paused => {
                        Some(format!("Paused at {}. Time spent paused is not recorded.", secs(*elapsed_ms)))
                    }
                    State::Stopping => Some("Stopping, and finishing the video file.".to_string()),
                    State::Stopped => Some(format!("Stopped. Take {take} is finished.")),
                    State::Error => Some("The recorder stopped because of a problem.".to_string()),
                }
            }
            Event::Level { source, text, elapsed_ms, .. } => {
                // A sound level is said when it changes, and then at most once every few seconds for each
                // sound, so that a person (or a screen reader) is not talked over by a meter that flickers.
                // The one exception is a sound that is too loud, which is said at once.
                let changed = self.last_level_text.get(source).map(String::as_str) != Some(text.as_str());
                let waited_enough = match self.last_level_said_ms.get(source) {
                    Some(said_at) => *elapsed_ms < *said_at || *elapsed_ms - *said_at >= LEVEL_EVERY_MS,
                    None => true,
                };
                if !changed || !(waited_enough || text == "too loud") {
                    return None;
                }
                self.last_level_text.insert(source.clone(), text.clone());
                self.last_level_said_ms.insert(source.clone(), *elapsed_ms);
                let name = match source.as_str() {
                    "mic" => "microphone",
                    "system" => "the computer's sound",
                    other => other,
                };
                Some(format!("Sound level, {name}: {text}."))
            }
            Event::Warning { code, message } => {
                // The same advice was said a moment ago as a note; hearing it twice only wastes the listener's time.
                let topic = match code.as_str() {
                    "balance-off" => "balance",
                    "echo-likely" => "echo",
                    _ => "",
                };
                if !topic.is_empty() && self.advice_said.iter().any(|said| said == topic) {
                    return None;
                }
                Some(format!("Warning: {message}"))
            }
            Event::Error { message, .. } => Some(format!("PROBLEM: {message}")),
            Event::Marker { label, elapsed_ms } => Some(if label.is_empty() {
                format!("Marker added at {}.", secs(*elapsed_ms))
            } else {
                format!("Marker added at {}: {label}", secs(*elapsed_ms))
            }),
            Event::TakeSetAside { take, next_take } => Some(format!(
                "Start again: take {take} was set aside (it is kept, not erased). Beginning take {next_take}."
            )),
            Event::Check { check, ok, message } => {
                if !*ok && ADVICE_CHECKS.contains(&check.as_str()) {
                    self.advice_said.push(check.clone());
                    return Some(format!("Note: {message}"));
                }
                Some(format!("{}: {message}", if *ok { "OK" } else { "PROBLEM" }))
            }
            Event::Progress { message, .. } => Some(message.clone()),
            Event::Result { ok, project, take, state, duration_ms, segments } => Some(if *take == 0 {
                format!("Finished. No take was recorded. Project folder: {project}")
            } else {
                format!(
                    "Finished{}. Take {take} is {state}: {}, {segments} media file(s). Project folder: {project}",
                    if *ok { "" } else { " with a problem" },
                    secs(*duration_ms)
                )
            }),
        }
    }
}

impl EventSink for PlainSink {
    fn emit(&mut self, event: Event) -> bool {
        match self.describe(&event) {
            Some(line) => {
                let mut out = io::stdout().lock();
                writeln!(out, "{line}").and_then(|_| out.flush()).is_ok()
            }
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(state: State, elapsed_ms: u64, take: u32) -> Event {
        Event::State { state, elapsed_ms, take }
    }

    #[test]
    fn a_state_is_said_once_not_every_second() {
        let mut p = PlainSink::new();
        assert_eq!(p.describe(&state(State::Recording, 0, 1)).as_deref(), Some("Recording take 1."));
        assert_eq!(p.describe(&state(State::Recording, 1000, 1)), None);
        assert_eq!(p.describe(&state(State::Recording, 2000, 1)), None);
        assert_eq!(
            p.describe(&state(State::Paused, 2500, 1)).as_deref(),
            Some("Paused at 2.5 seconds. Time spent paused is not recorded.")
        );
        assert_eq!(p.describe(&state(State::Recording, 2500, 1)).as_deref(), Some("Resumed. Recording take 1 again."));
        // A new take with the same state is news again.
        assert_eq!(p.describe(&state(State::Recording, 0, 2)).as_deref(), Some("Recording take 2."));
    }

    #[test]
    fn everything_a_person_needs_to_hear_is_in_words() {
        let mut p = PlainSink::new();
        assert_eq!(
            p.describe(&Event::Marker { label: "look here".into(), elapsed_ms: 7000 }).as_deref(),
            Some("Marker added at 7.0 seconds: look here")
        );
        assert_eq!(
            p.describe(&Event::Marker { label: String::new(), elapsed_ms: 100 }).as_deref(),
            Some("Marker added at 0.1 seconds.")
        );
        assert_eq!(
            p.describe(&Event::TakeSetAside { take: 1, next_take: 2 }).as_deref(),
            Some("Start again: take 1 was set aside (it is kept, not erased). Beginning take 2.")
        );
        assert_eq!(
            p.describe(&Event::Error {
                code: "media-failed".into(),
                message: "No picture arrived.".into(),
                fatal: true
            })
            .as_deref(),
            Some("PROBLEM: No picture arrived.")
        );
        assert_eq!(
            p.describe(&Event::Warning {
                code: "not-paused".into(),
                message: "Resume only works while paused.".into()
            })
            .as_deref(),
            Some("Warning: Resume only works while paused.")
        );
        let done = Event::Result {
            ok: true,
            project: "lesson.capture".into(),
            take: 1,
            state: "complete".into(),
            duration_ms: 7400,
            segments: 1,
        };
        assert_eq!(
            p.describe(&done).as_deref(),
            Some("Finished. Take 1 is complete: 7.4 seconds, 1 media file(s). Project folder: lesson.capture")
        );
    }

    #[test]
    fn advice_about_balance_and_echo_is_a_note_said_once_and_a_real_fault_is_a_problem() {
        let mut p = PlainSink::new();
        let check = |name: &str, ok: bool| Event::Check { check: name.into(), ok, message: format!("{name} words") };
        let warning = |code: &str| Event::Warning { code: code.into(), message: format!("{code} words") };
        // Advice that found something: a note, then its warning is not said again.
        assert_eq!(p.describe(&check("balance", false)).as_deref(), Some("Note: balance words"));
        assert_eq!(p.describe(&warning("balance-off")), None);
        assert_eq!(p.describe(&check("echo", false)).as_deref(), Some("Note: echo words"));
        assert_eq!(p.describe(&warning("echo-likely")), None);
        // Advice that found nothing is still OK, and other warnings are still said.
        assert_eq!(p.describe(&check("balance", true)).as_deref(), Some("OK: balance words"));
        assert_eq!(p.describe(&warning("slow-disk")).as_deref(), Some("Warning: slow-disk words"));
        // A check that is a fault of the recorder is a problem.
        assert_eq!(p.describe(&check("video-readback", false)).as_deref(), Some("PROBLEM: video-readback words"));
        // With no note said first, a warning is not dropped.
        let mut fresh = PlainSink::new();
        assert_eq!(fresh.describe(&warning("echo-likely")).as_deref(), Some("Warning: echo-likely words"));
    }

    fn level_at(source: &str, text: &str, elapsed_ms: u64) -> Event {
        Event::Level { source: source.into(), peak: 0.5, text: text.into(), elapsed_ms }
    }

    #[test]
    fn a_steady_sound_level_is_said_only_when_it_changes() {
        let mut p = PlainSink::new();
        assert!(p.describe(&level_at("mic", "good", 0)).is_some());
        assert!(p.describe(&level_at("mic", "good", 6000)).is_none());
        assert_eq!(p.describe(&level_at("mic", "loud", 12_000)).as_deref(), Some("Sound level, microphone: loud."));
    }

    #[test]
    fn a_flickering_level_is_said_at_most_once_every_few_seconds_for_each_sound() {
        let mut p = PlainSink::new();
        assert!(p.describe(&level_at("mic", "quiet", 500)).is_some());
        // The meter flickers between quiet and silent twice a second: nothing more is said for 5 seconds.
        for (i, text) in ["silent", "quiet", "silent", "quiet", "silent"].iter().enumerate() {
            assert!(p.describe(&level_at("mic", text, 1000 + i as u64 * 500)).is_none(), "{text} at step {i}");
        }
        assert_eq!(p.describe(&level_at("mic", "good", 5600)).as_deref(), Some("Sound level, microphone: good."));
        // The other sound has its own clock.
        assert!(p.describe(&level_at("system", "loud", 5700)).is_some());
    }

    #[test]
    fn a_sound_that_is_too_loud_is_said_at_once() {
        let mut p = PlainSink::new();
        assert!(p.describe(&level_at("mic", "good", 1000)).is_some());
        assert_eq!(
            p.describe(&level_at("mic", "too loud", 1500)).as_deref(),
            Some("Sound level, microphone: too loud.")
        );
    }

    #[test]
    fn a_session_that_stopped_before_recording_says_so() {
        let mut p = PlainSink::new();
        let none =
            Event::Result { ok: true, project: "p".into(), take: 0, state: "none".into(), duration_ms: 0, segments: 0 };
        assert_eq!(p.describe(&none).as_deref(), Some("Finished. No take was recorded. Project folder: p"));
    }
}
