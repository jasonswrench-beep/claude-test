// SPDX-License-Identifier: Apache-2.0
//! Noticing when the microphone is hearing the computer's own sound.
//!
//! With speakers instead of headphones, the microphone picks up what the speakers play, so the mixed
//! recording has the computer's sound twice, the second time a little late and quieter: an echo. That
//! is hard to fix afterwards, so it is better to tell the author early and plainly.
//!
//! The method is simple on purpose. Each track is reduced to how loud it is in every 20 millisecond
//! window. If the microphone's loudness rises and falls with the computer's, a little later, the
//! microphone is hearing the computer. The numbers below are first estimates, not measured on real
//! rooms yet: this can miss an echo that is quiet compared with the voice, and it says "likely", never
//! "certain".

use crate::{check, Check};
use openstage_capture_project::read_wav_in_pieces;
use std::io;
use std::path::Path;

/// How long each loudness reading covers.
pub const WINDOW_MS: u32 = 20;
/// The longest delay looked for: the speakers to the microphone is a few milliseconds, but the
/// computer's sound and the microphone are not captured at exactly the same moment.
const MAX_LAG_MS: u32 = 300;
/// How much computer sound is needed to say anything: 2 seconds of it.
const MIN_ACTIVE_WINDOWS: usize = 100;
/// The microphone's loudness must follow the computer's at least this closely (0 to 1).
const MIN_CORRELATION: f32 = 0.5;
/// ...and the microphone must carry at least this much of the computer's sound (about 3 percent).
const MIN_LEAK_DB: f32 = -30.0;

/// Loudness readings of a sound track, one per window, each from 0.0 to 1.0.
pub struct Envelope {
    frames_per_window: usize,
    channels: usize,
    sum_squares: f64,
    frames_in_window: usize,
    out: Vec<f32>,
}

impl Envelope {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            frames_per_window: (sample_rate as usize * WINDOW_MS as usize / 1000).max(1),
            channels: channels.max(1) as usize,
            sum_squares: 0.0,
            frames_in_window: 0,
            out: Vec::new(),
        }
    }

    /// Add interleaved sound.
    pub fn push(&mut self, samples: &[i16]) {
        for frame in samples.chunks_exact(self.channels) {
            let mean_square: f64 =
                frame.iter().map(|s| (*s as f64 / 32768.0).powi(2)).sum::<f64>() / self.channels as f64;
            self.sum_squares += mean_square;
            self.frames_in_window += 1;
            if self.frames_in_window == self.frames_per_window {
                self.out.push((self.sum_squares / self.frames_in_window as f64).sqrt() as f32);
                self.sum_squares = 0.0;
                self.frames_in_window = 0;
            }
        }
    }

    /// The readings. A last, shorter window is not included.
    pub fn finish(self) -> Vec<f32> {
        self.out
    }
}

