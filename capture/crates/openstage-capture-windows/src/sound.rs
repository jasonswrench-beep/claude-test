// SPDX-License-Identifier: Apache-2.0
//! Recording sound together with the screen.
//!
//! Each take can hold two sound sources: the microphone and the sound the computer itself is playing
//! (including a screen reader's speech). Both are laid onto the recording clock by [`AudioAligner`], so
//! they are exactly as long as the picture, and are then stored twice:
//!
//! * as separate WAV files, `mic.wav` and `system.wav`, which the editor needs (for example to remove
//!   an echo when the microphone also heard the speakers); and
//! * mixed into one AAC sound track inside `screen.mp4`, which is what every player plays.
//!
//! The recording clock stands still while paused, so sound heard during a pause is thrown away. Sound
//! captured while the recorder was starting up is thrown away too: recording time starts only when
//! everything is ready.

use crate::audio::{choose_device, mix, peak, AudioAligner, AudioChoice, AudioDevice, AudioPlatform, AudioSource};
use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::report::TrackReport;
use crate::video::VideoSink;
use crate::{check, Check};
use openstage_capture::{describe_level, SessionError};
use openstage_capture_project::{wav_info, MediaFile, MixInfo, WavWriter};
use std::cell::Cell;
use std::path::Path;

/// The microphone's separate sound file in each take.
pub const MIC_FILE: &str = "mic.wav";
/// The computer's own sound, as a separate file in each take.
pub const SYSTEM_FILE: &str = "system.wav";
/// How good the sound inside the video is, in kilobits per second.
pub const MIXED_BITRATE_KBPS: u32 = 128;

/// The quietest and loudest adjustment allowed when mixing, in decibels.
pub const MIN_GAIN_DB: i32 = -40;
pub const MAX_GAIN_DB: i32 = 20;

/// Names used in `level` events.
const MIC_LEVEL: &str = "mic";
const SYSTEM_LEVEL: &str = "system";

fn media_error(what: impl std::fmt::Display) -> SessionError {
    SessionError::Media(what.to_string())
}

/// Which sound is to be recorded, with the devices already chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundPlan {
    /// The microphone to record, or none.
    pub microphone: Option<AudioDevice>,
    /// The sound output whose sound is to be recorded, or none.
    pub system: Option<AudioDevice>,
    /// Louder (positive) or quieter (negative) by this many decibels in the mix inside the video. The
    /// separate sound files are never changed, so the original balance can always be recovered.
    pub microphone_gain_db: i32,
    pub system_gain_db: i32,
}

impl SoundPlan {
    pub fn is_empty(&self) -> bool {
        self.microphone.is_none() && self.system.is_none()
    }

    /// Set how loud each sound is in the mix inside the video, in whole decibels (negative is quieter,
    /// 0 leaves it as heard). Every 6 decibels is about half or double the volume.
    pub fn with_gains(mut self, microphone_db: i32, system_db: i32) -> Result<Self, String> {
        for (what, db) in [("microphone", microphone_db), ("computer's sound", system_db)] {
            if !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(&db) {
                return Err(format!(
                    "The {what} cannot be changed by {db} decibels in the mix. Use a number from {MIN_GAIN_DB} to {MAX_GAIN_DB}; 0 leaves it as heard."
                ));
            }
        }
        self.microphone_gain_db = microphone_db;
        self.system_gain_db = system_db;
        Ok(self)
    }

    /// How the mix is made, as written into `take.json`.
    pub fn mix_info(&self) -> MixInfo {
        MixInfo { microphone_gain_db: self.microphone_gain_db, system_gain_db: self.system_gain_db }
    }

    /// The choice in words, so the author can see before recording what will be heard.
    pub fn describe(&self) -> String {
        let mic = match &self.microphone {
            Some(d) => format!("Recording your voice from microphone {}, {}.", d.number, d.name),
            None => "Not recording a microphone.".to_string(),
        };
        let system = match &self.system {
            Some(d) => format!("Recording the computer's sound from sound output {}, {}.", d.number, d.name),
            None => "Not recording the computer's sound.".to_string(),
        };
        let mut text = format!("{mic} {system}");
        let adjusted: Vec<String> =
            [("your voice", self.microphone_gain_db), ("the computer's sound", self.system_gain_db)]
                .into_iter()
                .filter(|(_, db)| *db != 0)
                .map(|(what, db)| format!("{what} {} decibels {}", db.abs(), if db < 0 { "quieter" } else { "louder" }))
                .collect();
        if !adjusted.is_empty() {
            text.push_str(&format!(
                " In the mix inside the video, {}. The separate files mic.wav and system.wav are not changed.",
                adjusted.join(" and ")
            ));
        }
        text
    }

    /// The sound files each take will hold.
    pub fn media_files(&self) -> Vec<MediaFile> {
        let mut files = Vec::new();
        if self.microphone.is_some() {
            files.push(MediaFile {
                file: MIC_FILE.into(),
                kind: "microphone".into(),
                container: "wav".into(),
                mix: None,
            });
        }
        if self.system.is_some() {
            files.push(MediaFile {
                file: SYSTEM_FILE.into(),
                kind: "system-audio".into(),
                container: "wav".into(),
                mix: None,
            });
        }
        files
    }
}

