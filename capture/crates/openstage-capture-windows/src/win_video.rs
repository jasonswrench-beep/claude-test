// SPDX-License-Identifier: Apache-2.0
//! The Windows side of the video test: Media Foundation's sink writer makes an H.264 MP4.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner
//! runs `TEST VIDEO CAPTURE.bat`, it has never been run. Treat every line of it as unproven.

use crate::video::{EncoderInfo, SinkSpec, VideoInspection, VideoPlatform, VideoSink};
use crate::{Frame, ScreenError};
use std::path::Path;
use std::sync::Once;
use std::time::Duration;

use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_High, IMFActivate, IMFAttributes, IMFMediaType, IMFSinkWriter, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Video, MFShutdown,
    MFStartup, MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264, MFVideoFormat_RGB32,
    MFVideoInterlace_Progressive, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT, MFT_REGISTER_TYPE_INFO, MF_MT_AVG_BITRATE,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_VERSION,
};
use windows::Win32::Media::MediaFoundation::{
    MFCreateSourceReaderFromURL, MFTranscodeContainerType_FMPEG4, MF_MPEG4SINK_MIN_FRAGMENT_DURATION,
    MF_MT_MAX_KEYFRAME_SPACING, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_TRANSCODE_CONTAINERTYPE,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

fn failed(what: &str, e: windows::core::Error) -> ScreenError {
    ScreenError::Failed(format!("{what} (Windows error code 0x{:08X}: {})", e.code().0 as u32, e.message()))
}

/// Two 32-bit numbers packed into one 64-bit attribute, the way Media Foundation wants sizes and ratios.
fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

pub struct WindowsVideo;

impl VideoPlatform for WindowsVideo {
    fn start_media(&self) -> Result<(), ScreenError> {
        // Windows' video tools need the Windows Runtime started first (the screen capture code does the same).
        crate::win::prepare();
        static ONCE: Once = Once::new();
        let mut result = Ok(());
        ONCE.call_once(|| {
            // Windows' own capture code started COM when it first ran; the video tools need only this.
            result = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
                .map_err(|e| failed("Media Foundation did not start", e));
        });
        // A second call in the same run finds it already started.
        result
    }

    fn h264_encoders(&self) -> Result<Vec<EncoderInfo>, ScreenError> {
        let mut found = enumerate(true)?;
        found.extend(enumerate(false)?);
        Ok(found)
    }

    fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
        let (width, height) = (spec.width, spec.height);
        // First let Windows use the graphics card's encoder. If that setup is refused, try the processor's.
        match unsafe { build_writer(spec, true) } {
            Ok(writer) => Ok(Box::new(Mp4Sink { writer: Some(writer), stream: 0, width, height, note: None })),
            Err(first) => {
                let writer = unsafe { build_writer(spec, false) }.map_err(|second| {
                    ScreenError::Failed(format!("{first} Trying the processor's encoder instead also failed: {second}"))
                })?;
                Ok(Box::new(Mp4Sink {
                    writer: Some(writer),
                    stream: 0,
                    width,
                    height,
                    note: Some(format!(
                        "The graphics card's encoder could not be used ({first}), so the processor's encoder was used instead."
                    )),
                }))
            }
        }
    }

    fn inspect_video(&self, path: &Path) -> Result<VideoInspection, ScreenError> {
        // Open the file the way a player would, and read every video sample without decoding it.
        let reader = unsafe { MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None) }
            .map_err(|e| failed("Windows could not open the video file", e))?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let (mut frames, mut end_100ns) = (0u64, 0i64);
        loop {
            let mut flags = 0u32;
            let mut timestamp = 0i64;
            let mut sample = None;
            unsafe { reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut timestamp), Some(&mut sample)) }
                .map_err(|e| failed("Windows could not read the video file all the way through", e))?;
            if let Some(sample) = sample {
                frames += 1;
                let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0);
                end_100ns = end_100ns.max(timestamp + duration);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        Ok(VideoInspection { frames, duration_ms: (end_100ns.max(0) / 10_000) as u64 })
    }

    fn process_cpu_time(&self) -> Option<Duration> {
        let (mut created, mut exited, mut kernel, mut user) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        // FILETIME counts in units of 100 nanoseconds.
        Some(Duration::from_nanos((ticks(kernel) + ticks(user)) * 100))
    }

    fn logical_cores(&self) -> u32 {
        std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1)
    }
}

