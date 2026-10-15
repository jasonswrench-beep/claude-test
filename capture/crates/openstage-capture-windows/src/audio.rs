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
use std::path::PathBuf;
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
    /// Everything captured since the last call, as packets that say when they were captured. The
    /// default knows no times; sources that can say when sound was captured override it.
    fn read_packets(&mut self) -> Vec<AudioPacket> {
        let samples = self.read();
        if samples.is_empty() {
            Vec::new()
        } else {
            vec![AudioPacket { samples, captured_100ns: None, arrived_100ns: None }]
        }
    }
    /// While paused, the source keeps listening (a microphone cannot be paused) but throws away what it
    /// hears, so a long pause does not fill the computer's memory. Sources that stop by themselves
    /// need not do anything.
    fn set_paused(&mut self, _paused: bool) {}
}

/// Sound captured in one go, with the time it was captured.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPacket {
    /// Interleaved 16-bit samples.
    pub samples: Vec<i16>,
    /// When the first sample was captured, on the clock Windows also uses for pictures (units of
    /// 100 nanoseconds). `None` when unknown.
    pub captured_100ns: Option<i64>,
    /// When the packet reached this program, on the same clock. `None` when not measured.
    pub arrived_100ns: Option<i64>,
}

/// What kind of sound device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Microphone,
    /// Something that plays sound (speakers, headphones, a monitor's speakers).
    SoundOutput,
}

impl DeviceKind {
    fn label(self) -> &'static str {
        match self {
            DeviceKind::Microphone => "microphone",
            DeviceKind::SoundOutput => "sound output",
        }
    }
}

/// One microphone or sound output the computer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub kind: DeviceKind,
    /// 1, 2, 3 ... within its kind, in the order Windows lists them.
    pub number: u32,
    /// What Windows calls it, such as "Speakers (Realtek(R) Audio)".
    pub name: String,
    /// Whether Windows uses it when nothing else is chosen.
    pub is_default: bool,
    /// What the platform needs to open exactly this device.
    pub id: String,
}

/// Which device to use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AudioChoice {
    /// The one Windows uses by default.
    #[default]
    Default,
    /// A number from the list.
    Number(u32),
    /// Whichever one has this text in its name (capital letters do not matter).
    NameContains(String),
}

impl AudioChoice {
    /// From what a person typed: nothing or "default", a number from the list, or part of a name.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("default") {
            return AudioChoice::Default;
        }
        match text.parse::<u32>() {
            Ok(n) => AudioChoice::Number(n),
            Err(_) => AudioChoice::NameContains(text.to_string()),
        }
    }
}

/// Pick the device a person asked for, or say why that cannot be done, in words.
pub fn choose_device<'a>(
    devices: &'a [AudioDevice],
    kind: DeviceKind,
    choice: &AudioChoice,
) -> Result<&'a AudioDevice, String> {
    let of_kind: Vec<&AudioDevice> = devices.iter().filter(|d| d.kind == kind).collect();
    let label = kind.label();
    let Some(first) = of_kind.first() else {
        return Err(format!("No {label} was found on this computer."));
    };
    match choice {
        AudioChoice::Default => Ok(of_kind.iter().find(|d| d.is_default).copied().unwrap_or(first)),
        AudioChoice::Number(n) => of_kind.iter().find(|d| d.number == *n).copied().ok_or_else(|| {
            format!(
                "There is no {label} number {n}. Choose a number from 1 to {}, or leave it out to use the default.",
                of_kind.len()
            )
        }),
        AudioChoice::NameContains(text) => {
            let wanted = text.to_lowercase();
            let hits: Vec<&&AudioDevice> = of_kind.iter().filter(|d| d.name.to_lowercase().contains(&wanted)).collect();
            match hits.as_slice() {
                [] => Err(format!("No {label} has \"{text}\" in its name.")),
                [one] => Ok(**one),
                many => Err(format!(
                    "More than one {label} has \"{text}\" in its name: {}. Use its number instead.",
                    many.iter().map(|d| format!("{}, {}", d.number, d.name)).collect::<Vec<_>>().join("; ")
                )),
            }
        }
    }
}

/// The devices in words, with the default marked, for a person to choose from.
pub fn describe_devices(devices: &[AudioDevice]) -> String {
    let part = |kind: DeviceKind, title: &str| {
        let list: Vec<String> = devices
            .iter()
            .filter(|d| d.kind == kind)
            .map(|d| format!("{}, {}{}", d.number, d.name, if d.is_default { ", the default" } else { "" }))
            .collect();
        if list.is_empty() {
            format!("{title}: none found.")
        } else {
            format!("{title}: {}.", list.join("; "))
        }
    };
    format!("{} {}", part(DeviceKind::Microphone, "Microphones"), part(DeviceKind::SoundOutput, "Sound outputs"))
}

