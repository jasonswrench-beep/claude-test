// SPDX-License-Identifier: Apache-2.0
//! The first-run check of a computer (story A9): can this Windows install record, and if not, what
//! is missing and what to do about it, in plain words.
//!
//! It only looks; it records nothing and asks for no permission. Each finding says what was found
//! and, when something is wrong, what to change. Parts that are optional (a microphone, a sound
//! output) never fail the check: they are reported so the author knows what a recording will lack.

use crate::audio::{AudioPlatform, DeviceKind};
use crate::{check, Check, Platform, VideoPlatform};
use std::path::{Path, PathBuf};

/// Whether Windows lets desktop programs use a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Allowed,
    /// Switched off, with what to change.
    Blocked(String),
    /// Windows did not say.
    Unknown,
}

/// What the check needs to ask the system that the other platform traits do not cover.
pub trait SystemFacts {
    /// Whether desktop programs may use the microphone.
    fn microphone_access(&self) -> Access;
    /// Free space on the drive that holds `dir`, in bytes, if it can be told.
    fn free_disk_bytes(&self, dir: &Path) -> Option<u64>;
}

/// The real thing on Windows; elsewhere one that knows nothing.
pub fn system_facts() -> Box<dyn SystemFacts> {
    #[cfg(windows)]
    {
        Box::new(crate::win_setup::WindowsFacts)
    }
    #[cfg(not(windows))]
    {
        Box::new(NoFacts)
    }
}

#[cfg(not(windows))]
struct NoFacts;

