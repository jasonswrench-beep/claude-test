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

/// How much sound is needed to say anything: half a second of each.
const MIN_ACTIVE_WINDOWS: usize = 25;
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

/// How much louder (positive) or quieter (negative) each sound is made in the mix inside the video,
/// in whole decibels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mix {
    pub microphone_db: i32,
    pub system_db: i32,
}

impl Mix {
    fn changed(self) -> bool {
        self.microphone_db != 0 || self.system_db != 0
    }
}

/// What was found. The numbers describe the mix inside the video, with the adjustments the author chose.
#[derive(Debug, Clone, PartialEq)]
pub enum BalanceVerdict {
    /// The voice is comfortably louder than the computer's sound.
    Good {
        voice_above_db: f32,
        /// The same difference before any adjustment, as the two sounds were heard.
        as_heard_db: f32,
    },
    /// The computer's sound is as loud as the voice, or nearly.
    ComputerTooLoud {
        /// How much louder the computer's sound is than the voice (negative if it is a little quieter).
        louder_by_db: f32,
        as_heard_db: f32,
        /// How many decibels quieter the computer's sound should be made than it is in the mix now, to put
        /// the voice about 6 decibels above it.
        change_db: i32,
        /// The setting for the computer's sound that does that, in total.
        suggested_system_gain_db: i32,
    },
    /// The computer's sound is far quieter than the voice.
    ComputerVeryQuiet { voice_above_db: f32, as_heard_db: f32 },
    /// One of them did not make enough sound to compare. Says which.
    CannotTell(String),
}

