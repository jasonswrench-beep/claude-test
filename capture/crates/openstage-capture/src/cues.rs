// SPDX-License-Identifier: Apache-2.0
//! The sounds that tell the author what the recorder is doing.
//!
//! An author who cannot see the screen needs to know, at every moment, whether the recorder is
//! recording (story A4 in `docs/prd.md`). The recorder makes these sounds itself, because OpenStage's
//! own announcements may not be heard while another program is in front. There is one for each of:
//! the countdown, the start, a pause, a resume, the stop, and an error.
//!
//! They are told apart by their shape, not only by their pitch, so that nobody has to compare two
//! pitches from memory: rising notes mean "on", falling notes mean "off"; three notes mean "begin or
//! end" and two notes mean "pause or resume"; a low rough buzz means "something went wrong"; one very
//! short beep, once a second, is the countdown. Every cue is under 400 milliseconds long.
//!
//! **No cue is ever played while recording time is running**, so none of them ends up in a recording:
//! the engine plays them during the countdown, while paused, and after the recording has stopped, and
//! it waits for the start and resume cues to finish before it carries on. (The sound the recorder
//! hears during those moments is thrown away anyway; see `docs/capture-protocol-spec.md`.)
//!
//! This file only makes the sounds and decides when they play. Playing them is left to a
//! [`CuePlayer`]; the Windows one is in `openstage-capture-windows`.

use std::f64::consts::PI;

/// Samples per second of the cues.
pub const CUE_RATE: u32 = 48_000;
/// How loud the cues are when nothing is said, from 0 (silent) to 100.
pub const DEFAULT_VOLUME: u8 = 70;
/// Extra time left after a cue before the recorder carries on, so its last echo has faded.
pub const CUE_TAIL_MS: u64 = 60;
/// Every note fades in and out over this long, so a cue never clicks.
const FADE_MS: f64 = 8.0;
/// The loudest a cue gets at volume 100, as a share of the loudest sound possible.
const LOUDEST: f64 = 0.6;

/// The things the recorder can tell the author with a sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    /// Once a second during the countdown.
    Tick,
    /// The countdown is over: recording begins when this ends.
    Start,
    Pause,
    Resume,
    Stop,
    /// Something went wrong and the recording ended.
    Error,
    /// The recorder is about to note what you do (the cursor, the clicks, the keys): sounded once as the countdown
    /// begins, and only when interaction recording is on (story A6).
    Watching,
}

impl Cue {
    pub const ALL: [Cue; 7] = [Cue::Tick, Cue::Start, Cue::Pause, Cue::Resume, Cue::Stop, Cue::Error, Cue::Watching];

    /// What the cue is for, in a few words.
    pub fn name(self) -> &'static str {
        match self {
            Cue::Tick => "countdown",
            Cue::Start => "start",
            Cue::Pause => "pause",
            Cue::Resume => "resume",
            Cue::Stop => "stop",
            Cue::Error => "error",
            Cue::Watching => "interactions",
        }
    }

    /// What the cue sounds like, in words, for anyone who cannot hear it yet and for the documentation.
    pub fn describe(self) -> &'static str {
        match self {
            Cue::Tick => "one very short high beep, once a second during the countdown",
            Cue::Start => "three rising notes, the last one longer; recording begins when they end",
            Cue::Pause => "two falling notes",
            Cue::Resume => "two rising notes",
            Cue::Stop => "three falling notes, the last one low and long",
            Cue::Error => "two low, rough buzzes",
            Cue::Watching => "four soft notes that go up, stay, and come back down: what you do is being noted",
        }
    }

    /// How long the cue sounds, in milliseconds.
    pub fn duration_ms(self) -> u64 {
        self.steps().iter().map(|s| s.ms as u64).sum()
    }

    /// The cue as a list of notes, for tests and for people who want to check the design.
    pub fn steps(self) -> &'static [Step] {
        match self {
            Cue::Tick => &TICK,
            Cue::Start => &START,
            Cue::Pause => &PAUSE,
            Cue::Resume => &RESUME,
            Cue::Stop => &STOP,
            Cue::Error => &ERROR,
            Cue::Watching => &WATCHING,
        }
    }
}

const fn soft(hz: f64, ms: u32) -> Step {
    Step { hz, ms, tone: Tone::Soft }
}
const fn buzz(hz: f64, ms: u32) -> Step {
    Step { hz, ms, tone: Tone::Buzz }
}
const fn rest(ms: u32) -> Step {
    Step { hz: 0.0, ms, tone: Tone::Soft }
}