/// Turn what a person asked for ("the default", "number 3", "Shure", or nothing) into real devices,
/// or say in words why that cannot be done. `None` means do not record that sound.
pub fn plan_sound(
    platform: &dyn AudioPlatform,
    microphone: Option<&AudioChoice>,
    output: Option<&AudioChoice>,
) -> Result<SoundPlan, String> {
    if microphone.is_none() && output.is_none() {
        return Ok(SoundPlan::default());
    }
    let devices = platform.list_devices().map_err(|e| format!("The sound devices could not be listed: {e}"))?;
    let mut plan = SoundPlan::default();
    if let Some(choice) = microphone {
        plan.microphone = Some(choose_device(&devices, crate::audio::DeviceKind::Microphone, choice)?.clone());
    }
    if let Some(choice) = output {
        plan.system = Some(choose_device(&devices, crate::audio::DeviceKind::SoundOutput, choice)?.clone());
    }
    Ok(plan)
}

/// How the sound for each take is started.
pub struct Sound {
    platform: Box<dyn AudioPlatform>,
    plan: SoundPlan,
}

/// Sources that are listening, before the take's files exist.
pub(crate) struct OpenSound {
    tracks: Vec<(Kind, Box<dyn AudioSource>)>,
    /// How loud each sound is in the mix, in decibels: microphone, then computer's sound.
    gains_db: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Microphone,
    System,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Microphone => "microphone",
            Kind::System => "computer's sound",
        }
    }
    fn level_name(self) -> &'static str {
        match self {
            Kind::Microphone => MIC_LEVEL,
            Kind::System => SYSTEM_LEVEL,
        }
    }
    fn file(self) -> &'static str {
        match self {
            Kind::Microphone => MIC_FILE,
            Kind::System => SYSTEM_FILE,
        }
    }
}

impl Sound {
    pub fn new(platform: Box<dyn AudioPlatform>, plan: SoundPlan) -> Self {
        Self { platform, plan }
    }

    pub fn plan(&self) -> &SoundPlan {
        &self.plan
    }

    /// Start listening. Nothing is written yet.
    pub(crate) fn open(&self) -> Result<OpenSound, SessionError> {
        let mut tracks = Vec::new();
        if let Some(device) = &self.plan.microphone {
            let source = self
                .platform
                .start_microphone(Some(device))
                .map_err(|e| media_error(format!("Could not listen to the microphone {}: {e}", device.name)))?;
            tracks.push((Kind::Microphone, source));
        }
        if let Some(device) = &self.plan.system {
            let source = self.platform.start_system_audio(Some(device)).map_err(|e| {
                media_error(format!("Could not listen to the computer's sound from {}: {e}", device.name))
            })?;
            tracks.push((Kind::System, source));
        }
        for (kind, source) in &tracks {
            let info = source.info();
            if info.sample_rate != SAMPLE_RATE || info.channels != CHANNELS {
                return Err(media_error(format!(
                    "The {} delivers {} channels at {} samples per second, but {} channels at {} were asked for.",
                    kind.label(),
                    info.channels,
                    info.sample_rate,
                    CHANNELS,
                    SAMPLE_RATE
                )));
            }
        }
        Ok(OpenSound { tracks, gains_db: (self.plan.microphone_gain_db, self.plan.system_gain_db) })
    }
}

impl OpenSound {
    /// Create the sound files in the take's folder.
    pub(crate) fn begin(self, take_dir: &Path) -> Result<SoundTake, SessionError> {
        let mut tracks = Vec::new();
        for (kind, source) in self.tracks {
            let path = take_dir.join(kind.file());
            let wav = WavWriter::create(&path, SAMPLE_RATE, CHANNELS)
                .map_err(|e| media_error(format!("Could not create the sound file {}: {e}", path.display())))?;
            let db = if kind == Kind::Microphone { self.gains_db.0 } else { self.gains_db.1 };
            tracks.push(Track {
                kind,
                source,
                gain: 10f32.powf(db as f32 / 20.0),
                aligner: AudioAligner::new(SAMPLE_RATE, CHANNELS),
                wav,
                ready: Vec::new(),
                recent_peak: Cell::new(0.0),
                captured_frames: 0,
            });
        }
        Ok(SoundTake { tracks, mixed_frames: 0 })
    }
}

struct Track {
    kind: Kind,
    source: Box<dyn AudioSource>,
    /// How much louder or quieter this sound is in the mix (1.0 leaves it as heard).
    gain: f32,
    aligner: AudioAligner,
    wav: WavWriter,
    /// Sound laid on the recording clock but not yet mixed into the video's sound track.
    ready: Vec<i16>,
    /// The loudest moment since the level was last asked for.
    recent_peak: Cell<f32>,
    /// How many frames of sound the device really delivered (before any silence was filled in).
    captured_frames: u64,
}

