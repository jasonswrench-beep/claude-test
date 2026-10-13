// SPDX-License-Identifier: Apache-2.0
//! The Windows side of sound: WASAPI, Windows' own audio capture service.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner
//! runs `TEST AUDIO CAPTURE.bat`, it has never been run. Treat every line of it as unproven.
//!
//! Each stream (the microphone, and what the computer is playing) runs on its own thread, asks
//! Windows for 48,000 samples per second, stereo, 16 bits (Windows converts from whatever the
//! device really uses), and collects whatever arrives. The recorder takes it from there.

use crate::audio::{AudioDevice, AudioInfo, AudioPlatform, AudioSource, DeviceKind, CHANNELS, SAMPLE_RATE};
use crate::ScreenError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_DeviceInterface_FriendlyName, PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

/// How big Windows' own buffer is, in units of 100 nanoseconds (100 milliseconds).
const BUFFER_100NS: i64 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Microphone,
    SystemSound,
}

impl Kind {
    fn device_kind(self) -> DeviceKind {
        match self {
            Kind::Microphone => DeviceKind::Microphone,
            Kind::SystemSound => DeviceKind::SoundOutput,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Microphone => "microphone",
            Kind::SystemSound => "sound output",
        }
    }
}

pub struct WindowsAudio;

impl AudioPlatform for WindowsAudio {
    fn list_devices(&self) -> Result<Vec<AudioDevice>, ScreenError> {
        // Windows' components must be started on every thread that uses them.
        crate::win::prepare();
        let mut all = unsafe { list(Kind::Microphone) }.map_err(ScreenError::Failed)?;
        all.extend(unsafe { list(Kind::SystemSound) }.map_err(ScreenError::Failed)?);
        Ok(all)
    }

    fn start_microphone(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
        start(Kind::Microphone, device.cloned())
    }

    fn start_system_audio(&self, device: Option<&AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
        start(Kind::SystemSound, device.cloned())
    }
}

/// What the capture thread and the reader share.
struct Shared {
    samples: Mutex<Vec<i16>>,
    lost: AtomicU64,
    failure: Mutex<Option<String>>,
    stop: AtomicBool,
    /// While set, what is heard is thrown away (recording is paused).
    paused: AtomicBool,
}

