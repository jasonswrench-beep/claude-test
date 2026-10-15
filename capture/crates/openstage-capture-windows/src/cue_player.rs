// SPDX-License-Identifier: Apache-2.0
//! Playing the recorder's own cue sounds, and the test that lets a person hear them one by one.
//!
//! The sounds themselves are made, and their timing decided, in `openstage-capture` (`cues`). This
//! file plays them on Windows and checks that they can be heard.

use crate::{check, Check};
use openstage_capture::cues::{render, to_wav, Cue, CuePlayer, CUE_TAIL_MS};
use openstage_capture::{Clock, SystemClock};
use std::path::PathBuf;

/// The real player on Windows; on any other system one that plays nothing.
pub fn system_cue_player(volume: u8) -> Box<dyn CuePlayer> {
    #[cfg(windows)]
    {
        Box::new(crate::win_cues::WindowsCues::new(volume))
    }
    #[cfg(not(windows))]
    {
        let _ = volume;
        Box::new(openstage_capture::NoCues)
    }
}

/// What the cue test should do.
#[derive(Debug, Clone)]
pub struct CueTestOptions {
    /// How loud, from 0 to 100.
    pub volume: u8,
    /// Where to save each sound as a file, so it can also be opened in any player. `None` saves nothing.
    pub out_dir: Option<PathBuf>,
}

/// Play every cue in the order a recording uses them, saying in words what each is before it sounds.
pub fn run_cue_test(opts: &CueTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_cue_test_with(opts, system_cue_player(opts.volume).as_ref(), &SystemClock::new(), report)
}

/// [`run_cue_test`] with the player and the clock supplied, so it can be tested anywhere.
pub fn run_cue_test_with(
    opts: &CueTestOptions,
    player: &dyn CuePlayer,
    clock: &dyn Clock,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_fine = true;
    report(check(
        "cue-volume",
        opts.volume > 0,
        if opts.volume > 0 {
            format!(
                "The sounds are played at volume {} of 100. To change it use --cue-volume with a number from 1 to 100; --cues off turns them off.",
                opts.volume
            )
        } else {
            "The volume is 0, so nothing can be heard. Use --cue-volume with a number from 1 to 100.".to_string()
        },
    ));
    all_fine &= opts.volume > 0;
    if let Some(dir) = &opts.out_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            report(check("cue-files", false, format!("The folder {} could not be made: {e}", dir.display())));
            all_fine = false;
        } else {
            for cue in Cue::ALL {
                let path = dir.join(format!("cue-{}.wav", cue.name()));
                if let Err(e) = std::fs::write(&path, to_wav(&render(cue, opts.volume))) {
                    report(check("cue-files", false, format!("{} could not be saved: {e}", path.display())));
                    all_fine = false;
                }
            }
            report(check(
                "cue-files",
                true,
                format!("Each sound was also saved as a file in {}, to open in any player.", dir.display()),
            ));
        }
    }
    for cue in Cue::ALL {
        // The words come first, so a screen reader can say what is about to sound.
        report(check(
            &format!("cue-{}", cue.name()),
            true,
            format!(
                "Next: the {} sound. It is {}. It lasts {} milliseconds.",
                cue.name(),
                cue.describe(),
                cue.duration_ms()
            ),
        ));
        let played = player.play(cue);
        if played == 0 && opts.volume > 0 {
            report(check(
                &format!("cue-{}-played", cue.name()),
                false,
                format!("The {} sound could not be played on this computer.", cue.name()),
            ));
            all_fine = false;
        }
        clock.sleep_ms(cue.duration_ms() + CUE_TAIL_MS + 900);
    }
    all_fine
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstage_capture::ManualClock;
    use std::cell::RefCell;

    struct Counting(RefCell<Vec<Cue>>, bool);
    impl CuePlayer for Counting {
        fn play(&self, cue: Cue) -> u64 {
            self.0.borrow_mut().push(cue);
            if self.1 {
                cue.duration_ms()
            } else {
                0
            }
        }
    }

    fn run(volume: u8, out: Option<PathBuf>, works: bool) -> (bool, Vec<Check>, Vec<Cue>, u64) {
        let player = Counting(RefCell::new(Vec::new()), works);
        let clock = ManualClock::new();
        let mut checks = Vec::new();
        let ok = run_cue_test_with(&CueTestOptions { volume, out_dir: out }, &player, &clock, &mut |c| checks.push(c));
        (ok, checks, player.0.into_inner(), clock.now_ms())
    }

    #[test]
    fn every_cue_is_announced_in_words_then_played_in_order() {
        let (ok, checks, played, took) = run(70, None, true);
        assert!(ok, "{checks:?}");
        assert_eq!(played, Cue::ALL);
        let names: Vec<&str> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(
            names,
            [
                "cue-volume",
                "cue-countdown",
                "cue-start",
                "cue-pause",
                "cue-resume",
                "cue-stop",
                "cue-error",
                "cue-interactions"
            ]
        );
        assert!(
            checks[2].message.contains("three rising notes") && checks[2].message.contains("lasts 340 milliseconds")
        );
        assert!(took > 7 * 900, "a pause after each sound so they are not run together: {took}");
    }

    #[test]
    fn a_sound_that_cannot_be_played_fails_the_test_in_words() {
        let (ok, checks, _, _) = run(70, None, false);
        assert!(!ok);
        assert!(checks.iter().any(|c| !c.ok && c.message.contains("The start sound could not be played")));
    }

    #[test]
    fn volume_zero_is_said_not_played_as_a_failure_of_the_computer() {
        let (ok, checks, _, _) = run(0, None, false);
        assert!(!ok);
        assert!(checks[0].message.contains("volume is 0"));
        assert!(!checks.iter().any(|c| c.message.contains("could not be played")));
    }

    #[test]
    fn each_sound_can_also_be_saved_as_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let (ok, checks, _, _) = run(70, Some(dir.path().join("sounds")), true);
        assert!(ok, "{checks:?}");
        for cue in Cue::ALL {
            let bytes = std::fs::read(dir.path().join("sounds").join(format!("cue-{}.wav", cue.name()))).unwrap();
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(bytes.len(), 44 + render(cue, 70).len() * 2);
        }
        assert!(checks.iter().any(|c| c.check == "cue-files" && c.ok));
    }
}