/// Compare the loudness of the microphone and of the computer's sound as they are in the mix, from
/// readings of each.
pub fn judge_balance(mic: &[f32], system: &[f32], mix: Mix) -> BalanceVerdict {
    let (Some(voice), Some(computer)) = (typical_level(mic), typical_level(system)) else {
        let which = match (typical_level(mic).is_some(), typical_level(system).is_some()) {
            (false, false) => "neither the microphone nor the computer made enough sound",
            (false, true) => "the microphone made too little sound",
            _ => "the computer made too little sound",
        };
        return BalanceVerdict::CannotTell(format!("{which} (at least half a second of each is needed)"));
    };
    let as_heard = db(voice) - db(computer);
    let above = as_heard + (mix.microphone_db - mix.system_db) as f32;
    if above < MIN_VOICE_MARGIN_DB {
        let change = ((TARGET_VOICE_MARGIN_DB - above).round() as i32).clamp(1, 40);
        BalanceVerdict::ComputerTooLoud {
            louder_by_db: -above,
            as_heard_db: as_heard,
            change_db: change,
            suggested_system_gain_db: (mix.system_db - change).max(crate::sound::MIN_GAIN_DB),
        }
    } else if above > MUCH_QUIETER_DB {
        BalanceVerdict::ComputerVeryQuiet { voice_above_db: above, as_heard_db: as_heard }
    } else {
        BalanceVerdict::Good { voice_above_db: above, as_heard_db: as_heard }
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

/// Turn a verdict into words. `mix` is what the video was mixed with, so the words can say what was
/// heard and what to change from where things are now.
pub fn describe_balance(verdict: &BalanceVerdict, mix: Mix) -> BalanceFinding {
    let where_ = if mix.changed() { "In the mix inside the video, with your adjustments, " } else { "" };
    let before = |as_heard: f32| {
        if mix.changed() {
            format!(
                " (As heard, before your adjustments, your voice was {} decibels {} the computer's sound.)",
                as_heard.abs().round(),
                if as_heard >= 0.0 { "above" } else { "below" }
            )
        } else {
            String::new()
        }
    };
    match verdict {
        BalanceVerdict::Good { voice_above_db, as_heard_db } => BalanceFinding {
            check: check(
                "balance",
                true,
                format!(
                    "{where_}{}our voice is about {} decibels louder than the computer's sound while each is making sound. That is a good balance for narration.{}",
                    if mix.changed() { "y" } else { "Y" },
                    voice_above_db.round(),
                    before(*as_heard_db)
                ),
            ),
            warning: None,
        },
        BalanceVerdict::ComputerTooLoud { louder_by_db, as_heard_db, change_db, suggested_system_gain_db } => {
            let how = if *louder_by_db >= 1.0 {
                format!("about {} decibels louder than your voice", louder_by_db.round())
            } else if *louder_by_db > -1.0 {
                "about as loud as your voice".to_string()
            } else {
                format!("only {} decibels quieter than your voice", louder_by_db.abs().round())
            };
            let advice = if mix.changed() {
                format!(
                    "To put your voice about 6 decibels above it, make the computer's sound {change_db} decibels quieter than it is in the mix now (record with --system-gain-db {suggested_system_gain_db}), or balance mic.wav and system.wav in the editor: the separate files are not changed by the mix."
                )
            } else {
                format!(
                    "To put your voice about 6 decibels above it, make the computer's sound {change_db} decibels quieter in the mix (record with --system-gain-db {suggested_system_gain_db}), or balance mic.wav and system.wav in the editor: the separate files are not changed by the mix."
                )
            };
            let sentence = if mix.changed() {
                format!("{where_}the computer's sound is {how} while each is making sound, so your voice may be hard to hear.")
            } else {
                format!("The computer's sound is {how} while each is making sound, so in the mixed video your voice may be hard to hear.")
            };
            BalanceFinding {
                check: check("balance", false, format!("{sentence}{} {advice}", before(*as_heard_db))),
                warning: Some(format!("Balance: the computer's sound is {how}{}. {advice}", if mix.changed() { " in the mix" } else { "" })),
            }
        }
        BalanceVerdict::ComputerVeryQuiet { voice_above_db, as_heard_db } => BalanceFinding {
            check: check(
                "balance",
                true,
                format!(
                    "{where_}{}he computer's sound is about {} decibels quieter than your voice, which may be too quiet to hear in the mixed video. If it should be heard, raise it in the editor, or record with a higher --system-gain-db.{}",
                    if mix.changed() { "t" } else { "T" },
                    voice_above_db.round(),
                    before(*as_heard_db)
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

/// Loudness readings of sound held in memory, for the sound test (which mixes nothing, so nothing is adjusted).
pub fn balance_of_samples(mic: &[i16], system: &[i16], sample_rate: u32, channels: u16) -> BalanceFinding {
    let envelope = |samples: &[i16]| {
        let mut e = Envelope::new(sample_rate, channels);
        e.push(samples);
        e.finish()
    };
    describe_balance(&judge_balance(&envelope(mic), &envelope(system), Mix::default()), Mix::default())
}

/// Compare a take's two saved sound files, as they were mixed into the video.
pub fn check_balance_in_files(mic: &Path, system: &Path, mix: Mix) -> Result<BalanceFinding, String> {
    let mic = envelope_of_file(mic).map_err(|e| format!("The microphone file could not be read: {e}"))?;
    let system = envelope_of_file(system).map_err(|e| format!("The computer's sound file could not be read: {e}"))?;
    Ok(describe_balance(&judge_balance(&mic, &system, mix), mix))
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
        match judge_balance(&about(0.05, 400), &about(0.2, 400), Mix::default()) {
            BalanceVerdict::ComputerTooLoud { louder_by_db, suggested_system_gain_db, .. } => {
                assert!((11.0..=13.0).contains(&louder_by_db), "{louder_by_db}");
                assert!((-19..=-17).contains(&suggested_system_gain_db), "{suggested_system_gain_db}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_voice_comfortably_above_the_computer_is_a_good_balance() {
        match judge_balance(&about(0.1, 400), &about(0.03, 400), Mix::default()) {
            BalanceVerdict::Good { voice_above_db, .. } => {
                assert!((9.0..=12.0).contains(&voice_above_db), "{voice_above_db}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_voice_only_slightly_above_the_computer_still_gets_advice() {
        match judge_balance(&about(0.1, 400), &about(0.08, 400), Mix::default()) {
            BalanceVerdict::ComputerTooLoud { louder_by_db, suggested_system_gain_db, .. } => {
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
            judge_balance(&about(0.5, 400), &about(0.006, 400), Mix::default()),
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
        match judge_balance(&about(0.1, 400), &quiet, Mix::default()) {
            BalanceVerdict::CannotTell(why) => assert!(why.contains("the computer made too little sound"), "{why}"),
            other => panic!("{other:?}"),
        }
        match judge_balance(&quiet, &about(0.1, 400), Mix::default()) {
            BalanceVerdict::CannotTell(why) => assert!(why.contains("the microphone made too little sound"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(judge_balance(&[], &[], Mix::default()), BalanceVerdict::CannotTell(_)));
        assert!(matches!(
            judge_balance(&about(0.1, 20), &about(0.1, 20), Mix::default()),
            BalanceVerdict::CannotTell(_)
        ));
    }

    #[test]
    fn the_words_give_the_numbers_and_a_way_to_fix_it() {
        let f = describe_balance(&judge_balance(&about(0.05, 400), &about(0.2, 400), Mix::default()), Mix::default());
        assert!(!f.check.ok);
        assert!(f.check.message.contains("about 12 decibels louder than your voice"), "{}", f.check.message);
        assert!(
            f.check.message.contains("--system-gain-db -18") && f.check.message.contains("editor"),
            "{}",
            f.check.message
        );
        assert!(f.warning.as_deref().unwrap_or("").starts_with("Balance:"));

        let ok = describe_balance(&judge_balance(&about(0.1, 400), &about(0.03, 400), Mix::default()), Mix::default());
        assert!(ok.check.ok && ok.warning.is_none() && ok.check.message.contains("good balance"));

        let nearly =
            describe_balance(&judge_balance(&about(0.1, 400), &about(0.08, 400), Mix::default()), Mix::default());
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
        let finding =
            check_balance_in_files(&write("mic.wav", 1600), &write("system.wav", 6400), Mix::default()).unwrap();
        assert!(!finding.check.ok && finding.check.message.contains("12 decibels louder"), "{}", finding.check.message);
        assert!(check_balance_in_files(&dir.path().join("a.wav"), &dir.path().join("b.wav"), Mix::default()).is_err());
    }

    #[test]
    fn adjustments_already_chosen_are_taken_into_account_not_ignored() {
        // As heard, the computer is 12 decibels louder than the voice. The mix already made the voice 6 louder
        // and the computer 10 quieter, which puts the voice 4 above it.
        let mix = Mix { microphone_db: 6, system_db: -10 };
        let verdict = judge_balance(&about(0.05, 400), &about(0.2, 400), mix);
        match &verdict {
            BalanceVerdict::Good { voice_above_db, as_heard_db } => {
                assert!((3.5..=4.5).contains(voice_above_db), "{voice_above_db}");
                assert!((-13.0..=-11.0).contains(as_heard_db), "{as_heard_db}");
            }
            other => panic!("{other:?}"),
        }
        let f = describe_balance(&verdict, mix);
        assert!(f.check.ok);
        assert!(
            f.check.message.starts_with(
                "In the mix inside the video, with your adjustments, your voice is about 4 decibels louder"
            ),
            "{}",
            f.check.message
        );
        assert!(
            f.check.message.contains("before your adjustments, your voice was 12 decibels below"),
            "{}",
            f.check.message
        );
    }

    #[test]
    fn advice_is_a_change_from_where_the_mix_is_now_with_the_new_total_to_use() {
        // As heard the computer is 12 decibels louder; the author only lowered it by 5, which leaves it 7 louder.
        let mix = Mix { microphone_db: 0, system_db: -5 };
        let verdict = judge_balance(&about(0.05, 400), &about(0.2, 400), mix);
        match &verdict {
            BalanceVerdict::ComputerTooLoud { change_db, suggested_system_gain_db, .. } => {
                assert!((12..=14).contains(change_db), "{change_db}");
                assert!((-19..=-17).contains(suggested_system_gain_db), "{suggested_system_gain_db}");
            }
            other => panic!("{other:?}"),
        }
        let f = describe_balance(&verdict, mix);
        assert!(!f.check.ok);
        assert!(
            f.check.message.contains("quieter than it is in the mix now")
                && f.check.message.contains("--system-gain-db -18"),
            "{}",
            f.check.message
        );
        assert!(f.warning.as_deref().unwrap_or("").contains("in the mix"));
    }

    #[test]
    fn the_suggested_total_never_goes_beyond_the_allowed_range() {
        let verdict = judge_balance(&about(0.01, 400), &about(0.9, 400), Mix { microphone_db: 0, system_db: -30 });
        match verdict {
            BalanceVerdict::ComputerTooLoud { suggested_system_gain_db, .. } => {
                assert_eq!(suggested_system_gain_db, crate::sound::MIN_GAIN_DB)
            }
            other => panic!("{other:?}"),
        }
    }
}