struct Wasapi {
    info: AudioInfo,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl AudioSource for Wasapi {
    fn info(&self) -> &AudioInfo {
        &self.info
    }

    fn read(&mut self) -> Vec<i16> {
        std::mem::take(&mut *self.shared.samples.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn discontinuities(&self) -> u64 {
        self.shared.lost.load(Ordering::Relaxed)
    }

    fn failure(&self) -> Option<String> {
        self.shared.failure.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_paused(&mut self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }
}

impl Drop for Wasapi {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn start(kind: Kind, device: Option<AudioDevice>) -> Result<Box<dyn AudioSource>, ScreenError> {
    let shared = Arc::new(Shared {
        samples: Mutex::new(Vec::new()),
        lost: AtomicU64::new(0),
        failure: Mutex::new(None),
        stop: AtomicBool::new(false),
        paused: AtomicBool::new(false),
    });
    let (tx, rx) = channel::<Result<AudioInfo, String>>();
    let for_thread = Arc::clone(&shared);
    let handle = thread::spawn(move || capture_thread(kind, device, for_thread, tx));
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(info)) => Ok(Box::new(Wasapi { info, shared, thread: Some(handle) })),
        Ok(Err(why)) => {
            let _ = handle.join();
            Err(ScreenError::Failed(why))
        }
        Err(_) => {
            shared.stop.store(true, Ordering::Relaxed);
            Err(ScreenError::Failed(format!(
                "Windows did not answer within 10 seconds when asked to start the {}.",
                kind.label()
            )))
        }
    }
}

/// Windows' error, in words: with advice for the common cause of each problem.
fn describe(kind: Kind, what: &str, e: &windows::core::Error) -> String {
    let code = e.code().0 as u32;
    let advice = match code {
        // E_ACCESSDENIED
        0x8007_0005 if kind == Kind::Microphone => {
            " Windows may be blocking microphone access: open Settings, Privacy and security, Microphone, and allow desktop apps to use it."
        }
        // E_NOTFOUND: no such device
        0x8007_0490 => " No such device was found; check that one is connected and enabled in the Windows sound settings.",
        // AUDCLNT_E_DEVICE_IN_USE
        0x8889_000A => " Another program has taken the device for its own exclusive use.",
        _ => "",
    };
    format!("{what} (Windows error code 0x{code:08X}: {}).{advice}", e.message().trim())
}

fn code_text(e: &windows::core::Error) -> String {
    format!("0x{:08X}", e.code().0 as u32)
}

/// One text property of a device, or why it could not be read.
fn text_property(store: &IPropertyStore, key: &PROPERTYKEY) -> Result<String, String> {
    unsafe {
        let value = store.GetValue(key).map_err(|e| format!("the value could not be read ({})", code_text(&e)))?;
        let text: PWSTR =
            PropVariantToStringAlloc(&value).map_err(|e| format!("it is not text ({})", code_text(&e)))?;
        // Lossy, so odd characters in a name can never make it unreadable.
        let name = String::from_utf16_lossy(text.as_wide()).trim().to_string();
        CoTaskMemFree(Some(text.0 as *const _));
        if name.is_empty() {
            Err("it is empty".to_string())
        } else {
            Ok(name)
        }
    }
}

/// What Windows calls a device. Tries the name, then the description, then the hardware's own name.
fn device_name(device: &IMMDevice) -> Result<String, String> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }
        .map_err(|e| format!("Windows would not open its details ({})", code_text(&e)))?;
    let mut reasons = Vec::new();
    for (what, key) in [
        ("name", &PKEY_Device_FriendlyName),
        ("description", &PKEY_Device_DeviceDesc),
        ("hardware name", &PKEY_DeviceInterface_FriendlyName),
    ] {
        match text_property(&store, key) {
            Ok(name) => return Ok(name),
            Err(why) => reasons.push(format!("{what}: {why}")),
        }
    }
    Err(reasons.join("; "))
}

fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let id = device.GetId().ok()?;
        let text = id.to_string().ok();
        CoTaskMemFree(Some(id.0 as *const _));
        text
    }
}

/// Every active device of one kind, numbered in the order Windows lists them, with the default marked.
unsafe fn list(kind: Kind) -> Result<Vec<AudioDevice>, String> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
        .map_err(|e| describe(kind, "Windows' sound service could not be reached", &e))?;
    let flow = match kind {
        Kind::Microphone => eCapture,
        Kind::SystemSound => eRender,
    };
    let default_id = enumerator.GetDefaultAudioEndpoint(flow, eConsole).ok().and_then(|d| device_id(&d));
    let collection = enumerator
        .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
        .map_err(|e| describe(kind, &format!("Windows would not list its {}s", kind.label()), &e))?;
    let count = collection.GetCount().map_err(|e| describe(kind, "Windows would not count its devices", &e))?;
    let mut devices = Vec::new();
    for i in 0..count {
        let Ok(device) = collection.Item(i) else { continue };
        let Some(id) = device_id(&device) else { continue };
        // A device whose name cannot be read is still listed, with the reason, so it can still be chosen by number.
        let name = match device_name(&device) {
            Ok(n) => crate::plain_name(&n),
            Err(why) => format!("{} {} (its name could not be read: {why})", kind.label(), i + 1),
        };
        devices.push(AudioDevice {
            kind: kind.device_kind(),
            number: i + 1,
            name,
            is_default: default_id.as_deref() == Some(id.as_str()),
            id,
        });
    }
    Ok(devices)
}

struct Opened {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    info: AudioInfo,
}

