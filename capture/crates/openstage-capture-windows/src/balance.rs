// SPDX-License-Identifier: Apache-2.0
//! Noticing when the microphone and the computer's sound are very different in loudness.
//!
//! A voice that is quieter than the computer's sound is hard to follow, and for some listeners it
//! cannot be followed at all. So after a recording, the program measures how loud each sound was while
//! it was actually making sound (not counting the pauses), compares them, and says in plain words
//! what it found and what would help. The separate sound files make any balance fixable afterwards;
//! this only says how far apart they are, and what to change if the mix inside the video should be
//! different.
//!
//! The numbers are first estimates: a voice is wanted about 6 decibels louder than the computer's
//! sound, and no more than 3 decibels short of that is called fine.

use crate::echo::{envelope_of_file, Envelope};
use crate::{check, Check};
use std::path::Path;

/// How much sound is needed to say anything: 1 second of each.
const MIN_ACTIVE_WINDOWS: usize = 50;
/// The voice should be at least this many decibels louder than the computer's sound.
const MIN_VOICE_MARGIN_DB: f32 = 3.0;
/// ...and is best about this much louder.
const TARGET_VOICE_MARGIN_DB: f32 = 6.0;
/// The computer's sound is "much quieter" than the voice beyond this.
const MUCH_QUIETER_DB: f32 = 24.0;

/// How loud a track is while it is making sound, from readings of its loudness: the middle value of
/// the readings that are clearly above silence. `None` if there is too little sound to say.
pub fn typical_level(envelope: &[f32]) -> Option<f32> {
    if envelope.is_empty() {
        return None;
    }
    let mut sorted = envelope.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p90 = sorted[(sorted.len() * 9 / 10).min(sorted.len() - 1)];
    let threshold = (0.1 * p90).max(0.003);
    let active: Vec<f32> = sorted.into_iter().filter(|v| *v > threshold).collect();
    if active.len() < MIN_ACTIVE_WINDOWS {
        return None;
    }
    Some(active[active.len() / 2])
}

fn db(level: f32) -> f32 {
    20.0 * level.max(1e-6).log10()
}

/// What was found.
#[derive(Debug, Clone, PartialEq)]
pub enum BalanceVerdict {
    /// The voice is comfortably louder than the computer's sound.
    Good { voice_above_db: f32 },
    /// The computer's sound is as loud as the voice, or nearly.
    ComputerTooLoud {
        /// How much louder the computer's sound is than the voice (negative if it is a little quieter).
        louder_by_db: f32,
        /// A change to the computer's sound in the mix, in decibels (negative), that puts the voice about
        /// 6 decibels above it.
        suggested_system_gain_db: i32,
    },
    /// The computer's sound is far quieter than the voice.
    ComputerVeryQuiet { voice_above_db: f32 },
    /// One of them did not make enough sound to compare. Says which.
    CannotTell(String),
}

/// Compare the loudness of the microphone and of the computer's sound, from readings of each.
pub fn judge_balance(mic: &[f32], system: &[f32]) -> BalanceVerdict {
    let (Some(voice), Some(computer)) = (typical_level(mic), typical_level(system)) else {
        let which = match (typical_level(mic).is_some(), typical_level(system).is_some()) {
            (false, false) => "neither the microphone nor the computer made enough sound",
            (false, true) => "the microphone made too little sound",
            _ => "the computer made too little sound",
        };
        return BalanceVerdict::CannotTell(format!("{which} (at least 1 second of each is needed)"));
    };
    let above = db(voice) - db(computer);
    if above < MIN_VOICE_MARGIN_DB {
        let wanted = (TARGET_VOICE_MARGIN_DB - above).round() as i32;
        BalanceVerdict::ComputerTooLoud { louder_by_db: -above, suggested_system_gain_db: -wanted.clamp(1, 40) }
    } else if above > MUCH_QUIETER_DB {
        BalanceVerdict::ComputerVeryQuiet { voice_above_db: above }
    } else {
        BalanceVerdict::Good { voice_above_db: above }
    }
}

