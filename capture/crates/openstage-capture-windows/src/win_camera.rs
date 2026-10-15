// SPDX-License-Identifier: Apache-2.0
//! The Windows side of the webcam: finding cameras and getting pictures from one with Media Foundation's
//! source reader.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner runs
//! `TEST WEBCAM.bat`, it has never been run. Treat every line of it as unproven.
//!
//! A camera offers several modes (a size, a rate and a form such as MJPG or YUY2). [`pick_mode`] chooses the one
//! closest to 1280 by 720 at 30 a second, the camera is set to that mode, and Windows' own converters turn
//! whatever the camera sends into plain blue-green-red pixels. A thread of its own waits for each picture and puts
//! it, with the time the camera says it was taken and the time it arrived, in a waiting line; the recording thread
//! takes them from there.

use crate::queue::FrameQueue;
use crate::video::VideoPlatform;
use crate::webcam::{
    number_cameras, pick_mode, CameraInfo, CameraMode, CameraPlatform, ModeOption, PictureForm, StartedCamera,
};
use crate::{now_100ns, Frame, FrameSource, ScreenError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::GUID;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSourceReader, MFCreateAttributes, MFCreateMediaType,
    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFMediaType_Video, MFVideoFormat_MJPG,
    MFVideoFormat_NV12, MFVideoFormat_RGB32, MFVideoFormat_YUY2, MFVideoInterlace_Progressive,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

/// How many pictures may wait to be taken; beyond that the oldest is dropped for the newest. A webcam sends 30 a
/// second, so this is about a third of a second.
const QUEUE_DEPTH: usize = 10;

fn failed(what: &str, e: windows::core::Error) -> ScreenError {
    ScreenError::Failed(format!("{what} (Windows error code 0x{:08X}: {})", e.code().0 as u32, e.message()))
}

/// What a few Windows errors mean for a person with a camera, in words, or `None` for any other error.
fn known_cause(e: &windows::core::Error) -> Option<&'static str> {
    use windows::Win32::Foundation::E_ACCESSDENIED;
    use windows::Win32::Media::MediaFoundation::{
        MF_E_HW_MFT_FAILED_START_STREAMING, MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED,
        MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED,
    };
    let code = e.code();
    if code == E_ACCESSDENIED {
        Some("Windows is not letting this program use cameras. Open Settings, Privacy & security, Camera, and turn on both \"Camera access\" and \"Let desktop apps access your camera\".")
    } else if code == MF_E_HW_MFT_FAILED_START_STREAMING {
        Some("another program is using the camera. Close it (a video call, the Camera app) and try again.")
    } else if code == MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED {
        Some("another program has taken the camera.")
    } else if code == MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED {
        Some("the camera was unplugged, or turned off in Windows.")
    } else {
        None
    }
}

/// What to tell the author when Windows refuses to open a camera.
fn explain_refusal(what: &str, e: &windows::core::Error) -> ScreenError {
    match known_cause(e) {
        Some(cause) => ScreenError::Failed(format!("{what}: {cause} (Windows error code 0x{:08X})", e.code().0 as u32)),
        None => failed(what, e.clone()),
    }
}

/// Two 32-bit numbers packed into one 64-bit attribute, the way Media Foundation keeps sizes and ratios.
fn unpack(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32)
}

fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

/// The four letters a picture form is known by, such as "MJPG" or "YUY2", or "other".
fn form_name(subtype: &GUID) -> String {
    let letters = subtype.data1.to_le_bytes();
    if letters.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(&letters).into_owned()
    } else {
        "other".to_string()
    }
}

fn string_of(activate: &IMFActivate, key: &GUID) -> Option<String> {
    let mut buffer = [0u16; 512];
    let length = unsafe { activate.GetStringLength(key) }.ok()? as usize;
    if length >= buffer.len() {
        return None;
    }
    unsafe { activate.GetString(key, &mut buffer, None) }.ok()?;
    Some(String::from_utf16_lossy(&buffer[..length]))
}

