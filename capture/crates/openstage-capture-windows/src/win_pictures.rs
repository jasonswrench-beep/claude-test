// SPDX-License-Identifier: Apache-2.0
//! The Windows side of exporting: reading the frames of recorded videos, and the images an edit imports.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner runs
//! `TEST EXPORT.bat`, it has never been run. Treat every line of it as unproven.
//!
//! A recorded video is opened with Media Foundation's source reader, which decodes it and turns every picture into
//! plain blue-green-red pixels. Exporting asks for pictures mostly in order, so the reader is kept where it was and
//! read forward; only a jump backwards, or a long way forwards (a cut to another part of the recording), makes it
//! seek. A seek lands on a keyframe at or before the moment, and the reader then reads forward to the right picture,
//! so the picture shown is always the one that was current at the moment asked for. Past the end of a video its
//! last picture is held.

use crate::export::source_path;
use crate::video::VideoPlatform;
use crate::win_camera::{copy_picture, layout_of, Layout};
use crate::win_video::WindowsVideo;
use openstage_capture_project::{EditSourceKind, EditSourceRef};
use openstage_capture_render::{Layer, Picture, PictureSource};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::core::{GUID, HSTRING};
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaType, IMFSourceReader, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
    MFMediaType_Video, MFVideoFormat_RGB32, MFVideoInterlace_Progressive, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VT_I8;

/// How far past the picture being held the wanted moment may be before the reader seeks instead of reading on, in
/// units of 100 nanoseconds. Reading forward costs a decode of every picture on the way.
const SEEK_IF_FURTHER_THAN: i64 = 30_000_000;

fn failed(what: &str, e: windows::core::Error) -> String {
    format!("{what} (Windows error code 0x{:08X}: {})", e.code().0 as u32, e.message())
}

/// A picture and when, in the video's own time, it is shown from.
struct Held {
    at: i64,
    picture: Rc<Picture>,
}

/// One recorded video, open for reading its pictures in any order (mostly in order).
struct VideoFile {
    path: PathBuf,
    reader: IMFSourceReader,
    layout: Layout,
    /// The newest picture shown at or before the moment last asked for.
    current: Option<Held>,
    /// The first picture after it, already read.
    ahead: Option<Held>,
    ended: bool,
}

impl VideoFile {
    fn open(path: &Path) -> Result<VideoFile, String> {
        unsafe {
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1).map_err(|e| failed("Could not prepare to read a recording", e))?;
            let attributes = attributes.ok_or("Could not prepare to read a recording.")?;
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(|e| failed("Could not ask Windows to convert the recording's pictures", e))?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attributes).map_err(|e| {
                failed(&format!("The recording {} could not be opened. Is it still there?", path.display()), e)
            })?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            // Only the pictures: the sound inside the video is not used (the edit mixes the separate sound files).
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(|e| failed("Windows would not select the recording's picture", e))?;
            reader
                .SetStreamSelection(stream, true)
                .map_err(|e| failed("Windows would not select the recording's picture", e))?;
            let native = reader
                .GetNativeMediaType(stream, 0)
                .map_err(|e| failed(&format!("The recording {} has no picture in it", path.display()), e))?;
            let packed = native
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|e| failed("Windows would not say how big the recording's pictures are", e))?;
            let (width, height) = ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32);
            let plain: IMFMediaType =
                MFCreateMediaType().map_err(|e| failed("Could not describe the pictures wanted", e))?;
            let describe = |e| failed("Could not describe the pictures wanted", e);
            plain.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(describe)?;
            plain.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(describe)?;
            plain.SetUINT64(&MF_MT_FRAME_SIZE, packed).map_err(describe)?;
            plain.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(describe)?;
            // Rows from the top down.
            plain.SetUINT32(&MF_MT_DEFAULT_STRIDE, width * 4).map_err(describe)?;
            reader
                .SetCurrentMediaType(stream, None, &plain)
                .map_err(|e| failed("Windows could not turn the recording's pictures into plain pixels", e))?;
            let layout = layout_of(&reader).map_err(|e| e.to_string())?;
            eprintln!("export: opened the recording {}: {width} by {height}", path.display());
            Ok(VideoFile { path: path.to_path_buf(), reader, layout, current: None, ahead: None, ended: false })
        }
    }

    fn seek(&mut self, to: i64) -> Result<(), String> {
        unsafe {
            let mut position = PROPVARIANT::default();
            let inner = &mut *position.Anonymous.Anonymous;
            inner.vt = VT_I8;
            inner.Anonymous.hVal = to.max(0);
            self.reader
                .SetCurrentPosition(&GUID::zeroed(), &position)
                .map_err(|e| failed(&format!("The recording {} could not be searched", self.path.display()), e))?;
        }
        self.current = None;
        self.ahead = None;
        self.ended = false;
        Ok(())
    }

    /// The next picture of the video, or `None` at its end.
    fn read_next(&mut self) -> Result<Option<Held>, String> {
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        loop {
            let (mut flags, mut at, mut sample) = (0u32, 0i64, None);
            unsafe { self.reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut at), Some(&mut sample)) }
                .map_err(|e| {
                    failed(&format!("The recording {} could not be read any further", self.path.display()), e)
                })?;
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                self.layout = unsafe { layout_of(&self.reader) }.map_err(|e| e.to_string())?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                self.ended = true;
                return Ok(None);
            }
            let Some(sample) = sample else { continue };
            let mut bgra = vec![0u8; self.layout.width as usize * self.layout.height as usize * 4];
            if !unsafe { copy_picture(&sample, self.layout, &mut bgra) } {
                continue;
            }
            let picture = Picture::solid_pixels(self.layout.width, self.layout.height, bgra);
            return Ok(Some(Held { at, picture: Rc::new(picture) }));
        }
    }

    /// The picture that is shown at `time_ms` of the video.
    fn at(&mut self, time_ms: u64) -> Result<Rc<Picture>, String> {
        let wanted = time_ms as i64 * 10_000;
        let behind = self.current.as_ref().is_some_and(|c| c.at > wanted);
        let far = match (&self.ahead, &self.current) {
            (Some(a), _) => wanted - a.at > SEEK_IF_FURTHER_THAN,
            (None, Some(c)) => wanted - c.at > SEEK_IF_FURTHER_THAN,
            (None, None) => false,
        };
        if behind || far {
            self.seek(wanted)?;
        }
        loop {
            match self.ahead.take() {
                Some(next) if next.at <= wanted => {
                    self.current = Some(next);
                }
                Some(next) => {
                    self.ahead = Some(next);
                    break;
                }
                None => {
                    if self.ended {
                        break;
                    }
                    match self.read_next()? {
                        Some(next) => self.ahead = Some(next),
                        None => break,
                    }
                }
            }
        }
        // Before the first picture, the first picture; past the last, the last.
        if self.current.is_none() {
            self.current = self.ahead.take();
        }
        self.current
            .as_ref()
            .map(|c| Rc::clone(&c.picture))
            .ok_or_else(|| format!("The recording {} has no pictures in it.", self.path.display()))
    }
}

