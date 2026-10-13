// SPDX-License-Identifier: Apache-2.0
//! Sound: the microphone and the sound the computer itself is playing (including a screen reader's speech).
//!
//! Windows' audio service (WASAPI) hands over sound in small packets, but only when there is sound:
//! the "what the computer is playing" stream sends nothing at all while the computer is silent. A
//! recording needs a sound track exactly as long as the picture, so [`AudioAligner`] lays what
//! arrived onto the recording clock, filling silence where nothing came and dropping the odd
//! surplus, so sound and picture can never drift apart by more than a few hundredths of a second.
//!
//! As with the screen, the real Windows parts sit behind traits ([`AudioPlatform`], [`AudioSource`])
//! so the whole flow, and every message a person will read, is tested here with fakes.

use crate::{check, Check, ScreenError};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Sound is recorded at this many samples per second, per channel.
pub const SAMPLE_RATE: u32 = 48_000;
/// Sound is recorded in stereo. (A microphone with one channel is spread over both.)
pub const CHANNELS: u16 = 2;

/// What a sound stream is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInfo {
    /// What Windows calls the device, such as "Microphone (Realtek Audio)".
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Something sound comes from.
pub trait AudioSource {
    fn info(&self) -> &AudioInfo;
    /// Everything captured since the last call: interleaved 16-bit samples at `info().sample_rate`.
    fn read(&mut self) -> Vec<i16>;
    /// How many times Windows reported that some sound was lost.
    fn discontinuities(&self) -> u64;
    /// What stopped the capture, if something did, in plain words.
    fn failure(&self) -> Option<String>;
}

/// Everything the sound checks need from the operating system.
pub trait AudioPlatform {
    /// Start listening to the default microphone.
    fn start_microphone(&self) -> Result<Box<dyn AudioSource>, ScreenError>;
    /// Start listening to what the computer is playing (the default sound output).
    fn start_system_audio(&self) -> Result<Box<dyn AudioSource>, ScreenError>;
}

/// The real thing on Windows; on any other system a stand-in that says so.
pub fn system_audio_platform() -> Box<dyn AudioPlatform> {
    #[cfg(windows)]
    {
        Box::new(crate::win_audio::WindowsAudio)
    }
    #[cfg(not(windows))]
    {
        Box::new(NoAudio)
    }
}

#[cfg(not(windows))]
struct NoAudio;

#[cfg(not(windows))]
impl AudioPlatform for NoAudio {
    fn start_microphone(&self) -> Result<Box<dyn AudioSource>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn start_system_audio(&self) -> Result<Box<dyn AudioSource>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
}

/// How loud the loudest moment was, from 0.0 (silence) to 1.0 (as loud as the format allows).
pub fn peak(samples: &[i16]) -> f32 {
    samples.iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
}

/// Add two sound tracks together, so they can be heard as one. Too-loud sums are held at the loudest
/// value the format allows, never wrapped around into noise. The shorter track is treated as silent after it ends.
pub fn mix(a: &[i16], b: &[i16]) -> Vec<i16> {
    let len = a.len().max(b.len());
    (0..len)
        .map(|i| {
            let sum = a.get(i).copied().unwrap_or(0) as i32 + b.get(i).copied().unwrap_or(0) as i32;
            sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16
        })
        .collect()
}