#[cfg(not(windows))]
impl SystemFacts for NoFacts {
    fn microphone_access(&self) -> Access {
        Access::Unknown
    }
    fn free_disk_bytes(&self, _: &Path) -> Option<u64> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct SetupOptions {
    /// Where recordings will be kept; the free space of its drive is reported. Defaults to the current folder.
    pub dir: PathBuf,
}

/// About how much a recording takes with the default settings: 8 megabits a second of picture, and two
/// sound files of 192 kilobytes a second each, per hour.
const GIGABYTES_PER_HOUR: f64 = 5.0;
/// Less free space than this is a problem; a recording could stop part-way.
const MIN_FREE_GB: f64 = 2.0;

fn gigabytes(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

/// Run the check on this computer.
pub fn run_setup_check(opts: &SetupOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_setup_check_with(
        crate::system_platform().as_ref(),
        crate::system_video_platform().as_ref(),
        crate::system_audio_platform().as_ref(),
        system_facts().as_ref(),
        opts,
        report,
    )
}

/// [`run_setup_check`] with everything supplied, so it can be tested anywhere.
pub fn run_setup_check_with(
    screen: &dyn Platform,
    video: &dyn VideoPlatform,
    audio: &dyn AudioPlatform,
    facts: &dyn SystemFacts,
    opts: &SetupOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut all_fine = true;
    let mut say = |c: Check| {
        all_fine &= c.ok;
        report(c);
    };

    // Windows 11 or later.
    match screen.windows_version() {
        Ok(v) if v.is_windows_11_or_later() => {
            say(check("windows-version", true, format!("{}. That is supported.", v.describe())))
        }
        Ok(v) => say(check(
            "windows-version",
            false,
            format!(
                "{}. OpenStage Capture needs Windows 11 or later. Windows 10 and earlier are not supported.",
                v.describe()
            ),
        )),
        Err(e) => say(check("windows-version", false, format!("Windows would not say which version this is: {e}"))),
    }

    // Screen capture.
    if screen.capture_supported() {
        say(check("screen-capture", true, "Windows can capture the screen."));
    } else {
        say(check(
            "screen-capture",
            false,
            "Windows' screen capture is not available on this computer. It needs Windows 11 and a graphics driver that supports it; installing the latest graphics driver often fixes this.",
        ));
    }
    match screen.list_displays() {
        Ok(displays) if !displays.is_empty() => say(check(
            "displays",
            true,
            format!(
                "Found {} display{}: {}.",
                displays.len(),
                if displays.len() == 1 { "" } else { "s" },
                displays.iter().map(|d| d.describe()).collect::<Vec<_>>().join("; ")
            ),
        )),
        Ok(_) => say(check("displays", false, "Windows reported no displays.")),
        Err(e) => say(check("displays", false, format!("The displays could not be listed: {e}"))),
    }

    // The media parts an MP4 needs. Some "N" editions of Windows come without them.
    let media_advice = "Some editions of Windows (the \"N\" editions) come without Windows' media parts. Microsoft's free Media Feature Pack adds them; search for \"Media Feature Pack\" in Windows Settings.";
    match video.start_media().and_then(|_| video.h264_encoders()) {
        Ok(list) if !list.is_empty() => say(check(
            "video-encoder",
            true,
            format!(
                "Windows can make H.264 video, which is what MP4 needs ({}).",
                list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join("; ")
            ),
        )),
        Ok(_) => say(check(
            "video-encoder",
            false,
            format!("Windows has no H.264 video encoder, so no MP4 can be made. {media_advice}"),
        )),
        Err(e) => say(check("video-encoder", false, format!("Windows' video tools would not answer: {e}"))),
    }
    match video.aac_encoder_available() {
        Ok(true) => say(check("sound-encoder", true, "Windows can make AAC sound, which MP4 needs.")),
        Ok(false) => say(check(
            "sound-encoder",
            false,
            format!("Windows has no AAC sound encoder, so no MP4 with sound can be made. {media_advice}"),
        )),
        Err(e) => say(check("sound-encoder", false, format!("Windows' sound tools would not answer: {e}"))),
    }

    // Sound devices are optional: a recording without a voice or without the computer's sound is still a recording.
    match audio.list_devices() {
        Ok(devices) => {
            let mics: Vec<_> = devices.iter().filter(|d| d.kind == DeviceKind::Microphone).collect();
            let outputs: Vec<_> = devices.iter().filter(|d| d.kind == DeviceKind::SoundOutput).collect();
            match mics.iter().find(|d| d.is_default).or(mics.first()) {
                Some(mic) => say(check(
                    "microphone",
                    true,
                    format!("Found {} microphone{}; the default is {}.", mics.len(), if mics.len() == 1 { "" } else { "s" }, mic.name),
                )),
                None => say(check(
                    "microphone",
                    true,
                    "No microphone was found. A recording can still be made, but it will have no voice. Plug one in and run this check again.",
                )),
            }
            match outputs.iter().find(|d| d.is_default).or(outputs.first()) {
                Some(out) => say(check(
                    "sound-output",
                    true,
                    format!("Found {} sound output{}; the default is {}.", outputs.len(), if outputs.len() == 1 { "" } else { "s" }, out.name),
                )),
                None => say(check(
                    "sound-output",
                    true,
                    "No sound output was found, so the computer's own sound cannot be recorded. A recording can still be made.",
                )),
            }
        }
        Err(e) => say(check("microphone", false, format!("The sound devices could not be listed: {e}"))),
    }
    match facts.microphone_access() {
        Access::Allowed => say(check("microphone-access", true, "Windows lets desktop programs use the microphone.")),
        Access::Blocked(how) => say(check(
            "microphone-access",
            false,
            format!("Windows is blocking the microphone. {how} Without it a recording that asks for the microphone will stop at the start."),
        )),
        Access::Unknown => say(check(
            "microphone-access",
            true,
            "Windows did not say whether desktop programs may use the microphone. If recording your voice fails, open Settings, Privacy and security, Microphone, and allow desktop apps to use it.",
        )),
    }

    // Room for the recordings.
    match facts.free_disk_bytes(&opts.dir) {
        Some(bytes) => {
            let gb = gigabytes(bytes);
            let hours = gb / GIGABYTES_PER_HOUR;
            say(check(
                "disk-space",
                gb >= MIN_FREE_GB,
                if gb >= MIN_FREE_GB {
                    format!(
                        "The drive that holds {} has {gb:.1} gigabytes free, enough for about {hours:.1} hours of recording at the default quality (about {GIGABYTES_PER_HOUR:.0} gigabytes an hour).",
                        opts.dir.display()
                    )
                } else {
                    format!(
                        "The drive that holds {} has only {gb:.1} gigabytes free. A recording takes about {GIGABYTES_PER_HOUR:.0} gigabytes an hour and could stop part-way. Free up space or choose another drive.",
                        opts.dir.display()
                    )
                },
            ));
        }
        None => say(check(
            "disk-space",
            true,
            format!("The free space on the drive that holds {} could not be told. A recording takes about {GIGABYTES_PER_HOUR:.0} gigabytes an hour.", opts.dir.display()),
        )),
    }
    all_fine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioDevice;
    use crate::video::{EncoderInfo, SinkSpec, VideoInspection, VideoSink};
    use crate::{DisplayInfo, FrameSource, ScreenError, WindowsVersion};
    use std::time::Duration;

    struct Screen {
        build: u32,
        capture: bool,
        displays: Vec<DisplayInfo>,
    }
    impl Platform for Screen {
        fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
            Ok(WindowsVersion { major: 10, minor: 0, build: self.build })
        }
        fn capture_supported(&self) -> bool {
            self.capture
        }
        fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
            Ok(self.displays.clone())
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
    }

    struct Video {
        h264: Vec<EncoderInfo>,
        aac: Result<bool, &'static str>,
    }
    impl VideoPlatform for Video {
        fn start_media(&self) -> Result<(), ScreenError> {
            Ok(())
        }
        fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
            Ok(self.h264.clone())
        }
        fn aac_encoder_available(&self) -> Result<bool, ScreenError> {
            self.aac.map_err(|e| ScreenError::Failed(e.into()))
        }
        fn create_sink(&self, _: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
        fn inspect_video(&self, _: &Path) -> Result<VideoInspection, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
        fn process_cpu_time(&self) -> Option<Duration> {
            None
        }
        fn logical_cores(&self) -> u32 {
            1
        }
    }

    struct Audio(Vec<AudioDevice>);
    impl AudioPlatform for Audio {
        fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
            Ok(self.0.clone())
        }
        fn start_microphone(&self, _: Option<&AudioDevice>) -> Result<Box<dyn crate::audio::AudioSource>, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
        fn start_system_audio(
            &self,
            _: Option<&AudioDevice>,
        ) -> Result<Box<dyn crate::audio::AudioSource>, ScreenError> {
            Err(ScreenError::Failed("not used".into()))
        }
    }

