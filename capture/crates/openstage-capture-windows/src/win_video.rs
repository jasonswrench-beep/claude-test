// SPDX-License-Identifier: Apache-2.0
//! The Windows side of the video test: Media Foundation's sink writer makes an H.264 MP4.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner
//! runs `TEST VIDEO CAPTURE.bat`, it has never been run. Treat every line of it as unproven.

use crate::video::{AudioInspection, AudioSpec, EncoderInfo, SinkSpec, VideoInspection, VideoPlatform, VideoSink};
use crate::{Frame, ScreenError};
use std::path::Path;
use std::sync::Once;
use std::time::Duration;

use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_High, IMFActivate, IMFAttributes, IMFMediaType, IMFSinkWriter, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Video, MFStartup,
    MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264, MFVideoFormat_RGB32, MFVideoInterlace_Progressive,
    MFSTARTUP_FULL, MFT_CATEGORY_AUDIO_ENCODER, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT, MFT_REGISTER_TYPE_INFO, MF_MT_AVG_BITRATE,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_VERSION,
};
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateSourceReaderFromURL, MFMediaType_Audio,
    MFTranscodeContainerType_FMPEG4, MF_E_INVALIDSTREAMNUMBER, MF_MPEG4SINK_MIN_FRAGMENT_DURATION,
    MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_MAX_KEYFRAME_SPACING, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_TRANSCODE_CONTAINERTYPE,
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
        crate::win::prepare();
        let mut found = enumerate(true)?;
        found.extend(enumerate(false)?);
        Ok(found)
    }

    fn aac_encoder_available(&self) -> Result<bool, ScreenError> {
        crate::win::prepare();
        let wanted = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Audio, guidSubtype: MFAudioFormat_AAC };
        Ok(!enumerate_mfts(MFT_CATEGORY_AUDIO_ENCODER, &wanted, MFT_ENUM_FLAG_SYNCMFT, false, "sound")?.is_empty())
    }

    fn create_sink(&self, spec: &SinkSpec) -> Result<Box<dyn VideoSink>, ScreenError> {
        crate::win::prepare();
        let (width, height) = (spec.width, spec.height);
        let sink = |(writer, audio_stream): (IMFSinkWriter, Option<u32>), note| {
            Box::new(Mp4Sink {
                writer: Some(writer),
                stream: 0,
                audio: spec.audio.clone().zip(audio_stream),
                width,
                height,
                note,
            }) as Box<dyn VideoSink>
        };
        // First let Windows use the graphics card's encoder. If that setup is refused, try the processor's.
        match unsafe { build_writer(spec, true) } {
            Ok(built) => Ok(sink(built, None)),
            Err(first) => {
                let built = unsafe { build_writer(spec, false) }.map_err(|second| {
                    ScreenError::Failed(format!("{first} Trying the processor's encoder instead also failed: {second}"))
                })?;
                Ok(sink(
                    built,
                    Some(format!(
                        "The graphics card's encoder could not be used ({first}), so the processor's encoder was used instead."
                    )),
                ))
            }
        }
    }

    fn inspect_video(&self, path: &Path) -> Result<VideoInspection, ScreenError> {
        // This is also called on its own (by the crash test), so start Windows' video tools here too.
        self.start_media()?;
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
        let audio = unsafe { inspect_sound(&reader) }?;
        // The pictures' size is packed into one number: the width in the top half, the height in the bottom half.
        let size = unsafe { reader.GetCurrentMediaType(stream) }
            .ok()
            .and_then(|t| unsafe { t.GetUINT64(&MF_MT_FRAME_SIZE) }.ok())
            .map(|packed| ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32));
        Ok(VideoInspection { frames, duration_ms: (end_100ns.max(0) / 10_000) as u64, audio, size })
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

    fn process_memory_mb(&self) -> Option<f64> {
        use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        let mut counters =
            PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) }.ok()?;
        Some(counters.WorkingSetSize as f64 / (1024.0 * 1024.0))
    }
}

/// List the H.264 encoders: the ones that run on the graphics card, or the ones that run on the processor.
fn enumerate(hardware: bool) -> Result<Vec<EncoderInfo>, ScreenError> {
    let wanted = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let flags = if hardware { MFT_ENUM_FLAG_HARDWARE } else { MFT_ENUM_FLAG_SYNCMFT };
    enumerate_mfts(MFT_CATEGORY_VIDEO_ENCODER, &wanted, flags, hardware, "video")
}