/// List the H.264 encoders: the ones that run on the graphics card, or the ones that run on the processor.
fn enumerate(hardware: bool) -> Result<Vec<EncoderInfo>, ScreenError> {
    let wanted = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let flags = if hardware { MFT_ENUM_FLAG_HARDWARE } else { MFT_ENUM_FLAG_SYNCMFT };
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&wanted),
            &mut list,
            &mut count,
        )
        .map_err(|e| failed("Windows would not list its video encoders", e))?;
    }
    let mut out = Vec::new();
    if !list.is_null() {
        for i in 0..count as usize {
            // Taking the item out of the array hands its reference to us, so it is released when dropped.
            let activate = unsafe { (*list.add(i)).take() };
            if let Some(activate) = activate {
                let name = friendly_name(&activate).unwrap_or_else(|| "an unnamed encoder".to_string());
                out.push(EncoderInfo { name, hardware });
            }
        }
        unsafe { CoTaskMemFree(Some(list as *const _)) };
    }
    Ok(out)
}

fn friendly_name(activate: &IMFActivate) -> Option<String> {
    let mut length = 0u32;
    let mut buffer = [0u16; 256];
    // First ask how long the name is, then read it.
    unsafe { activate.GetStringLength(&MFT_FRIENDLY_NAME_Attribute) }.ok().map(|n| length = n)?;
    if length as usize >= buffer.len() {
        return None;
    }
    unsafe { activate.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut buffer, None) }.ok()?;
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

unsafe fn build_writer(spec: &SinkSpec, allow_hardware: bool) -> Result<IMFSinkWriter, ScreenError> {
    let (path, width, height, fps, bitrate_kbps) = (&spec.path, spec.width, spec.height, spec.fps, spec.bitrate_kbps);
    let mut attributes: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attributes, 4).map_err(|e| failed("Could not prepare the video writer", e))?;
    let attributes = attributes.ok_or_else(|| ScreenError::Failed("Could not prepare the video writer.".into()))?;
    // Let Windows use the graphics card's encoder when there is one, and never make the writer wait.
    attributes
        .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, allow_hardware as u32)
        .map_err(|e| failed("Could not allow hardware encoding", e))?;
    attributes
        .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
        .map_err(|e| failed("Could not turn off throttling", e))?;

    if spec.fragmented {
        // A fragmented MP4: the setup is written first and then short self-contained fragments, so a
        // file cut off by a crash still plays up to the last whole fragment.
        attributes
            .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)
            .map_err(|e| failed("Could not ask for a crash-safe (fragmented) video file", e))?;
        // Close a fragment about every second.
        attributes
            .SetUINT64(&MF_MPEG4SINK_MIN_FRAGMENT_DURATION, 10_000_000)
            .map_err(|e| failed("Could not set the fragment length", e))?;
    }

    let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attributes)
        .map_err(|e| failed("Could not create the video file", e))?;

    // What goes into the file: H.264.
    let out_type: IMFMediaType = MFCreateMediaType().map_err(|e| failed("Could not describe the video", e))?;
    set_common(&out_type, &MFVideoFormat_H264, width, height, fps)?;
    out_type
        .SetUINT32(&MF_MT_AVG_BITRATE, bitrate_kbps.saturating_mul(1000))
        .map_err(|e| failed("Could not set the video quality", e))?;
    if spec.fragmented {
        // A fresh starting point every second, so that every fragment can be played on its own.
        out_type
            .SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, fps)
            .map_err(|e| failed("Could not set the keyframe spacing", e))?;
    }
    out_type
        .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)
        .map_err(|e| failed("Could not set the H.264 profile", e))?;
    let stream =
        writer.AddStream(&out_type).map_err(|e| failed("Windows would not accept the H.264 video format", e))?;

    // What the program hands in: plain pixels, blue-green-red-unused, rows top to bottom.
    let in_type: IMFMediaType = MFCreateMediaType().map_err(|e| failed("Could not describe the pictures", e))?;
    set_common(&in_type, &MFVideoFormat_RGB32, width, height, fps)?;
    // A positive stride tells Windows the rows run top to bottom (the default for this format is bottom to top).
    in_type.SetUINT32(&MF_MT_DEFAULT_STRIDE, width * 4).map_err(|e| failed("Could not describe the row order", e))?;
    writer
        .SetInputMediaType(stream, &in_type, None)
        .map_err(|e| failed("Windows could not connect the pictures to the H.264 encoder", e))?;
    writer.BeginWriting().map_err(|e| failed("Windows would not start writing the video", e))?;
    debug_assert_eq!(stream, 0);
    Ok(writer)
}