/// Every camera Windows knows, as the things to open them with, in the order Windows lists them.
fn enumerate() -> Result<Vec<(CameraInfo, IMFActivate)>, ScreenError> {
    crate::win::prepare();
    crate::win_video::WindowsVideo.start_media()?;
    let mut attributes: Option<IMFAttributes> = None;
    unsafe { MFCreateAttributes(&mut attributes, 1) }
        .map_err(|e| failed("Could not prepare to look for cameras", e))?;
    let attributes = attributes.ok_or_else(|| ScreenError::Failed("Could not prepare to look for cameras.".into()))?;
    unsafe { attributes.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID) }
        .map_err(|e| failed("Could not ask Windows for cameras", e))?;
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    unsafe { MFEnumDeviceSources(&attributes, &mut list, &mut count) }
        .map_err(|e| failed("Windows would not list its cameras", e))?;
    let mut found = Vec::new();
    if !list.is_null() {
        for i in 0..count as usize {
            // Taking the item out of the array hands its reference to us, so it is released when dropped.
            let activate = unsafe { (*list.add(i)).take() };
            if let Some(activate) = activate {
                let name = string_of(&activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                    .unwrap_or_else(|| "an unnamed camera".to_string());
                let id =
                    string_of(&activate, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK).unwrap_or_default();
                found.push((CameraInfo { number: 0, name, id }, activate));
            }
        }
        unsafe { CoTaskMemFree(Some(list as *const _)) };
    }
    // Number them in the order given.
    let infos = number_cameras(found.iter().map(|(info, _)| info.clone()).collect());
    Ok(infos.into_iter().zip(found.into_iter().map(|(_, activate)| activate)).collect())
}

pub struct WindowsCameras;

impl CameraPlatform for WindowsCameras {
    fn list_cameras(&self) -> Result<Vec<CameraInfo>, ScreenError> {
        Ok(enumerate()?.into_iter().map(|(info, _)| info).collect())
    }

    fn start_camera(&self, camera: &CameraInfo) -> Result<StartedCamera, ScreenError> {
        let (_, activate) = enumerate()?
            .into_iter()
            .find(|(info, _)| info.id == camera.id && !camera.id.is_empty() || info.name == camera.name)
            .ok_or_else(|| {
                ScreenError::Failed(format!(
                    "The camera {} is no longer there. Is it still plugged in?",
                    camera.describe()
                ))
            })?;
        unsafe { start(&camera.name, &activate) }
    }
}

/// Open the camera, choose its mode, and start the thread that takes its pictures.
unsafe fn start(name: &str, activate: &IMFActivate) -> Result<StartedCamera, ScreenError> {
    let source: IMFMediaSource = activate
        .ActivateObject()
        .map_err(|e| explain_refusal(&format!("Windows would not open the camera {name}"), &e))?;
    // From here on a failure must close the camera again, or it stays in use.
    match open_reader(&source) {
        Ok((reader, mode, layout)) => {
            let shared = Arc::new(Shared {
                queue: FrameQueue::new(QUEUE_DEPTH),
                stop: AtomicBool::new(false),
                finished: AtomicBool::new(false),
                ended: Mutex::new(None),
            });
            let thread = {
                let shared = Arc::clone(&shared);
                let reader = Mover(reader);
                std::thread::Builder::new()
                    .name("webcam".into())
                    .spawn(move || {
                        let reader = reader;
                        read_pictures(&reader.0, &shared, layout);
                        shared.finished.store(true, Ordering::Release);
                    })
                    .map_err(|e| {
                        let _ = source.Shutdown();
                        ScreenError::Failed(format!("Could not start listening to the camera: {e}"))
                    })?
            };
            Ok(StartedCamera { source: Box::new(CameraSource { shared, thread: Some(thread), source }), mode })
        }
        Err(e) => {
            let _ = source.Shutdown();
            Err(e)
        }
    }
}

/// A Windows object handed to another thread. Media Foundation's source reader may be used from any thread (it is
/// free-threaded), but the Rust bindings cannot know that.
struct Mover<T>(T);
unsafe impl<T> Send for Mover<T> {}

/// How a picture sits in memory: its size, the length of a row in bytes, and whether the rows run from the bottom up.
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) bottom_up: bool,
}