/// Write interleaved 16-bit sound as an ordinary WAV file, which any player can open.
pub fn write_wav(out: &mut impl Write, sample_rate: u32, channels: u16, samples: &[i16]) -> io::Result<()> {
    let data_bytes = samples.len() as u64 * 2;
    if data_bytes > u32::MAX as u64 - 36 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the sound is too long for a WAV file"));
    }
    let block_align = channels as u32 * 2;
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_bytes as u32).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?; // plain PCM
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&(sample_rate * block_align).to_le_bytes())?;
    out.write_all(&(block_align as u16).to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&(data_bytes as u32).to_le_bytes())?;
    for s in samples {
        out.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Lays captured sound onto the recording clock.
///
/// Ask it for the sound due up to some recording time: it returns exactly the sound that belongs
/// there, taking what was captured, filling silence if the stream sent nothing, and dropping a surplus
/// if the sound card's clock runs a little fast. The recording clock does not run while paused, so
/// sound captured during a pause is thrown away with [`AudioAligner::discard_pending`].
#[derive(Debug, Clone)]
pub struct AudioAligner {
    rate: u32,
    channels: u16,
    pending: Vec<i16>,
    delivered_frames: u64,
    padded_frames: u64,
    trimmed_frames: u64,
}

impl AudioAligner {
    /// Sound later than this is waited for, not replaced by silence (packets arrive in small bursts).
    const LATE_MS: u64 = 40;
    /// More sound than this ahead of the clock is trimmed back (the sound card's clock runs fast).
    const AHEAD_MS: u64 = 80;

    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            rate: sample_rate,
            channels: channels.max(1),
            pending: Vec::new(),
            delivered_frames: 0,
            padded_frames: 0,
            trimmed_frames: 0,
        }
    }

    fn frames(&self, ms: u64) -> u64 {
        ms * self.rate as u64 / 1000
    }

    /// Sound just captured, interleaved.
    pub fn push(&mut self, samples: &[i16]) {
        self.pending.extend_from_slice(samples);
    }

    /// The sound due up to `media_ms` of recording time that has not been handed out yet.
    pub fn take_until(&mut self, media_ms: u64) -> Vec<i16> {
        let channels = self.channels as usize;
        let target = self.frames(media_ms);
        let need = target.saturating_sub(self.delivered_frames);
        let have = (self.pending.len() / channels) as u64;
        let mut out = Vec::new();

        if have >= need {
            out.extend(self.pending.drain(..need as usize * channels));
            // Too far ahead: the sound card's clock is running fast. Drop the oldest of the surplus.
            let ahead = (self.pending.len() / channels) as u64;
            if ahead > self.frames(Self::AHEAD_MS) {
                let drop_frames = ahead - self.frames(Self::LATE_MS);
                self.pending.drain(..drop_frames as usize * channels);
                self.trimmed_frames += drop_frames;
            }
            self.delivered_frames += need;
        } else {
            let deficit = need - have;
            out.append(&mut self.pending);
            if deficit > self.frames(Self::LATE_MS) {
                // The stream sent nothing for a while (a quiet computer): fill the gap with silence.
                out.extend(std::iter::repeat_n(0i16, deficit as usize * channels));
                self.padded_frames += deficit;
                self.delivered_frames += need;
            } else {
                // Just a little late: hand over what there is and wait for the rest.
                self.delivered_frames += have;
            }
        }
        out
    }

    /// Throw away sound captured but not yet used (while paused, the recording clock stands still).
    pub fn discard_pending(&mut self) {
        self.pending.clear();
    }

    /// How much sound has been handed out, as seconds.
    pub fn delivered_ms(&self) -> u64 {
        self.delivered_frames * 1000 / self.rate as u64
    }

    /// Silence added because nothing arrived, in milliseconds.
    pub fn padded_ms(&self) -> u64 {
        self.padded_frames * 1000 / self.rate as u64
    }

    /// Sound dropped because it ran ahead of the clock, in milliseconds.
    pub fn trimmed_ms(&self) -> u64 {
        self.trimmed_frames * 1000 / self.rate as u64
    }
}

/// What to try.
#[derive(Debug, Clone)]
pub struct AudioTestOptions {
    /// How long to listen.
    pub seconds: f64,
    /// Where to save `mic.wav`, `system.wav`, and `both.wav`.
    pub out_dir: PathBuf,
}

/// One stream being listened to during the test.
struct Listening {
    label: &'static str,
    check: &'static str,
    advice: &'static str,
    source: Box<dyn AudioSource>,
    aligner: AudioAligner,
    /// Everything the aligner handed out: what a recording would hold.
    track: Vec<i16>,
    /// How much sound Windows really delivered (before any silence is filled in).
    captured: u64,
    peak: f32,
}

/// Run the sound test on this computer.
pub fn run_audio_test(opts: &AudioTestOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_audio_test_with(system_audio_platform().as_ref(), opts, report)
}