/// List Media Foundation's transforms of one category that produce the wanted kind of output.
fn enumerate_mfts(
    category: GUID,
    wanted: &MFT_REGISTER_TYPE_INFO,
    flags: MFT_ENUM_FLAG,
    hardware: bool,
    what: &str,
) -> Result<Vec<EncoderInfo>, ScreenError> {
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;
    unsafe {
        MFTEnumEx(category, flags | MFT_ENUM_FLAG_SORTANDFILTER, None, Some(wanted), &mut list, &mut count)
            .map_err(|e| failed(&format!("Windows would not list its {what} encoders"), e))?;
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

unsafe fn build_writer(spec: &SinkSpec, allow_hardware: bool) -> Result<(IMFSinkWriter, Option<u32>), ScreenError> {
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
    debug_assert_eq!(stream, 0);

    // The sound track, if there is one: AAC in the file, plain 16-bit sound handed in.
    let audio_stream = match &spec.audio {
        Some(audio) => Some(add_sound_track(&writer, audio)?),
        None => None,
    };
    writer.BeginWriting().map_err(|e| failed("Windows would not start writing the video", e))?;
    Ok((writer, audio_stream))
}

unsafe fn add_sound_track(writer: &IMFSinkWriter, audio: &AudioSpec) -> Result<u32, ScreenError> {
    let ctx = |e| failed("Could not describe the sound", e);
    let channels = audio.channels as u32;

    // What goes into the file: AAC, the sound every player can play.
    let out_type: IMFMediaType = MFCreateMediaType().map_err(ctx)?;
    out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(ctx)?;
    out_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(ctx)?;
    out_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(ctx)?;
    out_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio.sample_rate).map_err(ctx)?;
    out_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels).map_err(ctx)?;
    out_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, audio.bitrate_kbps * 1000 / 8).map_err(ctx)?;
    out_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 1).map_err(ctx)?;
    // AAC Low Complexity, level 2: the common setting every player understands.
    out_type.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29).map_err(ctx)?;
    let stream = writer.AddStream(&out_type).map_err(|e| failed("Windows would not accept the AAC sound format", e))?;

    // What the program hands in: plain 16-bit sound.
    let in_type: IMFMediaType = MFCreateMediaType().map_err(ctx)?;
    in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(ctx)?;
    in_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio.sample_rate).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, audio.sample_rate * channels * 2).map_err(ctx)?;
    in_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1).map_err(ctx)?;
    writer
        .SetInputMediaType(stream, &in_type, None)
        .map_err(|e| failed("Windows could not connect the sound to the AAC encoder", e))?;
    Ok(stream)
}

/// Read a file's sound track back without decoding it. `None` if the file has no sound track.
unsafe fn inspect_sound(reader: &IMFSourceReader) -> Result<Option<AudioInspection>, ScreenError> {
    let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    // A file with no sound track has no first audio stream.
    if reader.GetNativeMediaType(stream, 0).is_err() {
        return Ok(None);
    }
    let (mut samples, mut end_100ns) = (0u64, 0i64);
    loop {
        let mut flags = 0u32;
        let mut timestamp = 0i64;
        let mut sample = None;
        match reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut timestamp), Some(&mut sample)) {
            Ok(()) => {}
            Err(e) if e.code() == MF_E_INVALIDSTREAMNUMBER => return Ok(None),
            Err(e) => return Err(failed("Windows could not read the sound track all the way through", e)),
        }
        if let Some(sample) = sample {
            samples += 1;
            let duration = sample.GetSampleDuration().unwrap_or(0);
            end_100ns = end_100ns.max(timestamp + duration);
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    Ok(Some(AudioInspection { samples, duration_ms: (end_100ns.max(0) / 10_000) as u64 }))
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
    /// The sound track's stream number and format, if the file has one.
    audio: Option<(AudioSpec, u32)>,
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

    fn write_audio(&mut self, samples: &[i16], pts_100ns: i64) -> Result<(), ScreenError> {
        let writer =
            self.writer.as_ref().ok_or_else(|| ScreenError::Failed("The video file is already closed.".into()))?;
        let (spec, stream) = self
            .audio
            .as_ref()
            .ok_or_else(|| ScreenError::Failed("This video file was made without a sound track.".into()))?;
        let bytes = samples.len() * 2;
        let frames = samples.len() as i64 / spec.channels as i64;
        let duration = frames * 10_000_000 / spec.sample_rate as i64;
        unsafe {
            let buffer = MFCreateMemoryBuffer(bytes as u32).map_err(|e| failed("Could not make room for sound", e))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(|e| failed("Could not fill a piece of sound", e))?;
            std::ptr::copy_nonoverlapping(samples.as_ptr() as *const u8, data, bytes);
            buffer.Unlock().map_err(|e| failed("Could not finish filling a piece of sound", e))?;
            buffer.SetCurrentLength(bytes as u32).map_err(|e| failed("Could not size a piece of sound", e))?;

            let sample = MFCreateSample().map_err(|e| failed("Could not make a sound sample", e))?;
            sample.AddBuffer(&buffer).map_err(|e| failed("Could not attach sound", e))?;
            sample.SetSampleTime(pts_100ns).map_err(|e| failed("Could not time a piece of sound", e))?;
            sample.SetSampleDuration(duration).map_err(|e| failed("Could not time a piece of sound", e))?;
            writer
                .WriteSample(*stream, &sample)
                .map_err(|e| failed("Windows would not take a piece of sound for the video", e))?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), ScreenError> {
        let writer =
            self.writer.take().ok_or_else(|| ScreenError::Failed("The video file is already closed.".into()))?;
        let result = unsafe { writer.Finalize() }.map_err(|e| failed("Windows could not complete the video file", e));
        // Media Foundation is deliberately left running: reading the finished file back needs it, and
        // Windows tidies it up when the program ends.
        drop(writer);
        result
    }
}

// Silence an unused-import warning on the trait that makes `.cast()` and friends available on COM objects.
#[allow(dead_code)]
fn _uses_interface<T: Interface>(_: &T) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a brand new thread, with nothing prepared in advance, the video tools must still start. This
    /// guards against "CoInitialize has not been called", which the crash test once hit.
    #[test]
    fn windows_video_tools_work_from_a_thread_nobody_prepared() {
        let outcome = std::thread::spawn(|| {
            let video = WindowsVideo;
            let missing = video.inspect_video(Path::new("this-file-does-not-exist.mp4"));
            let encoders = video.h264_encoders();
            (missing, encoders)
        })
        .join()
        .unwrap();

        match outcome.0 {
            Ok(_) => panic!("a missing file cannot have pictures"),
            Err(e) => {
                let text = e.to_string();
                assert!(!text.contains("CoInitialize has not been called"), "the thread was not prepared: {text}");
            }
        }
        // Finding encoders needs the tools started, so it succeeds (with possibly zero encoders) rather than failing to start.
        assert!(outcome.1.is_ok(), "{:?}", outcome.1.err());
    }
}