/// Everything the sound checks need from the operating system.
pub trait AudioPlatform {
    /// Every microphone and sound output the computer has, numbered.
    fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError>;
    /// Start listening to a microphone: the given one, or the default if `None`.
    fn start_microphone(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError>;
    /// Start listening to what a sound output is playing: the given one, or the default if `None`.
    fn start_system_audio(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError>;
    /// Which sound output Windows uses by default right now (`None` when it has none). It is asked about
    /// every moment or so during a recording, so a platform that can answer cheaply should override it.
    fn default_output_id(&self) -> Result<Option<String>, ScreenError> {
        Ok(self.list_devices()?.into_iter().find(|d| d.kind == DeviceKind::SoundOutput && d.is_default).map(|d| d.id))
    }
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
    fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn start_microphone(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
        Err(ScreenError::NotWindows)
    }
    fn start_system_audio(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
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
    /// The most sound ever held back ahead of the recording clock (the sound card running fast).
    worst_ahead_frames: u64,
    /// The most sound ever missing from the recording clock while waiting for it (the sound card running slow or late).
    worst_late_frames: u64,
    /// Recording time at which each of those worst moments happened, in milliseconds.
    worst_ahead_at_ms: u64,
    worst_late_at_ms: u64,
    /// How many times sound was asked for, and on how many of them the sound was held back, or was
    /// missing, by more than `LATE_MS`. A single moment over the line is a blip; what matters for
    /// keeping sound with the picture is how often it happens.
    looks: u64,
    looks_over_ahead: u64,
    looks_over_late: u64,
    /// The current stretch of recording time over which the lowest amount of sound waiting beyond the clock
    /// is being found: when it ends, how many looks it held, and the lowest amount (in frames, `u64::MAX`
    /// before any look). Sound that is always waiting, even at its lowest, is the sound card running fast.
    window_end_ms: Option<u64>,
    window_looks: u32,
    window_lowest: u64,
    /// The lowest amount of the stretch before, if it held enough looks to count.
    previous_lowest: Option<u64>,
    /// The last frame handed out, so that where sound is cut out the join can be smoothed.
    last_frame: Vec<i16>,
    /// A slide still to be laid over sound about to be handed out, after a cut: per channel, how far the
    /// sound after the cut was from where the sound before it ended, and how many frames of the slide are done.
    glide: Option<(Vec<i32>, usize)>,
}

impl AudioAligner {
    /// Sound later than this is waited for, not replaced by silence (packets arrive in small bursts).
    const LATE_MS: u64 = 40;
    /// A sound card whose clock runs fast delivers a little more sound than the recording clock asks for, so
    /// sound waits, and the sound in the recording plays later and later than its picture. The lowest amount
    /// waiting over a stretch of this much recording time is the part that is always there (a burst of
    /// packets shows as a peak above it and is not counted).
    const WINDOW_MS: u64 = 2_000;
    /// Fewest looks a stretch must hold to count; a stretch of one or two looks says nothing.
    const WINDOW_MIN_LOOKS: u32 = 20;
    /// When the amount that is always waiting stays above this for two stretches in a row, it is cut back.
    /// The requirement is that sound stays within 40 ms of its picture, and bursts add to what is waiting, so
    /// the always-waiting part is kept well below that.
    const TRIM_ABOVE_MS: u64 = 20;
    /// What is left waiting after a cut: a cushion for bursts.
    const KEEP_MS: u64 = 5;
    /// Over this many frames the join where sound was cut is smoothed (10 ms): a slide, not a step.
    const SMOOTH_FRAMES: usize = 480;

    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            rate: sample_rate,
            channels: channels.max(1),
            pending: Vec::new(),
            delivered_frames: 0,
            padded_frames: 0,
            trimmed_frames: 0,
            worst_ahead_frames: 0,
            worst_late_frames: 0,
            worst_ahead_at_ms: 0,
            worst_late_at_ms: 0,
            looks: 0,
            looks_over_ahead: 0,
            looks_over_late: 0,
            window_end_ms: None,
            window_looks: 0,
            window_lowest: u64::MAX,
            previous_lowest: None,
            last_frame: Vec::new(),
            glide: None,
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
        self.looks += 1;
        let waiting_after;

        if have >= need {
            out.extend(self.pending.drain(..need as usize * channels));
            // Sound still waiting beyond the clock: it plays later than its picture by this much.
            let ahead = (self.pending.len() / channels) as u64;
            if ahead > self.worst_ahead_frames {
                self.worst_ahead_frames = ahead;
                self.worst_ahead_at_ms = media_ms;
            }
            if ahead > self.frames(Self::LATE_MS) {
                self.looks_over_ahead += 1;
            }
            waiting_after = ahead;
            self.delivered_frames += need;
        } else {
            let deficit = need - have;
            if deficit > self.worst_late_frames {
                self.worst_late_frames = deficit;
                self.worst_late_at_ms = media_ms;
            }
            if deficit > self.frames(Self::LATE_MS) {
                self.looks_over_late += 1;
            }
            // Nothing waits beyond the clock now.
            waiting_after = 0;
            if deficit > self.frames(Self::LATE_MS) {
                // The stream sent nothing for a while (a quiet computer, or the program was busy): fill the
                // gap with silence. The silence goes FIRST: sound that arrived after a quiet stretch belongs
                // at the end of the stretch, up against the present, not at its start.
                out.extend(std::iter::repeat_n(0i16, deficit as usize * channels));
                out.append(&mut self.pending);
                self.padded_frames += deficit;
                self.delivered_frames += need;
            } else {
                // Just a little late: hand over what there is and wait for the rest.
                out.append(&mut self.pending);
                self.delivered_frames += have;
            }
        }

        self.apply_glide(&mut out);
        // The last frame handed out comes first: if sound is cut below, the join is smoothed from it.
        if out.len() >= channels {
            self.last_frame.clear();
            self.last_frame.extend_from_slice(&out[out.len() - channels..]);
        }
        self.settle(media_ms, waiting_after);
        out
    }

    /// Note how much sound waited beyond the clock at this look, and at the end of each stretch of recording
    /// time cut back what has been waiting all along.
    ///
    /// A card whose clock runs fast has sound waiting that never goes away; a burst of packets, or the program
    /// being held up so that the clock it was given was out of date, makes a peak that is gone at the next look
    /// (a real recording once lost two seconds of sound by cutting such a peak). So only the LOWEST amount over
    /// a stretch counts, and only when two stretches in a row agree.
    fn settle(&mut self, media_ms: u64, waiting: u64) {
        let end = *self.window_end_ms.get_or_insert(media_ms + Self::WINDOW_MS);
        self.window_lowest = self.window_lowest.min(waiting);
        self.window_looks += 1;
        if media_ms < end {
            return;
        }
        let this = (self.window_looks >= Self::WINDOW_MIN_LOOKS).then_some(self.window_lowest);
        let always_there = match (self.previous_lowest, this) {
            (Some(before), Some(now)) => Some(before.min(now)),
            _ => None,
        };
        self.previous_lowest = this;
        self.window_end_ms = Some(media_ms + Self::WINDOW_MS);
        self.window_looks = 0;
        self.window_lowest = u64::MAX;
        let Some(always_there) = always_there else { return };
        if always_there <= self.frames(Self::TRIM_ABOVE_MS) {
            return;
        }
        let channels = self.channels as usize;
        let drop_frames = (always_there - self.frames(Self::KEEP_MS)).min((self.pending.len() / channels) as u64);
        if drop_frames == 0 {
            return;
        }
        self.pending.drain(..drop_frames as usize * channels);
        self.trimmed_frames += drop_frames;
        self.start_glide();
        // What was measured no longer holds.
        self.previous_lowest = None;
    }

    /// Where sound was cut out, the sound after the cut starts at a different level than the sound before it
    /// ended, which would be heard as a click. Start from the level where the last sound ended and slide to
    /// the real sound over ten milliseconds, as the sound is handed out (see [`AudioAligner::apply_glide`]).
    fn start_glide(&mut self) {
        let channels = self.channels as usize;
        if self.last_frame.len() != channels || self.pending.len() < channels {
            self.glide = None;
            return;
        }
        // A slide still going was laid over sound that has been handed out; this one starts from the level
        // that sound ended at, so it replaces it.
        let jump = (0..channels).map(|c| self.last_frame[c] as i32 - self.pending[c] as i32).collect();
        self.glide = Some((jump, 0));
    }

    /// Lay the slide that follows a cut over the sound being handed out.
    fn apply_glide(&mut self, out: &mut [i16]) {
        let channels = self.channels as usize;
        let Some((jump, done)) = self.glide.as_mut() else { return };
        for frame in out.chunks_exact_mut(channels) {
            if *done >= Self::SMOOTH_FRAMES {
                break;
            }
            let remaining = (Self::SMOOTH_FRAMES - *done) as i32;
            for (sample, jump) in frame.iter_mut().zip(jump.iter()) {
                let smoothed = *sample as i32 + jump * remaining / Self::SMOOTH_FRAMES as i32;
                *sample = smoothed.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
            *done += 1;
        }
        if *done >= Self::SMOOTH_FRAMES {
            self.glide = None;
        }
    }

    /// Throw away sound captured but not yet used (while paused, the recording clock stands still).
    pub fn discard_pending(&mut self) {
        self.pending.clear();
        // What was measured before the pause says nothing about the sound after it.
        self.window_end_ms = None;
        self.window_looks = 0;
        self.window_lowest = u64::MAX;
        self.previous_lowest = None;
        self.last_frame.clear();
        self.glide = None;
    }

    /// How much sound has been handed out, as seconds.
    pub fn delivered_ms(&self) -> u64 {
        self.delivered_frames * 1000 / self.rate as u64
    }

    /// The most sound that was ever held back ahead of the recording clock, in milliseconds: how far
    /// behind the picture the sound could be, as far as this counting can tell.
    pub fn worst_ahead_ms(&self) -> u64 {
        self.worst_ahead_frames * 1000 / self.rate as u64
    }

    /// The most sound that was ever missing at the recording clock while waiting for it, in milliseconds.
    pub fn worst_late_ms(&self) -> u64 {
        self.worst_late_frames * 1000 / self.rate as u64
    }

    /// When, in recording time, the sound was held back the most, in milliseconds.
    pub fn worst_ahead_at_ms(&self) -> u64 {
        self.worst_ahead_at_ms
    }

    /// When, in recording time, the sound was missing the most, in milliseconds.
    pub fn worst_late_at_ms(&self) -> u64 {
        self.worst_late_at_ms
    }

    /// How many times sound was asked for, and on how many the sound was held back (or missing) by more than 40 ms.
    pub fn looks(&self) -> u64 {
        self.looks
    }
    pub fn looks_over_later(&self) -> u64 {
        self.looks_over_ahead
    }
    pub fn looks_over_earlier(&self) -> u64 {
        self.looks_over_late
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
    /// Which microphone to listen to.
    pub microphone: AudioChoice,
    /// Which sound output to listen to.
    pub output: AudioChoice,
}

/// One stream being listened to during the test.
struct Listening {
    label: &'static str,
    check: &'static str,
    advice: &'static str,
    /// More to say if it hears nothing, such as which other devices there are.
    hint: String,
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

    // What devices there are, so the person can see which one is used and choose another.
    let devices = match platform.list_devices() {
        Ok(list) if list.is_empty() => {
            say(check("audio-devices", false, "No microphone or sound output was found on this computer."));
            Some(list)
        }
        Ok(list) => {
            say(check("audio-devices", true, describe_devices(&list)));
            Some(list)
        }
        Err(e) => {
            say(check("audio-devices", false, format!("Could not list the sound devices: {e}")));
            None
        }
    };
    let pick = |kind: DeviceKind, choice: &AudioChoice| -> Result<Option<AudioDevice>, String> {
        match &devices {
            Some(list) => choose_device(list, kind, choice).map(|d| Some(d.clone())),
            None if *choice == AudioChoice::Default => Ok(None),
            None => Err("The sound devices could not be listed, so a particular one cannot be chosen.".to_string()),
        }
    };
    let other_outputs = |chosen: &Option<AudioDevice>| -> String {
        let Some(list) = &devices else { return String::new() };
        let others: Vec<String> = list
            .iter()
            .filter(|d| d.kind == DeviceKind::SoundOutput && Some(&d.id) != chosen.as_ref().map(|c| &c.id))
            .map(|d| format!("{}, {}", d.number, d.name))
            .collect();
        if others.is_empty() {
            String::new()
        } else {
            format!(
                " Windows listens to one sound output at a time, the one chosen above. If the sound was playing through another one, run the test again with that one chosen. The other outputs are: {}.",
                others.join("; ")
            )
        }
    };

    let mut streams: Vec<Listening> = Vec::new();
    /// A stream to try: how to say it, its check name, advice and hint if it is silent, and whether it started.
    type Candidate = (&'static str, &'static str, &'static str, String, Result<Box<dyn AudioSource>, ScreenError>);
    let mic_pick = pick(DeviceKind::Microphone, &opts.microphone);
    let out_pick = pick(DeviceKind::SoundOutput, &opts.output);
    let out_hint = match &out_pick {
        Ok(chosen) => other_outputs(chosen),
        Err(_) => String::new(),
    };
    let candidates: [Candidate; 2] = [
        (
            "microphone",
            "microphone",
            "Speak during the test, and check that the microphone is not muted. Windows can also hand over silence when its privacy setting blocks desktop programs: open Settings, Privacy and security, Microphone, and turn on \"Let desktop apps access your microphone\".",
            String::new(),
            match mic_pick {
                Ok(device) => platform.start_microphone(device.as_ref()),
                Err(why) => Err(ScreenError::Failed(why)),
            },
        ),
        (
            "system sound",
            "system-audio",
            "Play something on the computer during the test, such as a video or a sound.",
            out_hint,
            match out_pick {
                Ok(device) => platform.start_system_audio(device.as_ref()),
                Err(why) => Err(ScreenError::Failed(why)),
            },
        ),
    ];
    for (label, name, advice, hint, started) in candidates {
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
                    hint,
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
            if discontinuities == 1 {
                " Windows reported 1 moment where some sound was lost (this often happens once as listening starts)."
                    .to_string()
            } else {
                format!(" Windows reported {discontinuities} moments where some sound was lost.")
            }
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
                    "Heard nothing from the {} ({:.1} seconds of sound arrived, all silent). {}{}{loss}",
                    s.label, heard_seconds, s.advice, s.hint
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

    // With both sounds, see whether the microphone is also hearing the computer's sound (an echo).
    let mic_track = streams.iter().find(|s| s.check == "microphone");
    let system_track = streams.iter().find(|s| s.check == "system-audio");
    if let (Some(m), Some(sy)) = (mic_track, system_track) {
        let mic_env = crate::echo::envelope_of(&m.track, SAMPLE_RATE, CHANNELS);
        let system_env = crate::echo::envelope_of(&sy.track, SAMPLE_RATE, CHANNELS);
        say(crate::echo::describe_echo(&crate::echo::detect_echo(&mic_env, &system_env)).check);
        // ...and how the two compare in loudness, since a voice that is quieter than the computer is hard to follow.
        say(crate::balance::balance_of_samples(&m.track, &sy.track, SAMPLE_RATE, CHANNELS).check);
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

/// What to try when looking for where the sound is playing.
#[derive(Debug, Clone)]
pub struct FindOutputOptions {
    /// How long to listen to all the outputs at once.
    pub seconds: f64,
}

/// Run the output finder on this computer.
pub fn run_find_output(opts: &FindOutputOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_find_output_with(system_audio_platform().as_ref(), opts, report)
}

/// Listen to every sound output at the same time and say which of them carry sound.
///
/// Windows records one sound output at a time, and a computer often has many (speakers, headphones,
/// monitors, virtual devices). When the sound you want is playing through one of them, this finds it.
pub fn run_find_output_with(
    platform: &dyn AudioPlatform,
    opts: &FindOutputOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let devices = match platform.list_devices() {
        Ok(list) => list,
        Err(e) => {
            report(check("audio-devices", false, format!("Could not list the sound devices: {e}")));
            return false;
        }
    };
    let outputs: Vec<&AudioDevice> = devices.iter().filter(|d| d.kind == DeviceKind::SoundOutput).collect();
    if outputs.is_empty() {
        report(check("audio-devices", false, "No sound output was found on this computer."));
        return false;
    }

    let mut listening: Vec<(&AudioDevice, Box<dyn AudioSource>, f32)> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for device in &outputs {
        match platform.start_system_audio(Some(device)) {
            Ok(source) => listening.push((device, source, 0.0)),
            Err(e) => failed.push(format!("{}, {} ({e})", device.number, device.name)),
        }
    }
    let tried = format!("Listened to {} of the {} sound outputs at the same time.", listening.len(), outputs.len());
    if listening.is_empty() {
        report(check(
            "outputs-tried",
            false,
            format!("{tried} None of them could be listened to: {}.", failed.join("; ")),
        ));
        return false;
    }
    let mut all_ok = true;
    if failed.is_empty() {
        report(check("outputs-tried", true, tried));
    } else {
        // Some devices (often virtual ones) cannot be listened to; that is worth knowing but not a failure.
        report(check("outputs-tried", true, format!("{tried} These could not be listened to: {}.", failed.join("; "))));
    }

    let run_for = Duration::from_secs_f64(opts.seconds.clamp(1.0, 120.0));
    let started = Instant::now();
    while started.elapsed() < run_for {
        sleep(Duration::from_millis(20));
        for (_, source, loudest) in &mut listening {
            *loudest = loudest.max(peak(&source.read()));
        }
    }

    let carrying: Vec<(&AudioDevice, f32)> =
        listening.iter().filter(|(_, _, p)| *p >= 0.01).map(|(d, _, p)| (*d, *p)).collect();
    if carrying.is_empty() {
        all_ok = false;
        report(check(
            "sound-found",
            false,
            format!(
                "No sound was heard on any of the {} outputs listened to. Start something playing (a video or some music), keep it playing, and run this again. If something was playing and this still heard nothing, please tell the author.",
                listening.len()
            ),
        ));
    } else {
        let list: Vec<String> = carrying
            .iter()
            .map(|(d, p)| {
                format!("{}, {} (level {}, peak {p:.2})", d.number, d.name, openstage_capture::describe_level(*p))
            })
            .collect();
        let advice = if carrying.len() == 1 {
            format!("To record it, choose sound output number {}.", carrying[0].0.number)
        } else {
            "More than one output carries sound. Some are virtual devices that copy another. Choose the one you listen through.".to_string()
        };
        report(check("sound-found", true, format!("Sound is coming out of: {}. {advice}", list.join("; "))));
    }
    all_ok
}

impl Listening {
    fn aligner_channels(&self) -> u16 {
        self.source.info().channels.max(1)
    }
}

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
    fn sound_that_starts_after_a_quiet_stretch_is_placed_after_the_silence_not_before() {
        // A quiet computer sends nothing, then sound starts partway through the interval that is due: 800 ms
        // of nothing and then 200 ms of sound. The sound happened at the END of the second.
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(9600, 5));
        let out = a.take_until(1000);
        assert_eq!(out.len(), 48_000 * 2, "exactly the second that is due");
        let first_sound = out.iter().position(|s| *s == 5).expect("the sound is in there");
        assert_eq!(first_sound / 2, 38_400, "800 ms of silence come first");
        assert!(out[..first_sound].iter().all(|s| *s == 0));
        assert!(out[first_sound..].iter().all(|s| *s == 5), "the sound runs to the end of the second");
        assert_eq!((a.worst_late_ms(), a.worst_late_at_ms()), (800, 1000), "when and how much was missing is noted");
    }

    /// Play a sound card against the recording clock: the clock is looked at every `look_ms`, and the card
    /// delivers its sound in packets of `packet_ms`, at `ppm` parts per million faster than the clock (negative:
    /// slower). Returns what was handed out (as a count of frames) and the aligner.
    fn run_card(ppm: f64, seconds: u64, look_ms: u64, packet_ms: u64) -> (u64, AudioAligner) {
        let mut a = AudioAligner::new(R, 2);
        let per_ms = 48.0 * (1.0 + ppm / 1_000_000.0);
        let (mut pushed, mut handed) = (0u64, 0u64);
        let mut t = 0;
        while t < seconds * 1000 {
            t += look_ms;
            // Packets that have been completed by now.
            let complete = (t / packet_ms) * packet_ms;
            let have = (complete as f64 * per_ms) as u64;
            if have > pushed {
                a.push(&stereo_frames((have - pushed) as usize, 5));
                pushed = have;
            }
            handed += (a.take_until(t).len() / 2) as u64;
        }
        (handed, a)
    }

    #[test]
    fn a_sound_card_that_runs_fast_is_kept_within_a_few_milliseconds_of_the_clock() {
        // 150 parts per million fast is 0.54 seconds an hour. Left alone the sound would play later and later
        // than its picture; it is cut back in small steps instead, long before it reaches 40 ms.
        for packet_ms in [10, 20] {
            let (handed, a) = run_card(150.0, 3600, 6, packet_ms);
            let drift_ms = 3600.0 * 150.0 / 1000.0; // 540
            assert!(
                (a.trimmed_ms() as f64 - drift_ms).abs() < 40.0,
                "packets of {packet_ms} ms: trimmed {} ms, the card gained about {drift_ms}",
                a.trimmed_ms()
            );
            assert!(a.worst_ahead_ms() <= 40, "packets of {packet_ms} ms: worst {} ms", a.worst_ahead_ms());
            assert_eq!(a.looks_over_later(), 0, "never over the 40 ms line");
            assert_eq!(a.padded_ms(), 0);
            assert_eq!(handed, 3600 * 48_000, "the track is exactly as long as the recording");
        }
    }

    #[test]
    fn the_cuts_are_small_and_rare() {
        let (_, a) = run_card(100.0, 600, 6, 10);
        // 100 ppm over ten minutes is 60 ms: a few small cuts, not a steady shaving.
        assert!((40..=75).contains(&a.trimmed_ms()), "{}", a.trimmed_ms());
    }

    #[test]
    fn a_card_on_time_with_bursty_packets_loses_nothing() {
        for packet_ms in [10, 20, 30] {
            let (_, a) = run_card(0.0, 600, 6, packet_ms);
            assert_eq!(a.trimmed_ms(), 0, "packets of {packet_ms} ms: nothing to cut");
            assert_eq!(a.padded_ms(), 0);
            assert!(a.worst_ahead_ms() <= 30, "{}", a.worst_ahead_ms());
        }
    }

    #[test]
    fn a_card_that_runs_slow_is_filled_with_silence_not_left_behind() {
        let (handed, a) = run_card(-100.0, 600, 6, 10);
        // Up to 40 ms missing is waited for rather than made up; the end of a recording tops it up.
        assert!(handed + 40 * 48 >= 600 * 48_000 && handed <= 600 * 48_000, "{handed}");
        assert_eq!(a.trimmed_ms(), 0);
        assert!(a.padded_ms() > 0, "the missing sound is made up with silence");
    }

    #[test]
    fn a_cut_leaves_no_click_at_the_join() {
        // A smooth low note. Cutting out part of it leaves the sound after the cut at a different point of the wave
        // than the sound before it ended; a step there would be heard as a click.
        let note = |n: u64| (10_000.0 * (2.0 * std::f64::consts::PI * 130.0 * n as f64 / R as f64).sin()) as i16;
        let mut a = AudioAligner::new(R, 1);
        let (mut made, mut t) = (0u64, 0u64);
        let mut push = |a: &mut AudioAligner, frames: u64| {
            let samples: Vec<i16> = (made..made + frames).map(note).collect();
            made += frames;
            a.push(&samples);
        };
        push(&mut a, 4800); // 100 ms waiting beyond the clock from the start, and it stays: the card is fast
        let mut all: Vec<i16> = Vec::new();
        while t < 8_000 {
            t += 10;
            push(&mut a, 480);
            let got = a.take_until(t);
            all.extend(got);
        }
        assert!(a.trimmed_ms() > 50, "the waiting sound was cut: {}", a.trimmed_ms());
        let steps = |v: &[i16]| v.windows(2).map(|w| (w[1] as i32 - w[0] as i32).abs()).max().unwrap();
        let note_alone: Vec<i16> = (0..all.len() as u64).map(note).collect();
        let (normal, with_cut) = (steps(&note_alone), steps(&all));
        // Without smoothing the step at the cut would be up to the whole height of the wave.
        assert!(with_cut <= normal + 100, "the biggest step is {with_cut}, the note's own is {normal}");
    }

    #[test]
    fn one_look_with_a_great_deal_waiting_cuts_nothing_however_big() {
        // The stale-clock case, in the middle of a stretch and exactly at the end of one.
        for stale_at in [1_010u64, 2_010, 3_990] {
            let mut a = AudioAligner::new(R, 2);
            let mut t = 0;
            while t < 6_000 {
                t += 10;
                a.push(&stereo_frames(480, 3));
                if t == stale_at {
                    a.push(&stereo_frames(R as usize * 2, 3)); // two seconds arrive that the clock has not caught up with
                    a.take_until(t);
                    // the next look has a fresh clock, two seconds on
                    t += 2_000;
                }
                a.take_until(t);
            }
            assert_eq!(a.trimmed_ms(), 0, "stale clock at {stale_at} ms");
        }
    }

    #[test]
    fn extra_sound_that_is_there_for_one_stretch_only_is_not_cut() {
        // 100 ms of sound is waiting for the first two seconds, then the card stops for 100 ms and the clock
        // catches up with it. Cutting at the end of the first stretch would have left a hole afterwards.
        let mut a = AudioAligner::new(R, 2);
        a.push(&stereo_frames(4800, 4));
        let mut t = 0;
        while t < 6_000 {
            t += 10;
            if !(2_200..2_300).contains(&t) {
                a.push(&stereo_frames(480, 4));
            }
            a.take_until(t);
        }
        assert_eq!(a.trimmed_ms(), 0, "nothing was cut");
        assert_eq!(a.padded_ms(), 0, "so nothing was missing afterwards");
    }

    #[test]
    fn a_pause_forgets_what_was_measured_before_it() {
        let (_, mut a) = run_card(150.0, 3, 6, 10);
        let before = a.trimmed_ms();
        a.discard_pending();
        // Right after the pause the first stretch begins again; nothing is cut on old measurements.
        a.push(&stereo_frames(480, 1));
        a.take_until(3_010);
        assert_eq!(a.trimmed_ms(), before);
    }

    #[test]
    fn a_hold_up_that_leaves_the_clock_behind_loses_no_sound() {
        // The program was held up for two seconds in the middle of a look (the disk, say). Sound went on arriving, but
        // the recording time it was given was from before the hold-up. A real recording lost two seconds of the
        // computer's sound by cutting that sound as if the sound card were running fast.
        let mut a = AudioAligner::new(R, 2);
        for ms in (10..=1000).step_by(10) {
            a.push(&stereo_frames(480, 3));
            a.take_until(ms);
        }
        a.push(&stereo_frames(R as usize * 2, 7));
        let held = a.take_until(1010); // the stale clock
        let after = a.take_until(3010); // a fresh one
        assert_eq!(a.trimmed_ms(), 0, "nothing was cut");
        assert_eq!(a.padded_ms(), 0, "nothing was made up");
        let sevens = held.iter().chain(after.iter()).filter(|s| **s == 7).count();
        assert_eq!(sevens, R as usize * 2 * 2, "all two seconds of the new sound are there, both channels");
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
        devices: Vec<AudioDevice>,
        /// How loud each sound output is, by its number; outputs not listed use `system_level`.
        output_levels: Vec<(u32, i16)>,
        /// Sound outputs that cannot be listened to, by number.
        failing_outputs: Vec<u32>,
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

    fn device(kind: DeviceKind, number: u32, name: &str, is_default: bool) -> AudioDevice {
        AudioDevice { kind, number, name: name.into(), is_default, id: format!("{kind:?}-{number}") }
    }

    fn two_of_each() -> Vec<AudioDevice> {
        vec![
            device(DeviceKind::Microphone, 1, "Microphone (Webcam Audio)", true),
            device(DeviceKind::Microphone, 2, "Headset Microphone (USB)", false),
            device(DeviceKind::SoundOutput, 1, "Speakers (Realtek)", true),
            device(DeviceKind::SoundOutput, 2, "Headphones (USB)", false),
        ]
    }

    impl AudioPlatform for FakeAudio {
        fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
            Ok(self.devices.clone())
        }
        fn start_microphone(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            if self.mic_fails {
                return Err(ScreenError::Failed("no microphone is connected".into()));
            }
            let name = device.map(|d| d.name.as_str()).unwrap_or("The Windows default microphone");
            Ok(fake(self.mic_level, true, self.lost.get(), name))
        }
        fn start_system_audio(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            if self.system_fails {
                return Err(ScreenError::Failed("no sound output".into()));
            }
            let name = device.map(|d| d.name.as_str()).unwrap_or("The Windows default output");
            let number = device.map(|d| d.number).unwrap_or(0);
            if self.failing_outputs.contains(&number) {
                return Err(ScreenError::Failed("it would not open".into()));
            }
            let level =
                self.output_levels.iter().find(|(n, _)| *n == number).map(|(_, l)| *l).unwrap_or(self.system_level);
            Ok(fake(level, false, 0, name))
        }
    }

    fn platform() -> FakeAudio {
        FakeAudio {
            mic_level: 8000,
            system_level: 4000,
            mic_fails: false,
            system_fails: false,
            lost: Rc::new(Cell::new(0)),
            devices: two_of_each(),
            output_levels: Vec::new(),
            failing_outputs: Vec::new(),
        }
    }

    fn run(p: &FakeAudio) -> (bool, Vec<Check>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions {
            seconds: 1.0,
            out_dir: dir.path().join("a"),
            microphone: AudioChoice::Default,
            output: AudioChoice::Default,
        };
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
            [
                "audio-devices",
                "microphone-start",
                "system-audio-start",
                "microphone",
                "system-audio",
                "audio-clock",
                "echo",
                "balance",
                "audio-saved"
            ]
        );
        let balance = find(&checks, "balance");
        assert!(balance.ok && balance.message.contains("6 decibels louder"), "{}", balance.message);
        let echo = find(&checks, "echo");
        assert!(
            echo.ok && echo.message.contains("Could not tell whether the microphone is hearing"),
            "{}",
            echo.message
        );
        assert!(find(&checks, "microphone-start")
            .message
            .contains("Microphone (Webcam Audio) (48000 samples per second, 2 channels)"));
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
        assert_eq!(checks.len(), 3, "the device list, then the two that could not start");
    }

    #[test]
    fn lost_sound_reported_by_windows_is_mentioned() {
        let p = platform();
        p.lost.set(3);
        let (_ok, checks, _dir) = run(&p);
        assert!(find(&checks, "microphone").message.contains("3 moments where some sound was lost"));
    }

    #[test]
    fn a_single_lost_moment_is_said_in_the_singular_and_called_normal() {
        let p = platform();
        p.lost.set(1);
        let (_ok, checks, _dir) = run(&p);
        let m = &find(&checks, "microphone").message;
        assert!(m.contains("1 moment where some sound was lost") && !m.contains("1 moments"), "{m}");
        assert!(m.contains("often happens once as listening starts"), "{m}");
    }

    // ---- choosing devices ----

    #[test]
    fn what_a_person_types_becomes_a_choice() {
        assert_eq!(AudioChoice::parse(""), AudioChoice::Default);
        assert_eq!(AudioChoice::parse("  Default "), AudioChoice::Default);
        assert_eq!(AudioChoice::parse("2"), AudioChoice::Number(2));
        assert_eq!(AudioChoice::parse("headset"), AudioChoice::NameContains("headset".into()));
    }

    #[test]
    fn the_right_device_is_found_by_default_number_or_part_of_its_name() {
        let d = two_of_each();
        let name = |kind, choice| choose_device(&d, kind, &choice).map(|x| x.name.clone());
        assert_eq!(name(DeviceKind::Microphone, AudioChoice::Default).unwrap(), "Microphone (Webcam Audio)");
        assert_eq!(name(DeviceKind::SoundOutput, AudioChoice::Number(2)).unwrap(), "Headphones (USB)");
        assert_eq!(
            name(DeviceKind::Microphone, AudioChoice::NameContains("HEADSET".into())).unwrap(),
            "Headset Microphone (USB)"
        );
    }

    #[test]
    fn a_choice_that_cannot_be_met_is_explained_in_words() {
        let d = two_of_each();
        let why = |kind, choice| choose_device(&d, kind, &choice).unwrap_err();
        assert!(why(DeviceKind::Microphone, AudioChoice::Number(5))
            .contains("There is no microphone number 5. Choose a number from 1 to 2"));
        assert!(why(DeviceKind::SoundOutput, AudioChoice::NameContains("hdmi".into()))
            .contains("No sound output has \"hdmi\" in its name"));
        let many = why(DeviceKind::SoundOutput, AudioChoice::NameContains("(".into()));
        assert!(
            many.contains("More than one sound output") && many.contains("1, Speakers (Realtek); 2, Headphones (USB)"),
            "{many}"
        );
        assert!(choose_device(&[], DeviceKind::Microphone, &AudioChoice::Default)
            .unwrap_err()
            .contains("No microphone was found"));
    }

    #[test]
    fn the_list_of_devices_reads_as_sentences_with_the_default_marked() {
        assert_eq!(
            describe_devices(&two_of_each()),
            "Microphones: 1, Microphone (Webcam Audio), the default; 2, Headset Microphone (USB). Sound outputs: 1, Speakers (Realtek), the default; 2, Headphones (USB)."
        );
        assert_eq!(describe_devices(&[]), "Microphones: none found. Sound outputs: none found.");
    }

    #[test]
    fn the_test_uses_the_devices_chosen_and_says_which() {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions {
            seconds: 1.0,
            out_dir: dir.path().join("a"),
            microphone: AudioChoice::Number(2),
            output: AudioChoice::NameContains("headphones".into()),
        };
        let mut checks = Vec::new();
        let ok = run_audio_test_with(&platform(), &opts, &mut |c| checks.push(c));
        assert!(ok, "{checks:?}");
        assert!(find(&checks, "microphone-start").message.contains("Headset Microphone (USB)"));
        assert!(find(&checks, "system-audio-start").message.contains("Headphones (USB)"));
        assert!(find(&checks, "audio-devices")
            .message
            .contains("Microphones: 1, Microphone (Webcam Audio), the default"));
    }

    #[test]
    fn a_choice_that_does_not_exist_stops_only_that_stream_with_a_clear_message() {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions {
            seconds: 1.0,
            out_dir: dir.path().join("a"),
            microphone: AudioChoice::Number(9),
            output: AudioChoice::Default,
        };
        let mut checks = Vec::new();
        let ok = run_audio_test_with(&platform(), &opts, &mut |c| checks.push(c));
        assert!(!ok);
        let c = find(&checks, "microphone-start");
        assert!(!c.ok && c.message.contains("There is no microphone number 9"));
        assert!(find(&checks, "system-audio").ok, "the other stream was still tested");
    }

    #[test]
    fn a_silent_output_names_the_other_outputs_so_the_right_one_can_be_chosen() {
        let mut p = platform();
        p.system_level = 0;
        let (ok, checks, _dir) = run(&p);
        assert!(!ok);
        let c = find(&checks, "system-audio");
        assert!(c.message.contains("Heard nothing from the system sound"));
        assert!(c.message.contains("The other outputs are: 2, Headphones (USB)"), "{}", c.message);
        assert!(c.message.contains("run the test again with that one chosen"));
    }

    // ---- finding where the sound is playing ----

    fn search(p: &FakeAudio) -> (bool, Vec<Check>) {
        let mut checks = Vec::new();
        let ok = run_find_output_with(p, &FindOutputOptions { seconds: 1.0 }, &mut |c| checks.push(c));
        (ok, checks)
    }

    #[test]
    fn the_output_that_carries_sound_is_named_with_its_number() {
        let mut p = platform();
        p.system_level = 0;
        p.output_levels = vec![(2, 6000)];
        let (ok, checks) = search(&p);
        assert!(ok, "{checks:?}");
        let c = find(&checks, "sound-found");
        assert!(
            c.message.contains("2, Headphones (USB)") && c.message.contains("choose sound output number 2"),
            "{}",
            c.message
        );
        assert!(!c.message.contains("Speakers (Realtek)"), "the silent output is not named as carrying sound");
        assert!(find(&checks, "outputs-tried").message.contains("Listened to 2 of the 2 sound outputs"));
    }

    #[test]
    fn if_no_output_carries_sound_the_advice_is_to_start_something_playing() {
        let mut p = platform();
        p.system_level = 0;
        let (ok, checks) = search(&p);
        assert!(!ok);
        let c = find(&checks, "sound-found");
        assert!(
            !c.ok
                && c.message.contains("No sound was heard on any of the 2 outputs")
                && c.message.contains("Start something playing")
        );
    }

    #[test]
    fn several_outputs_carrying_sound_are_all_named_with_a_note_about_virtual_devices() {
        let mut p = platform();
        p.system_level = 5000;
        let (ok, checks) = search(&p);
        assert!(ok);
        let c = find(&checks, "sound-found");
        assert!(c.message.contains("1, Speakers (Realtek)") && c.message.contains("2, Headphones (USB)"));
        assert!(c.message.contains("More than one output carries sound"));
    }

    #[test]
    fn an_output_that_cannot_be_listened_to_is_named_and_the_others_are_still_tried() {
        let mut p = platform();
        p.system_level = 0;
        p.output_levels = vec![(1, 7000)];
        p.failing_outputs = vec![2];
        let (ok, checks) = search(&p);
        assert!(ok, "{checks:?}");
        let tried = find(&checks, "outputs-tried");
        assert!(
            tried.ok
                && tried.message.contains("Listened to 1 of the 2")
                && tried.message.contains("2, Headphones (USB) (it would not open)")
        );
        assert!(find(&checks, "sound-found").message.contains("choose sound output number 1"));
    }

    #[test]
    fn if_no_output_can_be_listened_to_at_all_that_is_the_finding() {
        let mut p = platform();
        p.failing_outputs = vec![1, 2];
        let (ok, checks) = search(&p);
        assert!(!ok);
        assert!(find(&checks, "outputs-tried").message.contains("None of them could be listened to"));
    }

    #[cfg(not(windows))]
    #[test]
    fn on_other_systems_the_sound_test_says_it_needs_windows() {
        let dir = tempfile::tempdir().unwrap();
        let opts = AudioTestOptions {
            seconds: 1.0,
            out_dir: dir.path().join("a"),
            microphone: AudioChoice::Default,
            output: AudioChoice::Default,
        };
        let mut checks = Vec::new();
        assert!(!run_audio_test(&opts, &mut |c| checks.push(c)));
        assert!(checks[0].message.contains("only works on Windows"));
    }
}