/// Loudness readings of sound held in memory.
pub fn envelope_of(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<f32> {
    let mut e = Envelope::new(sample_rate, channels);
    e.push(samples);
    e.finish()
}

/// Loudness readings of a saved WAV file, read a piece at a time.
pub fn envelope_of_file(path: &Path) -> io::Result<Vec<f32>> {
    let mut envelope: Option<Envelope> = None;
    read_wav_in_pieces(path, |rate, channels, piece| {
        envelope.get_or_insert_with(|| Envelope::new(rate, channels)).push(piece);
    })?;
    Ok(envelope.map(Envelope::finish).unwrap_or_default())
}

/// What was found.
#[derive(Debug, Clone, PartialEq)]
pub enum EchoVerdict {
    /// The microphone seems to be hearing the computer's sound.
    Likely {
        /// How closely the microphone's loudness follows the computer's (0 to 1).
        correlation: f32,
        /// How much quieter the computer's sound is in the microphone, in decibels (negative).
        leak_db: f32,
        /// How much later it arrives in the microphone.
        delay_ms: u32,
    },
    /// The microphone's loudness does not follow the computer's.
    Unlikely { correlation: f32 },
    /// There was not enough computer sound, or it was too steady, to tell. Says why.
    CannotTell(String),
}

fn mean(v: &[f32]) -> f64 {
    v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64
}

/// Decide whether the microphone is hearing the computer's sound, from loudness readings of each.
pub fn detect_echo(mic: &[f32], system: &[f32]) -> EchoVerdict {
    let n = mic.len().min(system.len());
    if n == 0 {
        return EchoVerdict::CannotTell("there was no sound to look at".into());
    }
    let mut sorted: Vec<f32> = system[..n].to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p90 = sorted[(n * 9 / 10).min(n - 1)];
    // A window counts as "the computer is making sound" when it is clearly above silence and not tiny
    // next to the loud parts.
    let threshold = (0.1 * p90).max(0.003);
    let active: Vec<usize> = (0..n).filter(|i| system[*i] > threshold).collect();
    if active.len() < MIN_ACTIVE_WINDOWS {
        return EchoVerdict::CannotTell(format!(
            "the computer made sound for only {:.1} seconds, and at least {} seconds are needed",
            active.len() as f64 * WINDOW_MS as f64 / 1000.0,
            MIN_ACTIVE_WINDOWS as u32 * WINDOW_MS / 1000
        ));
    }

    let max_lag = (MAX_LAG_MS / WINDOW_MS) as usize;
    let mut best: Option<(f32, usize, f64)> = None; // correlation, lag, leak slope
    for lag in 0..=max_lag {
        let pairs: Vec<(f32, f32)> =
            active.iter().filter(|i| **i + lag < n).map(|i| (system[*i], mic[*i + lag])).collect();
        if pairs.len() < MIN_ACTIVE_WINDOWS {
            continue;
        }
        // Compare how the loudness rises and falls, with the loudest parts evened out a little.
        let xs: Vec<f32> = pairs.iter().map(|p| p.0.sqrt()).collect();
        let ys: Vec<f32> = pairs.iter().map(|p| p.1.sqrt()).collect();
        let (mx, my) = (mean(&xs), mean(&ys));
        let (mut sxy, mut sxx, mut syy) = (0.0f64, 0.0f64, 0.0f64);
        for (x, y) in xs.iter().zip(&ys) {
            let (dx, dy) = (*x as f64 - mx, *y as f64 - my);
            sxy += dx * dy;
            sxx += dx * dx;
            syy += dy * dy;
        }
        if sxx < 1e-9 || syy < 1e-12 {
            continue;
        }
        let r = (sxy / (sxx * syy).sqrt()) as f32;
        if best.map(|b| r > b.0).unwrap_or(true) {
            // How much of the computer's sound (in plain loudness) is in the microphone.
            let (rx, ry): (Vec<f64>, Vec<f64>) = pairs.iter().map(|p| (p.0 as f64, p.1 as f64)).unzip();
            let (mrx, mry) = (rx.iter().sum::<f64>() / rx.len() as f64, ry.iter().sum::<f64>() / ry.len() as f64);
            let cov: f64 = rx.iter().zip(&ry).map(|(a, b)| (a - mrx) * (b - mry)).sum();
            let var: f64 = rx.iter().map(|a| (a - mrx).powi(2)).sum();
            let slope = if var > 1e-12 { cov / var } else { 0.0 };
            best = Some((r, lag, slope));
        }
    }
    let Some((r, lag, slope)) = best else {
        return EchoVerdict::CannotTell(
            "the computer's sound was too steady to tell it apart from the microphone's".into(),
        );
    };
    let leak_db = 20.0 * (slope.max(1e-6)).log10() as f32;
    if r >= MIN_CORRELATION && leak_db >= MIN_LEAK_DB {
        EchoVerdict::Likely { correlation: r, leak_db, delay_ms: lag as u32 * WINDOW_MS }
    } else {
        EchoVerdict::Unlikely { correlation: r }
    }
}

/// What to tell the author.
#[derive(Debug, Clone)]
pub struct EchoFinding {
    /// The finding, named `echo`. `ok` is false only when an echo is likely.
    pub check: Check,
    /// A warning for the author to act on, when an echo is likely.
    pub warning: Option<String>,
}

/// Turn a verdict into words.
pub fn describe_echo(verdict: &EchoVerdict) -> EchoFinding {
    match verdict {
        EchoVerdict::Likely { correlation, leak_db, delay_ms } => {
            let advice = "Played back, this sounds like an echo. Wearing headphones avoids it. The microphone and the computer's sound are saved as separate files (mic.wav and system.wav), so it can also be fixed afterwards in the editor.";
            EchoFinding {
                check: check(
                    "echo",
                    false,
                    format!(
                        "The microphone is probably hearing the computer's sound (its loudness follows the computer's closely: {correlation:.2} out of 1; the computer's sound reaches the microphone about {} decibels quieter and about {delay_ms} milliseconds late). {advice}",
                        leak_db.abs().round()
                    ),
                ),
                warning: Some(format!("Echo likely: the microphone seems to be hearing the speakers. {advice}")),
            }
        }
        EchoVerdict::Unlikely { correlation } => EchoFinding {
            check: check(
                "echo",
                true,
                format!(
                    "The microphone does not seem to be hearing the computer's sound (its loudness follows the computer's only {correlation:.2} out of 1)."
                ),
            ),
            warning: None,
        },
        EchoVerdict::CannotTell(why) => EchoFinding {
            check: check(
                "echo",
                true,
                format!("Could not tell whether the microphone is hearing the computer's sound: {why}. Play something during the test to check."),
            ),
            warning: None,
        },
    }
}

/// Look for an echo in a take's two saved sound files.
pub fn check_echo_in_files(mic: &Path, system: &Path) -> Result<EchoFinding, String> {
    let mic = envelope_of_file(mic).map_err(|e| format!("The microphone file could not be read: {e}"))?;
    let system = envelope_of_file(system).map_err(|e| format!("The computer's sound file could not be read: {e}"))?;
    Ok(describe_echo(&detect_echo(&mic, &system)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstage_capture_project::WavWriter;

    /// A repeatable stream of numbers from 0 to 1.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32) / (1u64 << 31) as f32
        }
    }

    /// Loudness of music or speech: steady for a few windows, then another level.
    fn speechlike(windows: usize, seed: u64, loudest: f32) -> Vec<f32> {
        let mut rng = Lcg(seed);
        let mut out = Vec::new();
        while out.len() < windows {
            let level = loudest * (0.1 + 0.9 * rng.next());
            let hold = 3 + (rng.next() * 8.0) as usize;
            out.extend(std::iter::repeat_n(level, hold));
        }
        out.truncate(windows);
        out
    }

    fn delayed_copy(source: &[f32], windows: usize, gain: f32, floor: f32, seed: u64) -> Vec<f32> {
        let mut rng = Lcg(seed);
        (0..source.len())
            .map(|i| floor * rng.next() + if i >= windows { gain * source[i - windows] } else { 0.0 })
            .collect()
    }

    #[test]
    fn an_echo_is_found_with_its_delay_and_strength() {
        let system = speechlike(1500, 1, 0.3);
        let mic = delayed_copy(&system, 6, 0.2, 0.004, 2); // 120 ms late, 14 dB quieter
        match detect_echo(&mic, &system) {
            EchoVerdict::Likely { correlation, leak_db, delay_ms } => {
                assert!(correlation > 0.9, "{correlation}");
                assert!((-16.0..=-12.0).contains(&leak_db), "{leak_db}");
                assert_eq!(delay_ms, 120);
            }
            other => panic!("expected an echo, got {other:?}"),
        }
    }

    #[test]
    fn a_microphone_that_hears_only_the_room_is_not_an_echo() {
        let system = speechlike(1500, 1, 0.3);
        let voice = speechlike(1500, 99, 0.2); // someone talking, unrelated to the computer's sound
        let mic: Vec<f32> = voice.iter().map(|v| v + 0.004).collect();
        match detect_echo(&mic, &system) {
            EchoVerdict::Unlikely { correlation } => assert!(correlation < 0.3, "{correlation}"),
            other => panic!("expected no echo, got {other:?}"),
        }
    }

    #[test]
    fn an_echo_under_a_voice_is_still_found_when_the_voice_is_not_much_louder() {
        let system = speechlike(1500, 1, 0.3);
        let echo = delayed_copy(&system, 4, 0.25, 0.0, 3);
        let voice = speechlike(1500, 7, 0.05);
        let mic: Vec<f32> = echo.iter().zip(&voice).map(|(e, v)| e + v + 0.003).collect();
        assert!(matches!(detect_echo(&mic, &system), EchoVerdict::Likely { .. }), "{:?}", detect_echo(&mic, &system));
    }

    #[test]
    fn a_very_faint_leak_is_not_called_an_echo() {
        let system = speechlike(1500, 1, 0.3);
        let mic = delayed_copy(&system, 5, 0.005, 0.0005, 4); // 46 dB quieter: nobody would hear it
        assert!(matches!(detect_echo(&mic, &system), EchoVerdict::Unlikely { .. }), "{:?}", detect_echo(&mic, &system));
    }

    #[test]
    fn too_little_computer_sound_means_it_cannot_tell_and_says_why() {
        let silent = vec![0.0f32; 1500];
        let mic = speechlike(1500, 5, 0.2);
        match detect_echo(&mic, &silent) {
            EchoVerdict::CannotTell(why) => assert!(why.contains("0.0 seconds") && why.contains("at least 2"), "{why}"),
            other => panic!("{other:?}"),
        }
        let short = speechlike(60, 1, 0.3);
        assert!(matches!(detect_echo(&speechlike(60, 2, 0.2), &short), EchoVerdict::CannotTell(_)));
        assert!(matches!(detect_echo(&[], &[]), EchoVerdict::CannotTell(_)));
    }

    #[test]
    fn a_perfectly_steady_computer_sound_cannot_be_told_apart() {
        let steady = vec![0.1f32; 1500];
        let mic = speechlike(1500, 5, 0.2);
        assert!(matches!(detect_echo(&mic, &steady), EchoVerdict::CannotTell(_)));
    }

    #[test]
    fn loudness_is_read_from_sound_in_twenty_millisecond_windows() {
        // One second of a steady half-scale tone: 50 windows, each about 0.5 loud.
        let samples = vec![16384i16; 48_000 * 2];
        let env = envelope_of(&samples, 48_000, 2);
        assert_eq!(env.len(), 50);
        assert!(env.iter().all(|v| (v - 0.5).abs() < 0.001));
        // The same sound handed over in odd-sized pieces gives the same readings.
        let mut e = Envelope::new(48_000, 2);
        for piece in samples.chunks(3_000) {
            e.push(piece);
        }
        assert_eq!(e.finish(), env);
    }

    #[test]
    fn the_whole_path_from_sound_files_to_words_finds_an_echo_and_a_clean_recording() {
        let dir = tempfile::tempdir().unwrap();
        // Computer's sound: noise that gets louder and quieter every so often.
        let mut rng = Lcg(11);
        let loudness = speechlike(1500, 21, 0.4);
        let mut system: Vec<i16> = Vec::new();
        for level in &loudness {
            for _ in 0..(48 * WINDOW_MS as usize) {
                let v = ((rng.next() * 2.0 - 1.0) * level * 32767.0) as i16;
                system.extend_from_slice(&[v, v]);
            }
        }
        // The microphone hears it 30 milliseconds late and 12 decibels quieter, plus a little room noise.
        let delay = 48 * 30 * 2;
        let mut leaky = vec![0i16; system.len()];
        for i in delay..system.len() {
            leaky[i] = (system[i - delay] as f32 * 0.25) as i16 + ((rng.next() - 0.5) * 60.0) as i16;
        }
        let clean: Vec<i16> = (0..system.len()).map(|_| ((rng.next() - 0.5) * 60.0) as i16).collect();

        let write = |name: &str, samples: &[i16]| {
            let path = dir.path().join(name);
            let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
            w.append(samples).unwrap();
            w.finish().unwrap();
            path
        };
        let system_file = write("system.wav", &system);
        let echo = check_echo_in_files(&write("leaky.wav", &leaky), &system_file).unwrap();
        assert!(!echo.check.ok && echo.warning.is_some(), "{}", echo.check.message);
        assert!(
            echo.check.message.contains("probably hearing the computer's sound")
                && echo.check.message.contains("headphones")
        );
        let fine = check_echo_in_files(&write("clean.wav", &clean), &system_file).unwrap();
        assert!(fine.check.ok && fine.warning.is_none(), "{}", fine.check.message);
        assert!(fine.check.message.contains("does not seem to be hearing"));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_in_words() {
        let dir = tempfile::tempdir().unwrap();
        let e = check_echo_in_files(&dir.path().join("none.wav"), &dir.path().join("none2.wav")).unwrap_err();
        assert!(e.contains("microphone file could not be read"), "{e}");
    }
}