/// What the reader is now giving: asked of the current output type.
pub(crate) unsafe fn layout_of(reader: &IMFSourceReader) -> Result<Layout, ScreenError> {
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let current = reader
        .GetCurrentMediaType(stream)
        .map_err(|e| failed("Windows would not say what the camera's pictures look like", e))?;
    let (width, height) = unpack(
        current
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|e| failed("Windows would not say how big the camera's pictures are", e))?,
    );
    // The stride is a signed number kept in an unsigned attribute: negative means the rows run from the bottom up.
    let stride = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|v| v as i32).unwrap_or(width as i32 * 4);
    Ok(Layout {
        width,
        height,
        stride: if stride == 0 { width as usize * 4 } else { stride.unsigned_abs() as usize },
        bottom_up: stride < 0,
    })
}

/// Choose the camera's mode, and ask Windows for plain pixels from it.
unsafe fn open_reader(source: &IMFMediaSource) -> Result<(IMFSourceReader, CameraMode, Layout), ScreenError> {
    let mut attributes: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attributes, 2).map_err(|e| failed("Could not prepare to read the camera", e))?;
    let attributes = attributes.ok_or_else(|| ScreenError::Failed("Could not prepare to read the camera.".into()))?;
    // Let Windows turn the camera's form (MJPG, YUY2 ...) into plain pixels.
    attributes
        .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
        .map_err(|e| failed("Could not ask Windows to convert the camera's pictures", e))?;
    let reader = MFCreateSourceReaderFromMediaSource(source, &attributes)
        .map_err(|e| failed("Windows would not start reading the camera", e))?;
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    // Every mode the camera offers.
    let mut natives: Vec<IMFMediaType> = Vec::new();
    let mut options: Vec<ModeOption> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for index in 0.. {
        let Ok(native) = reader.GetNativeMediaType(stream, index) else { break };
        let size = native.GetUINT64(&MF_MT_FRAME_SIZE).ok().map(unpack);
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).ok().map(unpack);
        let subtype = native.GetGUID(&MF_MT_SUBTYPE).ok();
        if let (Some((width, height)), Some((numerator, denominator)), Some(subtype)) = (size, rate, subtype) {
            let form = if subtype == MFVideoFormat_NV12 || subtype == MFVideoFormat_YUY2 {
                PictureForm::Raw
            } else if subtype == MFVideoFormat_MJPG {
                PictureForm::Mjpeg
            } else {
                PictureForm::Other
            };
            options.push(ModeOption {
                width,
                height,
                fps: if denominator == 0 { 0.0 } else { numerator as f64 / denominator as f64 },
                form,
            });
            names.push(form_name(&subtype));
            natives.push(native);
        }
        if index > 500 {
            break;
        }
    }
    let Some(choice) = pick_mode(&options) else {
        return Err(ScreenError::Failed(
            "The camera offers no picture size this program can record (it needs at least 320 by 180, an even width and height, and 5 pictures a second).".into(),
        ));
    };

    // First set the camera itself to that mode, then ask for plain pixels of the same size.
    reader.SetStreamSelection(stream, true).map_err(|e| failed("Windows would not select the camera's picture", e))?;
    reader
        .SetCurrentMediaType(stream, None, &natives[choice])
        .map_err(|e| failed("Windows would not set the camera to the mode chosen for it", e))?;
    let chosen = &options[choice];
    let plain: IMFMediaType = MFCreateMediaType().map_err(|e| failed("Could not describe the pictures wanted", e))?;
    plain
        .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
        .map_err(|e| failed("Could not describe the pictures wanted", e))?;
    plain
        .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
        .map_err(|e| failed("Could not describe the pictures wanted", e))?;
    plain
        .SetUINT64(&MF_MT_FRAME_SIZE, pack(chosen.width, chosen.height))
        .map_err(|e| failed("Could not describe the pictures wanted", e))?;
    plain
        .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
        .map_err(|e| failed("Could not describe the pictures wanted", e))?;
    // A positive stride asks for rows that run from the top down (the default for this form is bottom to top).
    plain
        .SetUINT32(&MF_MT_DEFAULT_STRIDE, chosen.width * 4)
        .map_err(|e| failed("Could not describe the pictures wanted", e))?;
    reader
        .SetCurrentMediaType(stream, None, &plain)
        .map_err(|e| failed("Windows could not turn the camera's pictures into plain pixels", e))?;
    let layout = layout_of(&reader)?;
    let mode =
        CameraMode { width: layout.width, height: layout.height, fps: chosen.fps, format: names[choice].clone() };
    Ok((reader, mode, layout))
}