/// What to tell the author.
#[derive(Debug, Clone)]
pub struct BalanceFinding {
    /// The finding, named `balance`. `ok` is false only when the computer's sound is too loud.
    pub check: Check,
    /// A warning for the author, when the voice may be hard to hear.
    pub warning: Option<String>,
}

/// Turn a verdict into words.
pub fn describe_balance(verdict: &BalanceVerdict) -> BalanceFinding {
    match verdict {
        BalanceVerdict::Good { voice_above_db } => BalanceFinding {
            check: check(
                "balance",
                true,
                format!(
                    "Your voice is about {} decibels louder than the computer's sound while each is making sound. That is a good balance for narration.",
                    voice_above_db.round()
                ),
            ),
            warning: None,
        },
        BalanceVerdict::ComputerTooLoud { louder_by_db, suggested_system_gain_db } => {
            let how = if *louder_by_db >= 1.0 {
                format!("about {} decibels louder than your voice", louder_by_db.round())
            } else if *louder_by_db > -1.0 {
                "about as loud as your voice".to_string()
            } else {
                format!("only {} decibels quieter than your voice", louder_by_db.abs().round())
            };
            let cut = suggested_system_gain_db.abs();
            let advice = format!(
                "To put your voice about 6 decibels above it, make the computer's sound {cut} decibels quieter in the mix (record with --system-gain-db -{cut}), or balance mic.wav and system.wav in the editor: the separate files are not changed by the mix."
            );
            BalanceFinding {
                check: check(
                    "balance",
                    false,
                    format!(
                        "The computer's sound is {how} while each is making sound, so in the mixed video your voice may be hard to hear. {advice}"
                    ),
                ),
                warning: Some(format!("Balance: the computer's sound is {how}. {advice}")),
            }
        }
        BalanceVerdict::ComputerVeryQuiet { voice_above_db } => BalanceFinding {
            check: check(
                "balance",
                true,
                format!(
                    "The computer's sound is about {} decibels quieter than your voice, which may be too quiet to hear in the mixed video. If it should be heard, raise it in the editor, or record with a higher --system-gain-db.",
                    voice_above_db.round()
                ),
            ),
            warning: None,
        },
        BalanceVerdict::CannotTell(why) => BalanceFinding {
            check: check(
                "balance",
                true,
                format!("Could not compare how loud your voice and the computer's sound are: {why}."),
            ),
            warning: None,
        },
    }
}

/// Loudness readings of sound held in memory, for the sound test.
pub fn balance_of_samples(mic: &[i16], system: &[i16], sample_rate: u32, channels: u16) -> BalanceFinding {
    let envelope = |samples: &[i16]| {
        let mut e = Envelope::new(sample_rate, channels);
        e.push(samples);
        e.finish()
    };
    describe_balance(&judge_balance(&envelope(mic), &envelope(system)))
}