    struct Facts(Access, Option<u64>);
    impl SystemFacts for Facts {
        fn microphone_access(&self) -> Access {
            self.0.clone()
        }
        fn free_disk_bytes(&self, _: &Path) -> Option<u64> {
            self.1
        }
    }

    fn display() -> DisplayInfo {
        DisplayInfo {
            number: 1,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            main: true,
            system_name: "D1".into(),
            handle: 1,
        }
    }
    fn device(kind: DeviceKind, number: u32, name: &str, is_default: bool) -> AudioDevice {
        AudioDevice { kind, number, name: name.into(), is_default, id: format!("{kind:?}{number}") }
    }
    fn good_devices() -> Vec<AudioDevice> {
        vec![
            device(DeviceKind::Microphone, 1, "Shure MV7", true),
            device(DeviceKind::SoundOutput, 1, "Speakers (Realtek)", true),
        ]
    }

    fn run(screen: Screen, video: Video, devices: Vec<AudioDevice>, facts: Facts) -> (bool, Vec<Check>) {
        let mut checks = Vec::new();
        let ok = run_setup_check_with(
            &screen,
            &video,
            &Audio(devices),
            &facts,
            &SetupOptions { dir: PathBuf::from("D:\\Recordings") },
            &mut |c| checks.push(c),
        );
        (ok, checks)
    }

    fn good_screen() -> Screen {
        Screen { build: 26200, capture: true, displays: vec![display()] }
    }
    fn good_video() -> Video {
        Video { h264: vec![EncoderInfo { name: "NVIDIA H.264 Encoder MFT".into(), hardware: true }], aac: Ok(true) }
    }
    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn a_ready_computer_passes_every_check_in_words() {
        let (ok, checks) =
            run(good_screen(), good_video(), good_devices(), Facts(Access::Allowed, Some(500 * 1_073_741_824)));
        assert!(ok, "{checks:?}");
        let names: Vec<&str> = checks.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(
            names,
            [
                "windows-version",
                "screen-capture",
                "displays",
                "video-encoder",
                "sound-encoder",
                "microphone",
                "sound-output",
                "microphone-access",
                "disk-space"
            ]
        );
        assert!(find(&checks, "windows-version").message.contains("Windows 11 or later (version 10.0, build 26200)"));
        assert!(find(&checks, "displays").message.contains("Found 1 display: Display 1, 1920 by 1080, main display."));
        assert!(find(&checks, "microphone").message.contains("the default is Shure MV7"));
        assert!(find(&checks, "disk-space").message.contains("500.0 gigabytes free, enough for about 100.0 hours"));
    }