/// Run the sound test against any platform. Every check is reported as soon as it is known.
/// Returns whether all of them were fine.
pub fn run_audio_test_with(
    platform: &dyn AudioPlatform,
    opts: &AudioTestOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_ok = true;
    let mut say = |c: Check| {
        all_ok &= c.ok;
        report(c);
    };

    let mut streams: Vec<Listening> = Vec::new();
    /// A stream to try: how to say it, its check name, advice if it is silent, and whether it started.
    type Candidate = (&'static str, &'static str, &'static str, Result<Box<dyn AudioSource>, ScreenError>);
    let candidates: [Candidate; 2] = [
        (
            "microphone",
            "microphone",
            "Speak during the test, and check that the microphone is not muted. Windows can also hand over silence when its privacy setting blocks desktop programs: open Settings, Privacy and security, Microphone, and turn on \"Let desktop apps access your microphone\".",
            platform.start_microphone(),
        ),
        (
            "system sound",
            "system-audio",
            "Play something on the computer during the test, such as a video or a sound.",
            platform.start_system_audio(),
        ),
    ];
    for (label, name, advice, started) in candidates {
        match started {
            Ok(source) => {
                let info = source.info().clone();
                say(check(
                    &format!("{name}-start"),
                    true,
                    format!(
                        "Listening to the {label}: {} ({} samples per second, {} channels).",
                        info.device, info.sample_rate, info.channels
                    ),
                ));
                streams.push(Listening {
                    label,
                    check: name,
                    advice,
                    aligner: AudioAligner::new(info.sample_rate, info.channels),
                    source,
                    track: Vec::new(),
                    captured: 0,
                    peak: 0.0,
                });
            }
            Err(e) => say(check(&format!("{name}-start"), false, format!("Could not listen to the {label}: {e}"))),
        }
    }
    if streams.is_empty() {
        return false;
    }

    let run_for = Duration::from_secs_f64(opts.seconds.clamp(1.0, 600.0));
    let started = Instant::now();
    while started.elapsed() < run_for {
        sleep(Duration::from_millis(20));
        let media_ms = started.elapsed().as_millis() as u64;
        for s in &mut streams {
            let got = s.source.read();
            s.captured += got.len() as u64 / s.aligner_channels() as u64;
            s.peak = s.peak.max(peak(&got));
            s.aligner.push(&got);
            let due = s.aligner.take_until(media_ms);
            s.track.extend(due);
        }
    }
    let wall_ms = started.elapsed().as_millis() as u64;

    for s in &streams {
        let rate = s.source.info().sample_rate as f64;
        let heard_seconds = s.captured as f64 / rate;
        let discontinuities = s.source.discontinuities();
        let broke = s.source.failure();
        let words = openstage_capture::describe_level(s.peak);
        let loss = if discontinuities > 0 {
            format!(" Windows reported {discontinuities} moments where some sound was lost.")
        } else {
            String::new()
        };
        if let Some(why) = broke {
            say(check(s.check, false, format!("Listening to the {} stopped: {why}", s.label)));
        } else if s.peak < 0.01 {
            say(check(
                s.check,
                false,
                format!(
                    "Heard nothing from the {} ({:.1} seconds of sound arrived, all silent). {}{loss}",
                    s.label, heard_seconds, s.advice
                ),
            ));
        } else {
            say(check(
                s.check,
                true,
                format!(
                    "Heard the {}: the loudest moment was {words} (peak {:.2}). Sound arrived for {:.1} of the {:.1} seconds.{loss}",
                    s.label,
                    s.peak,
                    heard_seconds,
                    wall_ms as f64 / 1000.0
                ),
            ));
        }
    }

    // The microphone delivers sound all the time, so its sound card's clock can be compared with the computer's.
    if let Some(mic) = streams.iter().find(|s| s.check == "microphone") {
        let seconds = mic.captured as f64 / mic.source.info().sample_rate as f64;
        let diff_ms = (seconds * 1000.0 - wall_ms as f64).abs();
        let message = format!(
            "The microphone delivered {:.2} seconds of sound in {:.2} seconds of real time (difference {:.0} milliseconds). Silence was added for {} ms and {} ms was dropped to keep the sound in step.",
            seconds,
            wall_ms as f64 / 1000.0,
            diff_ms,
            mic.aligner.padded_ms(),
            mic.aligner.trimmed_ms()
        );
        if diff_ms <= 250.0 {
            say(check("audio-clock", true, message));
        } else {
            say(check(
                "audio-clock",
                false,
                format!("{message} A difference this large means sound and picture could drift apart. Please tell the author."),
            ));
        }
    }

    // Save what a recording would hold: every track as long as the real time, gaps filled with silence.
    let saved = fs::create_dir_all(&opts.out_dir).and_then(|_| {
        let mut files: Vec<PathBuf> = Vec::new();
        let save = |name: &str, samples: &[i16]| -> io::Result<PathBuf> {
            let path = opts.out_dir.join(name);
            let mut file = io::BufWriter::new(fs::File::create(&path)?);
            write_wav(&mut file, crate::audio::SAMPLE_RATE, crate::audio::CHANNELS, samples)?;
            file.flush()?;
            Ok(path)
        };
        let mic = streams.iter().find(|s| s.check == "microphone").map(|s| s.track.as_slice());
        let system = streams.iter().find(|s| s.check == "system-audio").map(|s| s.track.as_slice());
        if let Some(m) = mic {
            files.push(save("mic.wav", m)?);
        }
        if let Some(sys) = system {
            files.push(save("system.wav", sys)?);
        }
        if let (Some(m), Some(sys)) = (mic, system) {
            files.push(save("both.wav", &mix(m, sys))?);
        }
        Ok(files)
    });
    match saved {
        Ok(files) => say(check(
            "audio-saved",
            true,
            format!(
                "Saved {}. Open them and listen: you should hear yourself in mic.wav and what the computer played in system.wav.",
                files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
        )),
        Err(e) => say(check("audio-saved", false, format!("Could not save the sound files: {e}"))),
    }

    all_ok
}

impl Listening {
    fn aligner_channels(&self) -> u16 {
        self.source.info().channels.max(1)
    }
}

#[allow(dead_code)]
fn _path_is_used(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    const R: u32 = 48_000;

    fn stereo_frames(n: usize, value: i16) -> Vec<i16> {
        vec![value; n * 2]
    }

    // ---- peak, mix, wav ----

    #[test]
    fn the_loudest_moment_is_found_from_silence_to_full() {
        assert_eq!(peak(&[]), 0.0);
        assert_eq!(peak(&[0, 0, 0]), 0.0);
        assert!((peak(&[100, -16384, 50]) - 0.5).abs() < 0.001);
        assert_eq!(peak(&[i16::MIN]), 1.0, "the most negative value must not overflow");
    }

    #[test]
    fn mixing_adds_tracks_and_never_wraps_around() {
        assert_eq!(mix(&[100, 200], &[1, 2]), vec![101, 202]);
        assert_eq!(mix(&[30000], &[30000]), vec![i16::MAX], "held at the loudest, not wrapped to noise");
        assert_eq!(mix(&[-30000], &[-30000]), vec![i16::MIN]);
        assert_eq!(mix(&[5, 6, 7], &[1]), vec![6, 6, 7], "the shorter track counts as silent after it ends");
    }

    #[test]
    fn a_wav_file_has_the_headers_any_player_expects() {
        let mut out = Vec::new();
        write_wav(&mut out, 48_000, 2, &[1, -1, 2, -2]).unwrap();
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([out[22], out[23]]), 2, "channels");
        assert_eq!(u32::from_le_bytes([out[24], out[25], out[26], out[27]]), 48_000, "rate");
        assert_eq!(u16::from_le_bytes([out[34], out[35]]), 16, "bits");
        assert_eq!(&out[36..40], b"data");
        assert_eq!(u32::from_le_bytes([out[40], out[41], out[42], out[43]]), 8);
        assert_eq!(out.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes([out[4], out[5], out[6], out[7]]) as usize, out.len() - 8);
        assert_eq!(i16::from_le_bytes([out[46], out[47]]), -1);
    }

    // ---- the aligner ----

    #[test]
    fn sound_that_arrives_on_time_is_handed_over_exactly() {
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(4800, 7));
        let out = a.take_until(100);
        assert_eq!(out.len(), 4800 * 2);
        assert!(out.iter().all(|s| *s == 7));
        assert_eq!((a.padded_ms(), a.trimmed_ms(), a.delivered_ms()), (0, 0, 100));
    }

    #[test]
    fn sound_a_little_late_is_waited_for_not_replaced_by_silence() {
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(960, 5)); // 20 ms of the 30 ms due
        let first = a.take_until(30);
        assert_eq!(first.len(), 960 * 2, "only what exists");
        assert_eq!(a.padded_ms(), 0, "a 10 ms delay is not a gap");
        a.push(&stereo_frames(480, 5));
        let second = a.take_until(30);
        assert_eq!(second.len(), 480 * 2, "the rest arrived and is handed over");
        assert_eq!(a.delivered_ms(), 30);
    }

    #[test]
    fn a_stream_that_sends_nothing_is_filled_with_silence_so_the_track_stays_as_long_as_the_recording() {
        let mut a = AudioAligner::new(R, 2);
        let mut total = 0;
        for ms in (20..=2000).step_by(20) {
            total += a.take_until(ms).len();
        }
        let seconds = total as f64 / 2.0 / R as f64;
        assert!((1.95..=2.0).contains(&seconds), "two seconds of silence, got {seconds}");
        assert!(a.padded_ms() >= 1950);
    }

    #[test]
    fn a_sound_card_clock_that_runs_fast_is_trimmed_back_in_step() {
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(R as usize * 3, 9)); // three seconds arrive at once
        let out = a.take_until(1000);
        assert_eq!(out.len(), R as usize * 2, "exactly the one second that is due");
        assert!(a.trimmed_ms() > 1800, "the surplus was dropped, got {} ms", a.trimmed_ms());
        // Only a short cushion is kept.
        let next = a.take_until(1100);
        assert_eq!(next.len(), 4800 * 2);
    }

    #[test]
    fn sound_captured_while_paused_is_thrown_away() {
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(4800, 1));
        assert_eq!(a.take_until(100).len(), 4800 * 2);
        // Paused: the clock stands still at 100 ms while sound keeps arriving.
        a.push(&stereo_frames(48_000, 2));
        a.discard_pending();
        // Resumed: the next 20 ms of recording gets fresh sound, none of the paused sound.
        a.push(&stereo_frames(960, 3));
        let out = a.take_until(120);
        assert_eq!(out.len(), 960 * 2);
        assert!(out.iter().all(|s| *s == 3));
    }

    #[test]
    fn one_channel_and_two_channel_streams_are_both_counted_in_whole_frames() {
        let mut mono = AudioAligner::new(R, 1);
        mono.push(&vec![4i16; 480]);
        assert_eq!(mono.take_until(10).len(), 480);
        let mut stereo = AudioAligner::new(R, 2);
        stereo.push(&vec![4i16; 960]);
        assert_eq!(stereo.take_until(10).len(), 960);
    }

    #[test]
    fn the_time_handed_out_never_goes_backwards_even_if_asked_for_an_earlier_time() {
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(4800, 1));
        a.take_until(100);
        assert!(a.take_until(50).is_empty());
        assert_eq!(a.delivered_ms(), 100);
    }

    // ---- the whole test, with fakes ----

    struct FakeSource {
        info: AudioInfo,
        started: Instant,
        given_frames: u64,
        /// How loud the generated tone is; 0 for a silent source.
        level: i16,
        /// Whether it delivers anything while silent (a microphone does; "what the computer plays" does not).
        sends_silence: bool,
        lost: u64,
        broken: Option<String>,
    }

    impl AudioSource for FakeSource {
        fn info(&self) -> &AudioInfo {
            &self.info
        }
        fn read(&mut self) -> Vec<i16> {
            if self.level == 0 && !self.sends_silence {
                return Vec::new();
            }
            let due = (self.started.elapsed().as_secs_f64() * self.info.sample_rate as f64) as u64;
            let n = due.saturating_sub(self.given_frames);
            self.given_frames += n;
            (0..n * self.info.channels as u64)
                .map(|i| {
                    if self.level == 0 {
                        0
                    } else if i % 8 < 4 {
                        self.level
                    } else {
                        -self.level
                    }
                })
                .collect()
        }
        fn discontinuities(&self) -> u64 {
            self.lost
        }
        fn failure(&self) -> Option<String> {
            self.broken.clone()
        }
    }

    struct FakeAudio {
        mic_level: i16,
        system_level: i16,
        mic_fails: bool,
        system_fails: bool,
        lost: Rc<Cell<u64>>,
    }

    fn fake(level: i16, sends_silence: bool, lost: u64, name: &str) -> Box<dyn AudioSource> {
        Box::new(FakeSource {
            info: AudioInfo { device: name.into(), sample_rate: R, channels: 2 },
            started: Instant::now(),
            given_frames: 0,
            level,
            sends_silence,
            lost,
            broken: None,
        })
    }

    impl AudioPlatform for FakeAudio {
        fn start_microphone(&self) -> Result<Box<dyn AudioSource>, ScreenError> {
            if self.mic_fails {
                return Err(ScreenError::Failed("no microphone is connected".into()));
            }
            Ok(fake(self.mic_level, true, self.lost.get(), "Fake Microphone"))
        }
        fn start_system_audio(&self) -> Result<Box<dyn AudioSource>, ScreenError> {
            if self.system_fails {
                return Err(ScreenError::Failed("no sound output".into()));
            }
            Ok(fake(self.system_level, false, 0, "Fake Speakers"))
        }
    }

    fn platform() -> FakeAudio {
        FakeAudio {
            mic_level: 8000,
            system_level: 4000,
            mic_fails: false,
            system_fails: false,
            lost: Rc::new(Cell::new(0)),
        }
    }

    fn run(p: &FakeAudio) -> (bool, Vec<Check>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions { seconds: 1.0, out_dir: dir.path().join("a") };
        let mut checks = Vec::new();
        let ok = run_audio_test_with(p, &opts, &mut |c| checks.push(c));
        (ok, checks, dir)
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn a_healthy_computer_hears_both_streams_and_saves_three_files_as_long_as_the_test() {
        let (ok, checks, dir) = run(&platform());
        assert!(ok, "{checks:?}");
        let names: Vec<_> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(
            names,
            ["microphone-start", "system-audio-start", "microphone", "system-audio", "audio-clock", "audio-saved"]
        );
        assert!(find(&checks, "microphone-start")
            .message
            .contains("Fake Microphone (48000 samples per second, 2 channels)"));
        assert!(find(&checks, "microphone").message.contains("Heard the microphone"));
        for name in ["mic.wav", "system.wav", "both.wav"] {
            let bytes = fs::read(dir.path().join("a").join(name)).unwrap();
            assert_eq!(&bytes[0..4], b"RIFF", "{name}");
            let seconds = (bytes.len() - 44) as f64 / 4.0 / R as f64;
            assert!((0.9..=1.1).contains(&seconds), "{name} is about as long as the test, got {seconds}");
        }
    }

    #[test]
    fn a_silent_microphone_and_a_silent_output_each_get_their_own_advice() {
        let mut p = platform();
        p.mic_level = 0;
        p.system_level = 0;
        let (ok, checks, dir) = run(&p);
        assert!(!ok);
        let mic = find(&checks, "microphone");
        assert!(
            !mic.ok && mic.message.contains("Heard nothing from the microphone") && mic.message.contains("not muted")
        );
        let sys = find(&checks, "system-audio");
        assert!(!sys.ok && sys.message.contains("Play something on the computer"));
        // The silent output still gives a track as long as the test: silence filled in.
        let bytes = fs::read(dir.path().join("a/system.wav")).unwrap();
        let seconds = (bytes.len() - 44) as f64 / 4.0 / R as f64;
        assert!((0.9..=1.1).contains(&seconds), "a quiet computer still gives a full-length track, got {seconds}");
    }

    #[test]
    fn a_missing_microphone_is_reported_and_the_system_sound_is_still_tested() {
        let mut p = platform();
        p.mic_fails = true;
        let (ok, checks, _dir) = run(&p);
        assert!(!ok);
        assert!(!find(&checks, "microphone-start").ok);
        assert!(find(&checks, "microphone-start").message.contains("no microphone is connected"));
        assert!(find(&checks, "system-audio").ok);
        assert!(checks.iter().all(|c| c.check != "audio-clock"), "no microphone, no clock comparison");
    }

    #[test]
    fn nothing_to_listen_to_at_all_stops_the_test() {
        let mut p = platform();
        p.mic_fails = true;
        p.system_fails = true;
        let (ok, checks, _dir) = run(&p);
        assert!(!ok);
        assert_eq!(checks.len(), 2);
    }

    #[test]
    fn lost_sound_reported_by_windows_is_mentioned() {
        let p = platform();
        p.lost.set(3);
        let (_ok, checks, _dir) = run(&p);
        assert!(find(&checks, "microphone").message.contains("3 moments where some sound was lost"));
    }

    #[cfg(not(windows))]
    #[test]
    fn on_other_systems_the_sound_test_says_it_needs_windows() {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions { seconds: 1.0, out_dir: dir.path().join("a") };
        let mut checks = Vec::new();
        assert!(!run_audio_test(&opts, &mut |c| checks.push(c)));
        assert!(checks[0].message.contains("only works on Windows"));
    }
}