const TICK: [Step; 1] = [soft(1000.0, 50)];
/// C5, E5, G5
const START: [Step; 3] = [soft(523.25, 90), soft(659.25, 90), soft(783.99, 160)];
/// E5, A4
const PAUSE: [Step; 2] = [soft(659.25, 100), soft(440.0, 140)];
/// A4, E5
const RESUME: [Step; 2] = [soft(440.0, 100), soft(659.25, 140)];
/// G5, D5, G4
const STOP: [Step; 3] = [soft(783.99, 90), soft(587.33, 90), soft(392.0, 160)];
const ERROR: [Step; 3] = [buzz(196.0, 140), rest(60), buzz(196.0, 140)];
/// A4, E5, E5, A4: a hill, which is neither "on" (rising) nor "off" (falling).
const WATCHING: [Step; 4] = [soft(440.0, 70), soft(659.25, 70), soft(659.25, 70), soft(440.0, 110)];

/// How a note sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A clear, soft note.
    Soft,
    /// A rough, hollow note, unlike any of the others.
    Buzz,
}

/// One note of a cue. A `hz` of 0 is a short silence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub hz: f64,
    pub ms: u32,
    pub tone: Tone,
}

/// The cue as sound: one channel, 16 bits, [`CUE_RATE`] samples a second. `volume` is 0 to 100 (more is
/// treated as 100); 0 makes silence.
pub fn render(cue: Cue, volume: u8) -> Vec<i16> {
    let volume = volume.min(100) as f64 / 100.0;
    // Loudness is felt roughly by the square of the setting, so the middle of the scale is not nearly as loud as the top.
    let peak = LOUDEST * volume * volume;
    let mut out = Vec::new();
    for step in cue.steps() {
        let count = (CUE_RATE as u64 * step.ms as u64 / 1000) as usize;
        let fade = (CUE_RATE as f64 * FADE_MS / 1000.0).min(count as f64 / 2.0);
        for n in 0..count {
            let t = n as f64 / CUE_RATE as f64;
            let wave = if step.hz <= 0.0 {
                0.0
            } else {
                let x = 2.0 * PI * step.hz * t;
                match step.tone {
                    // A sine with a little of its octave, so small speakers still carry it.
                    Tone::Soft => (x.sin() + 0.25 * (2.0 * x).sin()) / 1.25,
                    // Odd harmonics only: a hollow, buzzing sound.
                    Tone::Buzz => {
                        (x.sin() + (3.0 * x).sin() / 3.0 + (5.0 * x).sin() / 5.0) / (1.0 + 1.0 / 3.0 + 1.0 / 5.0)
                    }
                }
            };
            let from_start = n as f64;
            let from_end = (count - 1 - n) as f64;
            let envelope = |d: f64| if d >= fade { 1.0 } else { 0.5 - 0.5 * (PI * d / fade).cos() };
            let level = envelope(from_start) * envelope(from_end);
            out.push((wave * level * peak * 32767.0).round() as i16);
        }
    }
    out
}