unsafe fn set_common(t: &IMFMediaType, subtype: &GUID, width: u32, height: u32, fps: u32) -> Result<(), ScreenError> {
    let ctx = |e| failed("Could not describe the video", e);
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(ctx)?;
    t.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(ctx)?;
    t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(ctx)?;
    t.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height)).map_err(ctx)?;
    t.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1)).map_err(ctx)?;
    t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(ctx)?;
    Ok(())
}

struct Mp4Sink {
    writer: Option<IMFSinkWriter>,
    stream: u32,
    width: u32,
    height: u32,
    note: Option<String>,
}

impl VideoSink for Mp4Sink {
    fn note(&self) -> Option<String> {
        self.note.clone()
    }

    fn write(&mut self, frame: &Frame, pts_100ns: i64, duration_100ns: i64) -> Result<(), ScreenError> {
        let writer =
            self.writer.as_ref().ok_or_else(|| ScreenError::Failed("The video file is already closed.".into()))?;
        if frame.width != self.width || frame.height != self.height {
            return Err(ScreenError::Failed(format!(
                "The screen changed size from {} by {} to {} by {} while recording, which is not handled yet.",
                self.width, self.height, frame.width, frame.height
            )));
        }
        let length = frame.bgra.len() as u32;
        unsafe {
            let buffer = MFCreateMemoryBuffer(length).map_err(|e| failed("Could not make room for a picture", e))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(|e| failed("Could not fill a picture", e))?;
            std::ptr::copy_nonoverlapping(frame.bgra.as_ptr(), data, frame.bgra.len());
            buffer.Unlock().map_err(|e| failed("Could not finish filling a picture", e))?;
            buffer.SetCurrentLength(length).map_err(|e| failed("Could not size a picture", e))?;

            let sample = MFCreateSample().map_err(|e| failed("Could not make a video sample", e))?;
            sample.AddBuffer(&buffer).map_err(|e| failed("Could not attach a picture", e))?;
            sample.SetSampleTime(pts_100ns).map_err(|e| failed("Could not time a picture", e))?;
            sample.SetSampleDuration(duration_100ns).map_err(|e| failed("Could not time a picture", e))?;
            writer
                .WriteSample(self.stream, &sample)
                .map_err(|e| failed("Windows would not take a picture for the video", e))?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), ScreenError> {
        let writer =
            self.writer.take().ok_or_else(|| ScreenError::Failed("The video file is already closed.".into()))?;
        let result = unsafe { writer.Finalize() }.map_err(|e| failed("Windows could not complete the video file", e));
        drop(writer);
        unsafe {
            let _ = MFShutdown();
        }
        result
    }
}

// Silence an unused-import warning on the trait that makes `.cast()` and friends available on COM objects.
#[allow(dead_code)]
fn _uses_interface<T: Interface>(_: &T) {}