/// Compare a take's two saved sound files.
pub fn check_balance_in_files(mic: &Path, system: &Path) -> Result<BalanceFinding, String> {
    let mic = envelope_of_file(mic).map_err(|e| format!("The microphone file could not be read: {e}"))?;
    let system = envelope_of_file(system).map_err(|e| format!("The computer's sound file could not be read: {e}"))?;
    Ok(describe_balance(&judge_balance(&mic, &system)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A track that makes sound at about `level` for `windows` readings, a little uneven.
    fn about(level: f32, windows: usize) -> Vec<f32> {
        (0..windows).map(|i| level * (0.9 + 0.2 * ((i * 7 % 10) as f32 / 10.0))).collect()
    }

    fn with_pauses(level: f32, windows: usize) -> Vec<f32> {
        // Talking for 30 readings, then 20 of silence, over and over.
        (0..windows).map(|i| if i % 50 < 30 { level } else { 0.0005 }).collect()
    }

    #[test]
    fn a_computer_that_is_much_louder_than_the_voice_gets_a_number_to_try() {
        // The owner's case: the microphone about 0.05, the computer's sound about 0.2: 12 decibels apart.
        match judge_balance(&about(0.05, 400), &about(0.2, 400)) {
            BalanceVerdict::ComputerTooLoud { louder_by_db, suggested_system_gain_db } => {
                assert!((11.0..=13.0).contains(&louder_by_db), "{louder_by_db}");
                assert!((-19..=-17).contains(&suggested_system_gain_db), "{suggested_system_gain_db}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_voice_comfortably_above_the_computer_is_a_good_balance() {
        match judge_balance(&about(0.1, 400), &about(0.03, 400)) {
            BalanceVerdict::Good { voice_above_db } => {
                assert!((9.0..=12.0).contains(&voice_above_db), "{voice_above_db}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_voice_only_slightly_above_the_computer_still_gets_advice() {
        match judge_balance(&about(0.1, 400), &about(0.08, 400)) {
            BalanceVerdict::ComputerTooLoud { louder_by_db, suggested_system_gain_db } => {
                assert!(louder_by_db < 0.0, "the computer is a little quieter: {louder_by_db}");
                // 0.1 against 0.08 is 2 decibels; to get 6 decibels of room it must come down about 4.
                assert!((-5..=-3).contains(&suggested_system_gain_db), "{suggested_system_gain_db}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_much_quieter_computer_is_noted_not_blamed() {
        assert!(matches!(
            judge_balance(&about(0.5, 400), &about(0.006, 400)),
            BalanceVerdict::ComputerVeryQuiet { .. }
        ));
    }

    #[test]
    fn pauses_do_not_count_as_quiet() {
        // A voice that pauses a lot is as loud as the same voice that does not.
        let talking = typical_level(&with_pauses(0.1, 500)).unwrap();
        let steady = typical_level(&about(0.1, 500)).unwrap();
        assert!((talking / steady - 1.0).abs() < 0.12, "{talking} against {steady}");
    }

    #[test]
    fn too_little_sound_means_it_says_which_one_and_why() {
        let quiet = vec![0.0005f32; 400];
        match judge_balance(&about(0.1, 400), &quiet) {
            BalanceVerdict::CannotTell(why) => assert!(why.contains("the computer made too little sound"), "{why}"),
            other => panic!("{other:?}"),
        }
        match judge_balance(&quiet, &about(0.1, 400)) {
            BalanceVerdict::CannotTell(why) => assert!(why.contains("the microphone made too little sound"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(judge_balance(&[], &[]), BalanceVerdict::CannotTell(_)));
        assert!(matches!(judge_balance(&about(0.1, 30), &about(0.1, 30)), BalanceVerdict::CannotTell(_)));
    }

    #[test]
    fn the_words_give_the_numbers_and_a_way_to_fix_it() {
        let f = describe_balance(&judge_balance(&about(0.05, 400), &about(0.2, 400)));
        assert!(!f.check.ok);
        assert!(f.check.message.contains("about 12 decibels louder than your voice"), "{}", f.check.message);
        assert!(
            f.check.message.contains("--system-gain-db -18") && f.check.message.contains("editor"),
            "{}",
            f.check.message
        );
        assert!(f.warning.as_deref().unwrap_or("").starts_with("Balance:"));

        let ok = describe_balance(&judge_balance(&about(0.1, 400), &about(0.03, 400)));
        assert!(ok.check.ok && ok.warning.is_none() && ok.check.message.contains("good balance"));

        let nearly = describe_balance(&judge_balance(&about(0.1, 400), &about(0.08, 400)));
        assert!(nearly.check.message.contains("only 2 decibels quieter than your voice"), "{}", nearly.check.message);
    }

    #[test]
    fn sound_files_are_compared_from_disk() {
        use openstage_capture_project::WavWriter;
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, value: i16| {
            let path = dir.path().join(name);
            let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
            w.append(&vec![value; 48_000 * 2 * 3]).unwrap();
            w.finish().unwrap();
            path
        };
        let finding = check_balance_in_files(&write("mic.wav", 1600), &write("system.wav", 6400)).unwrap();
        assert!(!finding.check.ok && finding.check.message.contains("12 decibels louder"), "{}", finding.check.message);
        assert!(check_balance_in_files(&dir.path().join("a.wav"), &dir.path().join("b.wav")).is_err());
    }
}