struct Shared {
    queue: FrameQueue,
    stop: AtomicBool,
    finished: AtomicBool,
    ended: Mutex<Option<String>>,
}

/// The thread that waits for each picture, one after another, until told to stop or the camera goes.
fn read_pictures(reader: &IMFSourceReader, shared: &Shared, mut layout: Layout) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let started = now_100ns();
    let tell = |why: String| {
        if let Ok(mut ended) = shared.ended.lock() {
            ended.get_or_insert(why);
        }
    };
    while !shared.stop.load(Ordering::Acquire) {
        let mut flags = 0u32;
        let mut taken = 0i64;
        let mut sample = None;
        let result =
            unsafe { reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut taken), Some(&mut sample)) };
        let arrived = now_100ns();
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if let Err(e) = result {
            tell(match known_cause(&e) {
                Some(cause) => format!("The camera stopped sending pictures: {cause}"),
                None => format!(
                    "Windows stopped sending pictures from the camera (Windows error code 0x{:08X}: {}).",
                    e.code().0 as u32,
                    e.message()
                ),
            });
            break;
        }
        if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
            tell("Windows reported an error from the camera.".to_string());
            break;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            tell(
                "The camera stopped sending pictures (it may have been unplugged, or another program took it)."
                    .to_string(),
            );
            break;
        }
        if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
            if let Ok(now) = unsafe { layout_of(reader) } {
                layout = now;
            }
        }
        // A gap in the stream, or a tick, comes with no picture.
        let Some(sample) = sample else { continue };
        let length = layout.width as usize * layout.height as usize * 4;
        let mut bgra = shared.queue.take_buffer(length);
        if !unsafe { copy_picture(&sample, layout, &mut bgra) } {
            shared.queue.recycle_memory(bgra);
            continue;
        }
        shared.queue.push(Frame {
            width: layout.width,
            height: layout.height,
            t_ms: (arrived.saturating_sub(started) / 10_000).max(0) as u64,
            captured_100ns: Some(taken),
            arrived_100ns: Some(arrived),
            bgra,
        });
    }
}

/// Copy a picture out of Windows' memory into ours, rows from the top, whatever order Windows has them in.
/// `false` if the picture was too short to be a whole picture.
pub(crate) unsafe fn copy_picture(
    sample: &windows::Win32::Media::MediaFoundation::IMFSample,
    layout: Layout,
    out: &mut [u8],
) -> bool {
    let Ok(buffer) = sample.ConvertToContiguousBuffer() else { return false };
    let mut data: *mut u8 = std::ptr::null_mut();
    let mut used = 0u32;
    if buffer.Lock(&mut data, None, Some(&mut used)).is_err() || data.is_null() {
        return false;
    }
    let row = layout.width as usize * 4;
    let rows = layout.height as usize;
    let whole = layout.stride * rows;
    let ok = used as usize >= whole && layout.stride >= row;
    if ok {
        let memory = std::slice::from_raw_parts(data, used as usize);
        for r in 0..rows {
            // Bottom-up pictures have their last row (the top of the picture) first.
            let from = if layout.bottom_up { (rows - 1 - r) * layout.stride } else { r * layout.stride };
            out[r * row..(r + 1) * row].copy_from_slice(&memory[from..from + row]);
        }
    }
    let _ = buffer.Unlock();
    ok
}

/// A camera sending pictures.
struct CameraSource {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Kept so the camera can be shut down; that is what ends a read that is waiting.
    source: IMFMediaSource,
}

impl FrameSource for CameraSource {
    fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
        self.shared.queue.pop(timeout)
    }
    fn dropped(&self) -> u64 {
        self.shared.queue.replaced()
    }
    fn received(&self) -> u64 {
        self.shared.queue.pushed()
    }
    fn recycle(&mut self, frame: Frame) {
        self.shared.queue.recycle(frame);
    }
    fn ended(&self) -> Option<String> {
        self.shared.ended.lock().ok().and_then(|e| e.clone())
    }
}

impl Drop for CameraSource {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // Shutting the camera down ends the read that the thread is waiting in.
        let _ = unsafe { self.source.Shutdown() };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.shared.finished.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.shared.finished.load(Ordering::Acquire) {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
        // A thread that has not finished by now is left to end by itself; the camera is already shut.
    }
}