/// Decode an image file (PNG, JPEG and the others Windows knows) into blue-green-red pixels with how solid each is.
fn load_image(path: &Path) -> Result<Picture, String> {
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| failed("Windows' picture reader would not start", e))?;
        let decoder = factory
            .CreateDecoderFromFilename(
                &HSTRING::from(path.as_os_str()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(|e| {
                failed(
                    &format!("The picture {} could not be opened. Is it a picture Windows can read?", path.display()),
                    e,
                )
            })?;
        let frame = decoder.GetFrame(0).map_err(|e| failed("The picture has nothing in it", e))?;
        let converter =
            factory.CreateFormatConverter().map_err(|e| failed("Windows' picture converter would not start", e))?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|e| failed("Windows could not turn the picture into plain pixels", e))?;
        let (mut width, mut height) = (0u32, 0u32);
        converter
            .GetSize(&mut width, &mut height)
            .map_err(|e| failed("Windows would not say how big the picture is", e))?;
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        converter
            .CopyPixels(std::ptr::null(), width * 4, &mut bgra)
            .map_err(|e| failed("Windows could not read the picture's pixels", e))?;
        Ok(Picture::new(width, height, bgra))
    }
}

/// The pictures of recordings and course images, for the exporter.
pub struct WindowsPictures {
    project: PathBuf,
    assets: Option<PathBuf>,
    videos: HashMap<EditSourceRef, VideoFile>,
    images: HashMap<EditSourceRef, Rc<Picture>>,
}

impl WindowsPictures {
    pub fn new(project: &Path, assets: Option<&Path>) -> Result<Self, String> {
        WindowsVideo.start_media().map_err(|e| format!("Windows' video tools would not start: {e}"))?;
        Ok(Self {
            project: project.to_path_buf(),
            assets: assets.map(Path::to_path_buf),
            videos: HashMap::new(),
            images: HashMap::new(),
        })
    }
}

impl PictureSource for WindowsPictures {
    fn picture(&mut self, layer: &Layer) -> Result<Rc<Picture>, String> {
        if layer.kind == EditSourceKind::Image {
            if let Some(found) = self.images.get(&layer.source) {
                return Ok(Rc::clone(found));
            }
            let path = source_path(&self.project, self.assets.as_deref(), &layer.source)?;
            let picture = Rc::new(load_image(&path)?);
            self.images.insert(layer.source.clone(), Rc::clone(&picture));
            return Ok(picture);
        }
        if !self.videos.contains_key(&layer.source) {
            let path = source_path(&self.project, self.assets.as_deref(), &layer.source)?;
            self.videos.insert(layer.source.clone(), VideoFile::open(&path)?);
        }
        self.videos.get_mut(&layer.source).expect("just opened").at(layer.source_time_ms)
    }
}