unsafe fn open(kind: Kind, chosen: Option<AudioDevice>) -> Result<Opened, String> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
        .map_err(|e| describe(kind, "Windows' sound service could not be reached", &e))?;
    let (flow, extra_flags) = match kind {
        Kind::Microphone => (eCapture, 0),
        // "Loopback": listen to what a sound output is playing.
        Kind::SystemSound => (eRender, AUDCLNT_STREAMFLAGS_LOOPBACK),
    };
    let device = match &chosen {
        // A device the person chose: open exactly that one.
        Some(c) => enumerator
            .GetDevice(&HSTRING::from(c.id.as_str()))
            .map_err(|e| describe(kind, &format!("Could not open {}", c.name), &e))?,
        None => enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|e| describe(kind, &format!("Windows has no default {} to listen to", kind.label()), &e))?,
    };
    let name =
        device_name(&device).map(|n| crate::plain_name(&n)).unwrap_or_else(|_| format!("the chosen {}", kind.label()));
    let client: IAudioClient =
        device.Activate(CLSCTX_ALL, None).map_err(|e| describe(kind, &format!("Could not open {name}"), &e))?;

    // Ask for plain 16-bit stereo at 48,000 samples per second; Windows converts from what the device really does.
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * CHANNELS as u32 * 2,
        nBlockAlign: CHANNELS * 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            extra_flags | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            BUFFER_100NS,
            0,
            &format,
            None,
        )
        .map_err(|e| describe(kind, &format!("Could not start listening to {name}"), &e))?;
    let capture: IAudioCaptureClient =
        client.GetService().map_err(|e| describe(kind, "Could not get the sound from the device", &e))?;
    client.Start().map_err(|e| describe(kind, &format!("Could not start {name}"), &e))?;
    Ok(Opened { client, capture, info: AudioInfo { device: name, sample_rate: SAMPLE_RATE, channels: CHANNELS } })
}

fn capture_thread(
    kind: Kind,
    device: Option<AudioDevice>,
    shared: Arc<Shared>,
    started: std::sync::mpsc::Sender<Result<AudioInfo, String>>,
) {
    // Windows' components must be started on every thread that uses them.
    crate::win::prepare();
    let opened = match unsafe { open(kind, device) } {
        Ok(o) => o,
        Err(why) => {
            let _ = started.send(Err(why));
            return;
        }
    };
    let _ = started.send(Ok(opened.info.clone()));

    let channels = opened.info.channels as usize;
    while !shared.stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(10));
        loop {
            let waiting = match unsafe { opened.capture.GetNextPacketSize() } {
                Ok(n) => n,
                Err(e) => {
                    *shared.failure.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(describe(kind, "Windows stopped delivering sound", &e));
                    shared.stop.store(true, Ordering::Relaxed);
                    break;
                }
            };
            if waiting == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            if let Err(e) = unsafe { opened.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) } {
                *shared.failure.lock().unwrap_or_else(|p| p.into_inner()) =
                    Some(describe(kind, "Windows stopped delivering sound", &e));
                shared.stop.store(true, Ordering::Relaxed);
                break;
            }
            let count = frames as usize * channels;
            let mut block = vec![0i16; count];
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 && !data.is_null() {
                // Copied byte by byte into properly aligned memory, so the source's alignment does not matter.
                unsafe { std::ptr::copy_nonoverlapping(data, block.as_mut_ptr() as *mut u8, count * 2) };
            }
            if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                shared.lost.fetch_add(1, Ordering::Relaxed);
            }
            // Sound heard while paused is not part of the recording, so it is not kept.
            if !shared.paused.load(Ordering::Relaxed) {
                shared.samples.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(&block);
            }
            let _ = unsafe { opened.capture.ReleaseBuffer(frames) };
        }
    }
    let _ = unsafe { opened.client.Stop() };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a brand new thread, with nothing prepared in advance, starting to listen must still reach
    /// Windows' sound service. (A computer with no microphone or no sound output gives a different,
    /// honest error; only the "not started" mistake is what this guards against.)
    #[test]
    fn listening_starts_properly_from_a_thread_nobody_prepared() {
        let results = std::thread::spawn(|| {
            let audio = WindowsAudio;
            (
                audio.start_microphone(None).err().map(|e| e.to_string()),
                audio.start_system_audio(None).err().map(|e| e.to_string()),
                audio.list_devices().err().map(|e| e.to_string()),
            )
        })
        .join()
        .unwrap();
        for text in [results.0, results.1, results.2].into_iter().flatten() {
            assert!(!text.contains("CoInitialize has not been called"), "the thread was not prepared: {text}");
        }
    }
}