    #[test]
    fn windows_10_is_refused_in_words() {
        let (ok, checks) = run(
            Screen { build: 19045, ..good_screen() },
            good_video(),
            good_devices(),
            Facts(Access::Allowed, Some(10 * 1_073_741_824)),
        );
        assert!(!ok);
        let c = find(&checks, "windows-version");
        assert!(!c.ok && c.message.contains("needs Windows 11 or later"), "{}", c.message);
    }

    #[test]
    fn missing_media_parts_say_what_to_install() {
        let video = Video { h264: vec![], aac: Ok(false) };
        let (ok, checks) = run(good_screen(), video, good_devices(), Facts(Access::Allowed, Some(10 * 1_073_741_824)));
        assert!(!ok);
        for name in ["video-encoder", "sound-encoder"] {
            let c = find(&checks, name);
            assert!(
                !c.ok && c.message.contains("Media Feature Pack") && c.message.contains("\"N\" editions"),
                "{name}: {}",
                c.message
            );
        }
    }

    #[test]
    fn a_blocked_microphone_is_a_problem_with_the_fix_but_a_missing_one_is_not() {
        let blocked = Access::Blocked(
            "Open Settings, Privacy and security, Microphone, and turn on Let desktop apps access your microphone."
                .into(),
        );
        let (ok, checks) = run(good_screen(), good_video(), good_devices(), Facts(blocked, Some(10 * 1_073_741_824)));
        assert!(!ok);
        let c = find(&checks, "microphone-access");
        assert!(
            !c.ok
                && c.message.contains("turn on Let desktop apps access your microphone")
                && c.message.contains("will stop at the start")
        );

        let none = vec![device(DeviceKind::SoundOutput, 1, "Speakers", true)];
        let (ok, checks) = run(good_screen(), good_video(), none, Facts(Access::Allowed, Some(10 * 1_073_741_824)));
        assert!(ok, "optional parts never fail the check: {checks:?}");
        assert!(find(&checks, "microphone").message.contains("will have no voice"));
        let neither: Vec<AudioDevice> = vec![];
        let (ok, checks) = run(good_screen(), good_video(), neither, Facts(Access::Unknown, None));
        assert!(ok);
        assert!(find(&checks, "sound-output").message.contains("cannot be recorded"));
        assert!(find(&checks, "microphone-access").message.contains("did not say"));
        assert!(find(&checks, "disk-space").message.contains("could not be told"));
    }

    #[test]
    fn little_free_space_is_a_problem_with_the_amount() {
        let (ok, checks) =
            run(good_screen(), good_video(), good_devices(), Facts(Access::Allowed, Some(1_073_741_824)));
        assert!(!ok);
        let c = find(&checks, "disk-space");
        assert!(
            !c.ok && c.message.contains("only 1.0 gigabytes free") && c.message.contains("D:\\Recordings"),
            "{}",
            c.message
        );
        // Exactly the minimum is fine.
        let (ok, _) = run(good_screen(), good_video(), good_devices(), Facts(Access::Allowed, Some(2 * 1_073_741_824)));
        assert!(ok);
    }

    #[test]
    fn screen_capture_that_is_missing_is_explained() {
        let (ok, checks) = run(
            Screen { capture: false, displays: vec![], ..good_screen() },
            good_video(),
            good_devices(),
            Facts(Access::Allowed, Some(10 * 1_073_741_824)),
        );
        assert!(!ok);
        assert!(find(&checks, "screen-capture").message.contains("latest graphics driver"));
        assert!(!find(&checks, "displays").ok);
    }
}