/// One sample made louder or quieter, never wrapped around into noise.
fn scaled(sample: i16, gain: f32) -> i16 {
    (sample as f32 * gain).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

/// The sound of one take, while it is being recorded.
pub(crate) struct SoundTake {
    tracks: Vec<Track>,
    /// How much mixed sound has gone into the video, in frames.
    mixed_frames: u64,
}

impl SoundTake {
    /// Move the sound captured so far, up to `media_ms` of recording time, into the files and the video.
    pub(crate) fn pump(&mut self, media_ms: u64, sink: &mut dyn VideoSink) -> Result<(), SessionError> {
        for t in &mut self.tracks {
            if let Some(why) = t.source.failure() {
                return Err(media_error(format!(
                    "Listening to the {} stopped: {why} Everything recorded until then has been kept.",
                    t.kind.label()
                )));
            }
            let got = t.source.read();
            t.captured_frames += (got.len() / CHANNELS as usize) as u64;
            t.recent_peak.set(t.recent_peak.get().max(peak(&got)));
            t.aligner.push(&got);
            let due = t.aligner.take_until(media_ms);
            t.wav
                .append(&due)
                .map_err(|e| media_error(format!("Writing the {} file stopped with a problem: {e}", t.kind.label())))?;
            t.ready.extend(due);
        }
        self.mix_into(sink)
    }

    /// Mix whatever every track has ready into the video's sound track.
    fn mix_into(&mut self, sink: &mut dyn VideoSink) -> Result<(), SessionError> {
        let channels = CHANNELS as usize;
        let mut n = self.tracks.iter().map(|t| t.ready.len()).min().unwrap_or(0);
        n -= n % channels;
        if n == 0 {
            return Ok(());
        }
        // The separate files hold the sound exactly as heard; only the mix inside the video is adjusted.
        let adjusted = |t: &mut Track| -> Vec<i16> {
            let gain = t.gain;
            t.ready.drain(..n).map(|v| if gain == 1.0 { v } else { scaled(v, gain) }).collect()
        };
        let mut mixed: Vec<i16> = adjusted(&mut self.tracks[0]);
        for t in &mut self.tracks[1..] {
            let part = adjusted(t);
            mixed = mix(&mixed, &part);
        }
        let pts = (self.mixed_frames * 10_000_000 / SAMPLE_RATE as u64) as i64;
        sink.write_audio(&mixed, pts)
            .map_err(|e| media_error(format!("Writing the sound into the video stopped with a problem: {e}")))?;
        self.mixed_frames += (n / channels) as u64;
        Ok(())
    }

    /// Push everything written so far to the disk, so a crash loses only what came after.
    pub(crate) fn sync(&mut self) -> Result<(), SessionError> {
        for t in &mut self.tracks {
            t.wav.sync().map_err(|e| media_error(format!("Saving the {} file failed: {e}", t.kind.label())))?;
        }
        Ok(())
    }

    /// Recording is paused: keep listening, but keep nothing of it.
    pub(crate) fn pause(&mut self) {
        self.discard_backlog();
        for t in &mut self.tracks {
            t.source.set_paused(true);
        }
    }

    /// Recording carries on: drop what was heard while paused, then keep what follows.
    pub(crate) fn resume(&mut self) {
        for t in &mut self.tracks {
            t.source.set_paused(false);
        }
        self.discard_backlog();
    }

    /// Throw away what the sources have heard but nothing has used. Used at the end of start-up and
    /// when pausing or resuming: the recording clock was not running, so that sound has no place in the recording.
    pub(crate) fn discard_backlog(&mut self) {
        for t in &mut self.tracks {
            let _ = t.source.read();
            t.aligner.discard_pending();
        }
    }

    /// How each sound behaved, for the summary after a recording.
    pub(crate) fn reports(&self) -> Vec<TrackReport> {
        self.tracks
            .iter()
            .map(|t| TrackReport {
                microphone: t.kind == Kind::Microphone,
                heard_ms: t.captured_frames * 1000 / SAMPLE_RATE as u64,
                padded_ms: t.aligner.padded_ms(),
                trimmed_ms: t.aligner.trimmed_ms(),
                worst_later_ms: t.aligner.worst_ahead_ms(),
                worst_earlier_ms: t.aligner.worst_late_ms(),
                lost_moments: t.source.discontinuities(),
            })
            .collect()
    }

    /// How many separate sound files this take holds.
    pub(crate) fn track_count(&self) -> u32 {
        self.tracks.len() as u32
    }

    /// Current levels by name, each the loudest moment since last asked.
    pub(crate) fn levels(&self) -> Vec<(&'static str, f32)> {
        self.tracks.iter().map(|t| (t.kind.level_name(), t.recent_peak.replace(0.0))).collect()
    }

    /// Write the last sound up to `media_ms` and complete the sound files.
    pub(crate) fn finish(mut self, media_ms: u64, sink: &mut dyn VideoSink) -> Result<Vec<TrackReport>, SessionError> {
        self.pump(media_ms, sink)?;
        let reports = self.reports();
        for t in self.tracks {
            t.wav.finish().map_err(|e| media_error(format!("Completing the {} file failed: {e}", t.kind.label())))?;
        }
        Ok(reports)
    }
}

/// What looking at the saved sound files found.
#[derive(Debug, Default)]
pub struct SoundFileReport {
    /// One finding per file: is it there, readable, and as long as the recording?
    pub checks: Vec<Check>,
    /// Things worth the author's attention that are not failures, such as a track that is silent.
    pub warnings: Vec<(String, String)>,
}

/// Look at the saved sound files of a take and say what is in them, in plain words. The sound is read
/// from the files themselves, so this reports what was really saved.
pub fn check_sound_files(take_dir: &Path, plan: &SoundPlan, recorded_ms: u64) -> SoundFileReport {
    let mut report = SoundFileReport::default();
    let tracks = [
        (Kind::Microphone, "microphone-file", plan.microphone.as_ref(), "microphone", "microphone"),
        (Kind::System, "system-sound-file", plan.system.as_ref(), "computer's sound", "sound output"),
    ];
    for (kind, name, device, label, device_label) in tracks {
        let Some(device) = device else { continue };
        let path = take_dir.join(kind.file());
        match wav_info(&path) {
            Ok(info) if info.duration_ms().abs_diff(recorded_ms) <= 300 => {
                report.checks.push(check(
                    name,
                    true,
                    format!(
                        "The {label} file {} holds {:.1} seconds of sound; the loudest moment was {} (peak {:.2}). The recording time was {:.1} seconds.",
                        kind.file(),
                        info.duration_ms() as f64 / 1000.0,
                        describe_level(info.peak),
                        info.peak,
                        recorded_ms as f64 / 1000.0
                    ),
                ));
                if info.peak < 0.01 {
                    let advice = match kind {
                        Kind::Microphone => "Check that the microphone is not muted, and that Windows lets desktop programs use it: Settings, Privacy and security, Microphone, Let desktop apps access your microphone.".to_string(),
                        Kind::System => format!(
                            "If sound was playing, it was probably going to a different sound output than {device_label} {}, {}. FIND SOUND OUTPUT.bat shows which one carries the sound.",
                            device.number, device.name
                        ),
                    };
                    report.warnings.push((
                        "track-silent".to_string(),
                        format!(
                            "The {label} track is silent: nothing was heard from {} during this recording. {advice}",
                            device.name
                        ),
                    ));
                }
            }
            Ok(info) => report.checks.push(check(
                name,
                false,
                format!(
                    "The {label} file {} holds {:.1} seconds of sound, but the recording time was {:.1} seconds.",
                    kind.file(),
                    info.duration_ms() as f64 / 1000.0,
                    recorded_ms as f64 / 1000.0
                ),
            )),
            Err(e) => report.checks.push(check(
                name,
                false,
                format!("The {label} file {} could not be read back: {e}", kind.file()),
            )),
        }
    }
    report
}

/// Sound that follows a manual clock, and a computer to find it on, for the tests of everything that records sound.
#[cfg(test)]
pub(crate) mod fakes {
    use super::*;
    use crate::audio::DeviceKind;
    use crate::ScreenError;
    use openstage_capture::{Clock, ManualClock};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Sound that follows the manual clock, like a real microphone does: `rate` samples for every
    /// millisecond that passed, whether or not anyone is recording.
    pub(crate) struct ClockSound {
        pub(crate) clock: Rc<ManualClock>,
        pub(crate) info: crate::audio::AudioInfo,
        pub(crate) last_ms: u64,
        /// The sample value heard at this moment.
        pub(crate) value: Box<dyn Fn(u64) -> i16>,
        pub(crate) backlog: Vec<i16>,
        pub(crate) broken: Rc<Cell<Option<&'static str>>>,
        /// Every time the recorder told this source to pause (true) or carry on (false).
        pub(crate) pauses: Rc<RefCell<Vec<bool>>>,
    }

    impl ClockSound {
        pub(crate) fn new(clock: &Rc<ManualClock>, value: impl Fn(u64) -> i16 + 'static) -> Self {
            Self {
                clock: Rc::clone(clock),
                info: crate::audio::AudioInfo { device: "fake".into(), sample_rate: SAMPLE_RATE, channels: CHANNELS },
                last_ms: clock.now_ms(),
                value: Box::new(value),
                backlog: Vec::new(),
                broken: Rc::new(Cell::new(None)),
                pauses: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl AudioSource for ClockSound {
        fn info(&self) -> &crate::audio::AudioInfo {
            &self.info
        }
        fn read(&mut self) -> Vec<i16> {
            let now = self.clock.now_ms();
            let mut out = std::mem::take(&mut self.backlog);
            for ms in self.last_ms..now {
                let v = (self.value)(ms);
                out.extend(std::iter::repeat_n(v, 48 * CHANNELS as usize));
            }
            self.last_ms = now;
            out
        }
        fn discontinuities(&self) -> u64 {
            0
        }
        fn failure(&self) -> Option<String> {
            self.broken.get().map(|s| s.to_string())
        }
        fn set_paused(&mut self, paused: bool) {
            self.pauses.borrow_mut().push(paused);
        }
    }

    pub(crate) fn device(kind: DeviceKind, number: u32, name: &str, is_default: bool) -> AudioDevice {
        AudioDevice { kind, number, name: name.into(), is_default, id: format!("{kind:?}{number}") }
    }

    /// A computer with two microphones and two outputs. The sources it starts follow the shared clock.
    pub(crate) struct FakePlatform {
        pub(crate) clock: Rc<ManualClock>,
        pub(crate) mic: Rc<dyn Fn(u64) -> i16>,
        pub(crate) system: Rc<dyn Fn(u64) -> i16>,
        pub(crate) started: RefCell<Vec<String>>,
        pub(crate) mic_backlog: Vec<i16>,
        pub(crate) broken: Rc<Cell<Option<&'static str>>>,
        pub(crate) no_microphone: bool,
        pub(crate) mic_error: Option<&'static str>,
        /// What every source it started was told about pausing, in order.
        pub(crate) pauses: Rc<RefCell<Vec<bool>>>,
    }

    impl FakePlatform {
        pub(crate) fn new(clock: &Rc<ManualClock>) -> Self {
            Self {
                clock: Rc::clone(clock),
                mic: Rc::new(|_| 1000),
                system: Rc::new(|_| 2000),
                started: RefCell::new(Vec::new()),
                mic_backlog: Vec::new(),
                broken: Rc::new(Cell::new(None)),
                no_microphone: false,
                mic_error: None,
                pauses: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl AudioPlatform for FakePlatform {
        fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
            let mut list = Vec::new();
            if !self.no_microphone {
                list.push(device(DeviceKind::Microphone, 1, "Webcam microphone", true));
                list.push(device(DeviceKind::Microphone, 2, "Shure MV7", false));
            }
            list.push(device(DeviceKind::SoundOutput, 1, "Speakers (Realtek)", true));
            list.push(device(DeviceKind::SoundOutput, 2, "Headphones (USB)", false));
            Ok(list)
        }
        fn start_microphone(&self, d: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            self.started.borrow_mut().push(format!("microphone {}", d.map(|d| d.name.as_str()).unwrap_or("default")));
            if let Some(why) = self.mic_error {
                return Err(ScreenError::Failed(why.into()));
            }
            let mic = Rc::clone(&self.mic);
            let mut s = ClockSound::new(&self.clock, move |ms| mic(ms));
            s.backlog = self.mic_backlog.clone();
            s.broken = Rc::clone(&self.broken);
            s.pauses = Rc::clone(&self.pauses);
            Ok(Box::new(s))
        }
        fn start_system_audio(&self, d: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
            self.started.borrow_mut().push(format!("system {}", d.map(|d| d.name.as_str()).unwrap_or("default")));
            let system = Rc::clone(&self.system);
            Ok(Box::new(ClockSound::new(&self.clock, move |ms| system(ms))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::*;
    use super::*;
    use crate::audio::DeviceKind;
    use crate::{Frame, ScreenError};
    use openstage_capture::{Clock, ManualClock};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Pieces of sound the video received: where each starts (100 ns units) and its samples.
    type Pieces = Vec<(i64, Vec<i16>)>;

    #[derive(Default)]
    struct FakeSink(Rc<RefCell<Pieces>>);
    impl VideoSink for FakeSink {
        fn write(&mut self, _: &Frame, _: i64, _: i64) -> Result<(), ScreenError> {
            Ok(())
        }
        fn write_audio(&mut self, samples: &[i16], pts: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().push((pts, samples.to_vec()));
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            Ok(())
        }
    }

    fn choose(mic: Option<&str>, out: Option<&str>) -> (Rc<ManualClock>, FakePlatform, Result<SoundPlan, String>) {
        let clock = Rc::new(ManualClock::new());
        let platform = FakePlatform::new(&clock);
        let plan = plan_sound(&platform, mic.map(AudioChoice::parse).as_ref(), out.map(AudioChoice::parse).as_ref());
        (clock, platform, plan)
    }

    /// Record `seconds` of fake sound at 20 ms steps and return what went into the files and the video.
    fn record(clock: &Rc<ManualClock>, sound: &Sound, dir: &Path, seconds: u64) -> (Pieces, Result<(), SessionError>) {
        let sink_log = Rc::new(RefCell::new(Vec::new()));
        let mut sink = FakeSink(Rc::clone(&sink_log));
        let mut take = sound.open().unwrap().begin(dir).unwrap();
        take.discard_backlog();
        let mut media_ms = 0;
        let mut result = Ok(());
        while media_ms < seconds * 1000 {
            clock.sleep_ms(20);
            media_ms += 20;
            if let Err(e) = take.pump(media_ms, &mut sink) {
                result = Err(e);
                break;
            }
            if media_ms % 1000 == 0 {
                take.sync().unwrap();
            }
        }
        if result.is_ok() {
            result = take.finish(media_ms, &mut sink).map(|_| ());
        }
        let chunks = sink_log.borrow().clone();
        (chunks, result)
    }

    // ---- choosing ----

    #[test]
    fn the_default_microphone_and_output_are_chosen_and_described_in_words() {
        let (_clock, _p, plan) = choose(Some("default"), Some("default"));
        let plan = plan.unwrap();
        assert_eq!(plan.microphone.as_ref().unwrap().name, "Webcam microphone");
        assert_eq!(plan.system.as_ref().unwrap().name, "Speakers (Realtek)");
        let text = plan.describe();
        assert!(
            text.contains("microphone 1, Webcam microphone") && text.contains("sound output 1, Speakers (Realtek)")
        );
    }

    #[test]
    fn a_number_or_part_of_a_name_chooses_a_particular_device() {
        let (_c, _p, plan) = choose(Some("shure"), Some("2"));
        let plan = plan.unwrap();
        assert_eq!(plan.microphone.unwrap().name, "Shure MV7");
        assert_eq!(plan.system.unwrap().name, "Headphones (USB)");
    }

    #[test]
    fn leaving_one_out_records_only_the_other() {
        let (_c, _p, plan) = choose(Some("default"), None);
        let plan = plan.unwrap();
        assert!(plan.system.is_none());
        assert!(plan.describe().contains("Not recording the computer's sound."));
        let files = plan.media_files();
        assert_eq!(files.len(), 1);
        assert_eq!(
            (files[0].file.as_str(), files[0].kind.as_str(), files[0].container.as_str()),
            ("mic.wav", "microphone", "wav")
        );
        let (_c, _p, none) = choose(None, None);
        assert!(none.unwrap().is_empty());
    }

    #[test]
    fn a_device_that_does_not_exist_is_explained_before_anything_starts() {
        let (_c, _p, plan) = choose(Some("7"), None);
        let why = plan.unwrap_err();
        assert!(why.contains("no microphone number 7"), "{why}");
        let (_c, _p, plan) = choose(Some("nonsense"), None);
        assert!(plan.unwrap_err().contains("No microphone has \"nonsense\" in its name"));
    }

    #[test]
    fn a_computer_with_no_microphone_says_so_plainly() {
        let clock = Rc::new(ManualClock::new());
        let mut platform = FakePlatform::new(&clock);
        platform.no_microphone = true;
        let why = plan_sound(&platform, Some(&AudioChoice::Default), None).unwrap_err();
        assert!(why.contains("No microphone was found"), "{why}");
    }

    // ---- recording ----

    #[test]
    fn both_sounds_are_kept_separately_and_mixed_into_the_video() {
        let (clock, platform, plan) = choose(Some("default"), Some("default"));
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();
        let (chunks, result) = record(&clock, &sound, dir.path(), 3);
        result.unwrap();

        let mic = wav_info(&dir.path().join("mic.wav")).unwrap();
        let system = wav_info(&dir.path().join("system.wav")).unwrap();
        for (name, info) in [("mic", &mic), ("system", &system)] {
            assert_eq!((info.sample_rate, info.channels), (48_000, 2), "{name}");
            assert!((142_000..=144_000).contains(&info.frames), "{name}: about 3 seconds, got {} frames", info.frames);
        }
        assert!((mic.peak - 1000.0 / 32768.0).abs() < 0.001, "the microphone file holds only the microphone");
        assert!((system.peak - 2000.0 / 32768.0).abs() < 0.001, "the system file holds only the computer's sound");

        // The video's sound is both together, in one unbroken run starting at time zero.
        assert!(chunks.iter().all(|(_, s)| s.iter().all(|v| *v == 3000)), "1000 + 2000 mixed");
        assert_eq!(chunks[0].0, 0);
        let mut frames = 0u64;
        for (pts, samples) in &chunks {
            assert_eq!(*pts, (frames * 10_000_000 / 48_000) as i64, "each piece starts where the last one ended");
            frames += samples.len() as u64 / 2;
        }
        assert_eq!(frames, mic.frames, "the video holds as much sound as the files");
    }

    #[test]
    fn with_only_the_microphone_the_video_holds_just_the_microphone() {
        let (clock, platform, plan) = choose(Some("default"), None);
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();
        let (chunks, result) = record(&clock, &sound, dir.path(), 1);
        result.unwrap();
        assert!(!dir.path().join("system.wav").exists());
        assert!(!chunks.is_empty() && chunks.iter().all(|(_, s)| s.iter().all(|v| *v == 1000)));
    }

    #[test]
    fn sound_captured_while_the_recorder_was_starting_is_not_recorded() {
        let (clock, mut platform, plan) = choose(Some("default"), Some("default"));
        platform.mic_backlog = vec![5555; 48_000 * 2 * 2]; // two seconds heard during start-up
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();
        let (chunks, result) = record(&clock, &sound, dir.path(), 1);
        result.unwrap();
        let mic = wav_info(&dir.path().join("mic.wav")).unwrap();
        assert!((46_000..=48_000).contains(&mic.frames), "one second, not three: got {} frames", mic.frames);
        assert!((mic.peak - 1000.0 / 32768.0).abs() < 0.001, "none of the start-up sound is in the file");
        assert!(
            chunks.iter().all(|(_, s)| s.iter().all(|v| *v == 3000)),
            "nothing from the start-up backlog reached the video"
        );
    }

    #[test]
    fn sound_heard_during_a_pause_is_left_out() {
        let (clock, mut platform, plan) = choose(Some("default"), None);
        // While paused (between 1.0 and 2.0 seconds of real time) the microphone hears something loud.
        platform.mic = Rc::new(|ms| if (1000..2000).contains(&ms) { 9999 } else { 1000 });
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();

        let sink_log = Rc::new(RefCell::new(Vec::new()));
        let mut sink = FakeSink(Rc::clone(&sink_log));
        let mut take = sound.open().unwrap().begin(dir.path()).unwrap();
        take.discard_backlog();
        let mut media_ms = 0;
        // 1 second recording
        for _ in 0..50 {
            clock.sleep_ms(20);
            media_ms += 20;
            take.pump(media_ms, &mut sink).unwrap();
        }
        // 1 second paused: the clock runs, the recording clock does not, and nothing is pumped
        clock.sleep_ms(1000);
        take.discard_backlog(); // what the recorder does on resume
                                // 1 more second recording
        for _ in 0..50 {
            clock.sleep_ms(20);
            media_ms += 20;
            take.pump(media_ms, &mut sink).unwrap();
        }
        take.finish(media_ms, &mut sink).unwrap();

        let mic = wav_info(&dir.path().join("mic.wav")).unwrap();
        assert!((94_000..=96_000).contains(&mic.frames), "2 of the 3 seconds: got {} frames", mic.frames);
        assert!((mic.peak - 1000.0 / 32768.0).abs() < 0.001, "the loud sound from the pause is not in the file");
    }

    #[test]
    fn a_microphone_that_stops_ends_the_take_with_a_plain_message() {
        let (clock, platform, plan) = choose(Some("default"), Some("default"));
        let broken = Rc::clone(&platform.broken);
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();
        let sink_log = Rc::new(RefCell::new(Vec::new()));
        let mut sink = FakeSink(Rc::clone(&sink_log));
        let mut take = sound.open().unwrap().begin(dir.path()).unwrap();
        clock.sleep_ms(20);
        take.pump(20, &mut sink).unwrap();
        broken.set(Some("The device was unplugged."));
        clock.sleep_ms(20);
        let err = take.pump(40, &mut sink).unwrap_err().to_string();
        assert!(
            err.contains("Listening to the microphone stopped: The device was unplugged.")
                && err.contains("has been kept"),
            "{err}"
        );
    }

    #[test]
    fn levels_report_the_loudest_moment_since_last_asked() {
        let (clock, platform, plan) = choose(Some("default"), Some("default"));
        let sound = Sound::new(Box::new(platform), plan.unwrap());
        let dir = tempfile::tempdir().unwrap();
        let mut sink = FakeSink::default();
        let mut take = sound.open().unwrap().begin(dir.path()).unwrap();
        clock.sleep_ms(100);
        take.pump(100, &mut sink).unwrap();
        let levels = take.levels();
        assert_eq!(levels.iter().map(|l| l.0).collect::<Vec<_>>(), vec!["mic", "system"]);
        assert!((levels[0].1 - 1000.0 / 32768.0).abs() < 0.001 && (levels[1].1 - 2000.0 / 32768.0).abs() < 0.001);
        let again = take.levels();
        assert!(again.iter().all(|l| l.1 == 0.0), "asking resets the meter");
    }

    #[test]
    fn a_source_with_the_wrong_format_is_refused_in_words() {
        struct Odd(Rc<ManualClock>);
        impl AudioPlatform for Odd {
            fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
                Ok(vec![device(DeviceKind::Microphone, 1, "Odd", true)])
            }
            fn start_microphone(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
                let mut s = ClockSound::new(&self.0, |_| 0);
                s.info.sample_rate = 44_100;
                Ok(Box::new(s))
            }
            fn start_system_audio(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
                Err(ScreenError::Failed("unused".into()))
            }
        }
        let clock = Rc::new(ManualClock::new());
        let platform = Odd(Rc::clone(&clock));
        let plan = plan_sound(&platform, Some(&AudioChoice::Default), None).unwrap();
        let e = Sound::new(Box::new(platform), plan).open().err().expect("refused").to_string();
        assert!(e.contains("44100 samples per second"), "{e}");
    }

    #[test]
    fn a_device_that_cannot_be_started_is_named() {
        struct Busy;
        impl AudioPlatform for Busy {
            fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
                Ok(vec![device(DeviceKind::Microphone, 1, "Shure MV7", true)])
            }
            fn start_microphone(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
                Err(ScreenError::Failed("another program is using it".into()))
            }
            fn start_system_audio(&self, _: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
                Err(ScreenError::Failed("unused".into()))
            }
        }
        let plan = plan_sound(&Busy, Some(&AudioChoice::Default), None).unwrap();
        let e = Sound::new(Box::new(Busy), plan).open().err().expect("not started").to_string();
        assert!(e.contains("Could not listen to the microphone Shure MV7: another program is using it"), "{e}");
    }

    // ---- looking at the saved files ----

    fn saved_track(dir: &Path, file: &str, seconds: u64, value: i16) {
        let mut w = WavWriter::create(&dir.join(file), 48_000, 2).unwrap();
        w.append(&vec![value; seconds as usize * 48_000 * 2]).unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn saved_files_that_match_the_recording_pass_and_are_described_in_words() {
        let (_c, _p, plan) = choose(Some("default"), Some("default"));
        let dir = tempfile::tempdir().unwrap();
        saved_track(dir.path(), "mic.wav", 2, 8000);
        saved_track(dir.path(), "system.wav", 2, 600);
        let report = check_sound_files(dir.path(), &plan.unwrap(), 2000);
        assert!(report.checks.iter().all(|c| c.ok), "{:?}", report.checks);
        assert_eq!(report.checks.len(), 2);
        assert!(
            report.checks[0].message.contains("microphone file mic.wav holds 2.0 seconds")
                && report.checks[0].message.contains("good")
        );
        assert!(
            report.checks[1].message.contains("computer's sound file system.wav")
                && report.checks[1].message.contains("quiet")
        );
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn a_silent_track_is_flagged_with_advice_that_fits_what_it_was() {
        let (_c, _p, plan) = choose(Some("default"), Some("2"));
        let dir = tempfile::tempdir().unwrap();
        saved_track(dir.path(), "mic.wav", 1, 0);
        saved_track(dir.path(), "system.wav", 1, 0);
        let report = check_sound_files(dir.path(), &plan.unwrap(), 1000);
        assert!(report.checks.iter().all(|c| c.ok), "silence is a finding to tell, not a broken file");
        assert_eq!(report.warnings.len(), 2);
        let mic = &report.warnings[0].1;
        assert!(
            report.warnings[0].0 == "track-silent" && mic.contains("Webcam microphone") && mic.contains("not muted"),
            "{mic}"
        );
        let system = &report.warnings[1].1;
        assert!(
            system.contains("sound output 2, Headphones (USB)") && system.contains("FIND SOUND OUTPUT.bat"),
            "{system}"
        );
    }

    #[test]
    fn a_file_that_is_too_short_or_missing_is_a_failure() {
        let (_c, _p, plan) = choose(Some("default"), Some("default"));
        let dir = tempfile::tempdir().unwrap();
        saved_track(dir.path(), "mic.wav", 1, 8000); // the recording was 3 seconds
        let report = check_sound_files(dir.path(), &plan.unwrap(), 3000);
        assert_eq!(report.checks.len(), 2);
        assert!(
            !report.checks[0].ok
                && report.checks[0].message.contains("holds 1.0 seconds of sound, but the recording time was 3.0")
        );
        assert!(
            !report.checks[1].ok && report.checks[1].message.contains("could not be read back"),
            "{}",
            report.checks[1].message
        );
    }

    #[test]
    fn tracks_that_were_not_recorded_are_not_looked_for() {
        let (_c, _p, plan) = choose(Some("default"), None);
        let dir = tempfile::tempdir().unwrap();
        saved_track(dir.path(), "mic.wav", 1, 8000);
        let report = check_sound_files(dir.path(), &plan.unwrap(), 1000);
        assert_eq!(report.checks.len(), 1);
    }

    // ---- balancing the mix ----

    #[test]
    fn the_mix_can_make_the_computer_quieter_while_the_separate_files_stay_as_heard() {
        let (clock, platform, plan) = choose(Some("default"), Some("default"));
        let plan = plan.unwrap().with_gains(0, -6).unwrap(); // 6 decibels down is about half
        let sound = Sound::new(Box::new(platform), plan);
        let dir = tempfile::tempdir().unwrap();
        let (chunks, result) = record(&clock, &sound, dir.path(), 1);
        result.unwrap();
        // The microphone (1000) and half of the computer's sound (2000 becomes about 1002).
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|(_, s)| s.iter().all(|v| (2000..=2004).contains(v))), "{:?}", &chunks[0].1[..4]);
        assert!((wav_info(&dir.path().join("system.wav")).unwrap().peak - 2000.0 / 32768.0).abs() < 0.001);
        assert!((wav_info(&dir.path().join("mic.wav")).unwrap().peak - 1000.0 / 32768.0).abs() < 0.001);
    }

    #[test]
    fn a_louder_mix_never_wraps_around_into_noise() {
        let (clock, mut platform, plan) = choose(Some("default"), None);
        platform.mic = Rc::new(|_| 30000);
        let plan = plan.unwrap().with_gains(20, 0).unwrap(); // ten times louder
        let sound = Sound::new(Box::new(platform), plan);
        let dir = tempfile::tempdir().unwrap();
        let (chunks, result) = record(&clock, &sound, dir.path(), 1);
        result.unwrap();
        assert!(chunks.iter().all(|(_, s)| s.iter().all(|v| *v == i16::MAX)), "held at the loudest value");
    }

    #[test]
    fn a_change_beyond_the_allowed_range_is_refused_in_words() {
        let (_c, _p, plan) = choose(Some("default"), Some("default"));
        let plan = plan.unwrap();
        let why = plan.clone().with_gains(0, -80).unwrap_err();
        assert!(
            why.contains("computer's sound cannot be changed by -80 decibels") && why.contains("-40 to 20"),
            "{why}"
        );
        assert!(plan.clone().with_gains(25, 0).unwrap_err().contains("microphone cannot be changed by 25"));
        assert!(plan.with_gains(-40, 20).is_ok(), "the ends of the range are allowed");
    }

    #[test]
    fn the_adjustment_is_said_in_words_and_written_down_for_the_editor() {
        let (_c, _p, plan) = choose(Some("default"), Some("default"));
        let plan = plan.unwrap();
        assert!(!plan.describe().contains("In the mix"), "no sentence about the mix when nothing is changed");
        let plan = plan.with_gains(3, -12).unwrap();
        let text = plan.describe();
        assert!(
            text.contains("your voice 3 decibels louder and the computer's sound 12 decibels quieter")
                && text.contains("mic.wav and system.wav are not changed"),
            "{text}"
        );
        assert_eq!((plan.mix_info().microphone_gain_db, plan.mix_info().system_gain_db), (3, -12));
    }
}