/// The cue as the bytes of a WAV file (one channel, 16 bits), which Windows can play from memory or a
/// person can open in any player.
pub fn to_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // plain PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // one channel
    out.extend_from_slice(&CUE_RATE.to_le_bytes());
    out.extend_from_slice(&(CUE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Something that can make a cue heard.
pub trait CuePlayer {
    /// Start the cue and return how long it will sound, in milliseconds (0 if nothing will be heard).
    /// It does not wait for the cue to end, and it never fails: a cue that cannot be heard must not
    /// stop a recording.
    fn play(&self, cue: Cue) -> u64;
}

/// Plays nothing.
pub struct NoCues;

impl CuePlayer for NoCues {
    fn play(&self, _: Cue) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How strongly `hz` is present in the samples (a Goertzel filter).
    fn strength(samples: &[i16], hz: f64) -> f64 {
        let w = 2.0 * PI * hz / CUE_RATE as f64;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0, 0.0);
        for v in samples {
            let s = *v as f64 / 32768.0 + coeff * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        ((s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0)).sqrt() / samples.len() as f64
    }

    #[test]
    fn every_cue_is_short() {
        for cue in Cue::ALL {
            assert!(cue.duration_ms() < 400, "{} is {} ms", cue.name(), cue.duration_ms());
            assert!(cue.duration_ms() >= 50, "{}", cue.name());
        }
    }

    #[test]
    fn a_cue_is_as_long_as_it_says() {
        for cue in Cue::ALL {
            let samples = render(cue, 100);
            let expected = cue.duration_ms() * CUE_RATE as u64 / 1000;
            assert_eq!(samples.len() as u64, expected, "{}", cue.name());
        }
    }

    #[test]
    fn the_volume_sets_how_loud_and_zero_is_silent() {
        let peak = |volume| render(Cue::Start, volume).iter().map(|s| s.unsigned_abs()).max().unwrap() as f64 / 32768.0;
        assert_eq!(peak(0), 0.0, "volume 0 is silent");
        assert!(peak(25) < peak(50) && peak(50) < peak(70) && peak(70) < peak(100));
        assert!(peak(100) <= LOUDEST + 0.001, "never louder than the design allows, got {}", peak(100));
        assert!(peak(100) > 0.4, "audible at full volume, got {}", peak(100));
        assert_eq!(render(Cue::Stop, 250), render(Cue::Stop, 100), "more than 100 counts as 100");
        let default = peak(DEFAULT_VOLUME);
        assert!((0.15..0.35).contains(&default), "the default is a comfortable level, got {default}");
    }

    #[test]
    fn no_cue_clicks() {
        for cue in Cue::ALL {
            let s = render(cue, 100);
            assert_eq!((s[0], *s.last().unwrap()), (0, 0), "{} starts and ends in silence", cue.name());
            let biggest_jump = s.windows(2).map(|w| (w[1] as i32 - w[0] as i32).abs()).max().unwrap() as f64 / 32768.0;
            assert!(biggest_jump < 0.12, "{} jumps by {biggest_jump}", cue.name());
        }
    }

    #[test]
    fn each_note_really_has_the_pitch_it_is_written_with() {
        for cue in Cue::ALL {
            let samples = render(cue, 100);
            let mut at = 0usize;
            for step in cue.steps() {
                let len = (CUE_RATE as u64 * step.ms as u64 / 1000) as usize;
                let part = &samples[at..at + len];
                at += len;
                if step.hz == 0.0 {
                    assert!(part.iter().all(|s| *s == 0), "a rest is silent");
                    continue;
                }
                // The written pitch must be clearly stronger than any other pitch used by any cue.
                let own = strength(part, step.hz);
                for other in
                    Cue::ALL.iter().flat_map(|c| c.steps()).filter(|o| o.hz > 0.0 && (o.hz - step.hz).abs() > 40.0)
                {
                    let theirs = strength(part, other.hz);
                    assert!(
                        own > theirs * 2.0,
                        "{}: {} Hz is {own:.4}, but {} Hz is {theirs:.4}",
                        cue.name(),
                        step.hz,
                        other.hz
                    );
                }
            }
        }
    }

    /// What a listener can tell apart without comparing pitches from memory: how many notes, whether
    /// they rise, fall or stay, and whether the sound is soft or rough.
    fn shape(cue: Cue) -> (usize, i32, bool) {
        let notes: Vec<&Step> = cue.steps().iter().filter(|s| s.hz > 0.0).collect();
        let direction = match (notes.first(), notes.last()) {
            (Some(a), Some(b)) if b.hz > a.hz => 1,
            (Some(a), Some(b)) if b.hz < a.hz => -1,
            _ => 0,
        };
        (notes.len(), direction, notes.iter().any(|n| n.tone == Tone::Buzz))
    }

    #[test]
    fn no_two_cues_have_the_same_shape() {
        for (i, a) in Cue::ALL.iter().enumerate() {
            for b in &Cue::ALL[i + 1..] {
                assert_ne!(shape(*a), shape(*b), "{} and {} would sound alike", a.name(), b.name());
            }
        }
    }

    #[test]
    fn on_is_rising_and_off_is_falling() {
        assert_eq!(shape(Cue::Start).1, 1);
        assert_eq!(shape(Cue::Resume).1, 1);
        assert_eq!(shape(Cue::Stop).1, -1);
        assert_eq!(shape(Cue::Pause).1, -1);
        assert_eq!(shape(Cue::Start).0, 3);
        assert_eq!(shape(Cue::Pause).0, 2);
    }

    #[test]
    fn the_sound_that_says_what_you_do_is_noted_is_a_hill_and_is_neither_on_nor_off() {
        // Four notes, the first and last the same: not rising like on, not falling like off.
        assert_eq!(shape(Cue::Watching), (4, 0, false));
        let notes: Vec<f64> = Cue::Watching.steps().iter().map(|s| s.hz).collect();
        assert!(notes[0] < notes[1] && notes[1] == notes[2] && notes[2] > notes[3]);
    }

    #[test]
    fn the_error_is_the_only_rough_one_and_the_lowest() {
        let lowest = |cue: Cue| cue.steps().iter().filter(|s| s.hz > 0.0).map(|s| s.hz).fold(f64::MAX, f64::min);
        for cue in Cue::ALL.iter().filter(|c| **c != Cue::Error) {
            assert!(!shape(*cue).2);
            assert!(lowest(Cue::Error) < lowest(*cue));
        }
        assert!(shape(Cue::Error).2);
    }

    #[test]
    fn every_cue_is_described_in_words() {
        for cue in Cue::ALL {
            assert!(cue.describe().len() > 10 && !cue.name().is_empty());
        }
    }

    #[test]
    fn the_wav_bytes_are_a_valid_small_file() {
        let samples = render(Cue::Pause, 70);
        let wav = to_wav(&samples);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize, wav.len() - 8);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1, "one channel");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), CUE_RATE);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize, samples.len() * 2);
        assert_eq!(wav.len(), 44 + samples.len() * 2);
    }

    #[test]
    fn nothing_is_played_by_the_silent_player() {
        assert_eq!(NoCues.play(Cue::Start), 0);
    }
}
