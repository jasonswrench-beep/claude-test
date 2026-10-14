// SPDX-License-Identifier: Apache-2.0
//! The Windows side: finding displays and capturing one with Windows Graphics Capture.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner
//! runs `TEST SCREEN CAPTURE.bat`, it has never been run. Treat every line of it as unproven.

use crate::{
    number_displays, CaptureWork, DisplayInfo, Frame, FrameQueue, FrameSource, Platform, ScreenError, WindowsVersion,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use windows::core::{factory, Interface, BOOL};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{HMODULE, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};

/// How many pictures may wait to be taken; beyond that the oldest is dropped for the newest.
const QUEUE_DEPTH: usize = 8;
/// MONITORINFOF_PRIMARY
const MONITOR_IS_PRIMARY: u32 = 1;

fn failed(what: &str, e: windows::core::Error) -> ScreenError {
    ScreenError::Failed(format!("{what} (Windows error code 0x{:08X}: {})", e.code().0 as u32, e.message()))
}

/// Get Windows ready: pictures in real pixels (not scaled by display zoom), and the Windows
/// Runtime started on the calling thread. Safe to call as often as you like, and every entry point
/// that uses Windows' capture or video tools must call it first: Windows' components fail with
/// "CoInitialize has not been called" on a thread that has not been prepared.
pub(crate) fn prepare() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        // This can "fail" harmlessly: the program may already have chosen it.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    });
    // Starting the runtime is per thread, so it is done every time. A second call on the same thread is harmless.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
}

pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
        let mut info =
            OSVERSIONINFOW { dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
        let status = unsafe { RtlGetVersion(&mut info) };
        if status.0 != 0 {
            return Err(ScreenError::Failed(format!(
                "Windows would not say which version it is (code 0x{:08X}).",
                status.0 as u32
            )));
        }
        Ok(WindowsVersion { major: info.dwMajorVersion, minor: info.dwMinorVersion, build: info.dwBuildNumber })
    }

    fn capture_supported(&self) -> bool {
        prepare();
        GraphicsCaptureSession::IsSupported().unwrap_or(false)
    }

    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
        prepare();
        let mut found: Vec<DisplayInfo> = Vec::new();
        unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let found = &mut *(data.0 as *mut Vec<DisplayInfo>);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
                let r = info.monitorInfo.rcMonitor;
                let name_len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
                found.push(DisplayInfo {
                    number: 0,
                    x: r.left,
                    y: r.top,
                    width: (r.right - r.left).max(0) as u32,
                    height: (r.bottom - r.top).max(0) as u32,
                    main: info.monitorInfo.dwFlags & MONITOR_IS_PRIMARY != 0,
                    system_name: String::from_utf16_lossy(&info.szDevice[..name_len]),
                    handle: monitor.0 as isize,
                });
            }
            BOOL(1)
        }
        let ok = unsafe {
            EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut found as *mut Vec<DisplayInfo> as isize))
        };
        if !ok.as_bool() {
            return Err(ScreenError::Failed("Windows would not list the displays.".into()));
        }
        Ok(number_displays(found))
    }

    fn start_display_capture(
        &self,
        display: &DisplayInfo,
        with_cursor: bool,
    ) -> Result<Box<dyn FrameSource>, ScreenError> {
        prepare();
        let source = Capture::start_monitor(HMONITOR(display.handle as *mut c_void), with_cursor)?;
        Ok(Box::new(source))
    }

    fn show_region_outline(&self, display: &DisplayInfo, region: &crate::Region) -> Option<crate::Outline> {
        match crate::win_outline::FrameWindow::show(display, region) {
            Ok(frame) => frame.map(|f| Box::new(f) as crate::Outline),
            Err(e) => {
                // Not seeing the frame does not stop the recording.
                eprintln!("The frame around the region could not be shown (the recording is not affected): {e}");
                None
            }
        }
    }
}

/// What the picture-arrived callback needs, shared with the thread that owns the capture.
struct Shared {
    /// Where the time of reading pictures back went.
    work: WorkCounters,
    device: ID3D11Device,
    /// The device context may only be used by one thread at a time.
    gpu: Mutex<Gpu>,
    queue: FrameQueue,
    first_ticks: Mutex<Option<i64>>,
}

/// Totals kept by the thread that Windows delivers pictures on, read by the recording thread.
#[derive(Default)]
struct WorkCounters {
    pictures: AtomicU64,
    memory_ns: AtomicU64,
    readback_ns: AtomicU64,
    rows_ns: AtomicU64,
}

struct Gpu {
    context: ID3D11DeviceContext,
    staging: Option<(ID3D11Texture2D, u32, u32)>,
}

// The Direct3D objects are free-threaded; the context is guarded by the mutex above.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

struct Capture {
    shared: Arc<Shared>,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    token: i64,
}

impl Capture {
    fn start_monitor(monitor: HMONITOR, with_cursor: bool) -> Result<Self, ScreenError> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|e| failed("Windows' screen capture service could not be reached", e))?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor) }
            .map_err(|e| failed("Windows would not let the program capture that display", e))?;
        Self::start(item, with_cursor)
    }

    fn start(item: GraphicsCaptureItem, with_cursor: bool) -> Result<Self, ScreenError> {
        let (device, context) = create_device()?;
        let dxgi: IDXGIDevice = device.cast().map_err(|e| failed("The graphics device is not usable", e))?;
        let winrt_device: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .and_then(|inspectable| inspectable.cast())
            .map_err(|e| failed("The graphics device could not be shared with Windows' capture service", e))?;

        let size = item.Size().map_err(|e| failed("Windows would not say how large the display is", e))?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            3,
            size,
        )
        .map_err(|e| failed("Could not set up the capture buffers", e))?;
        let session = pool.CreateCaptureSession(&item).map_err(|e| failed("Could not start a capture session", e))?;
        session.SetIsCursorCaptureEnabled(with_cursor).map_err(|e| failed("Could not set the pointer option", e))?;

        let shared = Arc::new(Shared {
            work: WorkCounters::default(),
            device,
            gpu: Mutex::new(Gpu { context, staging: None }),
            queue: FrameQueue::new(QUEUE_DEPTH),
            first_ticks: Mutex::new(None),
        });

        let handler_shared = Arc::clone(&shared);
        let token = pool
            .FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(
                move |pool, _| {
                    if let Some(pool) = pool.as_ref() {
                        if let Ok(frame) = pool.TryGetNextFrame() {
                            // A problem with one picture must not stop the capture; the next one may be fine.
                            let _ = handle_frame(&handler_shared, &frame);
                            // Hand the buffer back to Windows straight away so it can be reused.
                            let _ = frame.Close();
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| failed("Could not listen for pictures", e))?;
        session.StartCapture().map_err(|e| failed("Windows would not start the capture", e))?;
        Ok(Self { shared, pool, session, token })
    }
}

impl FrameSource for Capture {
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

    fn work(&self) -> CaptureWork {
        let w = &self.shared.work;
        CaptureWork {
            pictures: w.pictures.load(Ordering::Relaxed),
            reused: self.shared.queue.reused(),
            memory: Duration::from_nanos(w.memory_ns.load(Ordering::Relaxed)),
            readback: Duration::from_nanos(w.readback_ns.load(Ordering::Relaxed)),
            rows: Duration::from_nanos(w.rows_ns.load(Ordering::Relaxed)),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), ScreenError> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|e| failed("Could not use the graphics card for capture", e))?;
    match (device, context) {
        (Some(d), Some(c)) => {
            // Windows' capture service uses this device from its own threads too, so ask Direct3D
            // to take turns between them. (Microsoft's capture samples do the same.)
            if let Ok(multithread) = c.cast::<ID3D11Multithread>() {
                unsafe {
                    let _ = multithread.SetMultithreadProtected(true);
                }
            }
            Ok((d, c))
        }
        _ => Err(ScreenError::Failed("The graphics card did not give the program what it needed.".into())),
    }
}

/// Copy one captured picture into ordinary memory and queue it.
fn handle_frame(shared: &Shared, frame: &Direct3D11CaptureFrame) -> windows::core::Result<()> {
    let ticks = frame.SystemRelativeTime()?.Duration;
    let arrived_100ns = crate::timing::now_100ns();
    let t_ms = {
        let mut first = shared.first_ticks.lock().unwrap();
        let base = *first.get_or_insert(ticks);
        (ticks.saturating_sub(base) / 10_000).max(0) as u64
    };

    let surface = frame.Surface()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    let (width, height) = (desc.Width, desc.Height);

    let mut gpu = shared.gpu.lock().unwrap();
    let needs_new = !matches!(&gpu.staging, Some((_, w, h)) if *w == width && *h == height);
    if needs_new {
        let mut staging_desc = desc;
        staging_desc.Usage = D3D11_USAGE_STAGING;
        staging_desc.BindFlags = 0;
        staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        staging_desc.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        unsafe { shared.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))? };
        gpu.staging = staging.map(|t| (t, width, height));
    }
    let Some((staging, _, _)) = gpu.staging.clone() else { return Ok(()) };

    // Memory an earlier picture used is used again (all of it is overwritten below), because asking
    // Windows for several megabytes of new memory for every picture costs processor time.
    let memory_started = Instant::now();
    let mut bgra = shared.queue.take_buffer(width as usize * height as usize * 4);
    let memory = memory_started.elapsed();
    let readback_started = Instant::now();
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        gpu.context.CopyResource(&staging, &texture);
        gpu.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
    }
    let readback = readback_started.elapsed();
    let rows_started = Instant::now();
    unsafe {
        let row_bytes = width as usize * 4;
        for y in 0..height as usize {
            let src = (mapped.pData as *const u8).add(y * mapped.RowPitch as usize);
            std::ptr::copy_nonoverlapping(src, bgra.as_mut_ptr().add(y * row_bytes), row_bytes);
        }
        gpu.context.Unmap(&staging, 0);
    }
    let rows = rows_started.elapsed();
    drop(gpu);
    let w = &shared.work;
    w.pictures.fetch_add(1, Ordering::Relaxed);
    w.memory_ns.fetch_add(memory.as_nanos() as u64, Ordering::Relaxed);
    w.readback_ns.fetch_add(readback.as_nanos() as u64, Ordering::Relaxed);
    w.rows_ns.fetch_add(rows.as_nanos() as u64, Ordering::Relaxed);

    shared.queue.push(Frame {
        width,
        height,
        t_ms,
        captured_100ns: Some(ticks),
        arrived_100ns: Some(arrived_100ns),
        bgra,
    });
    Ok(())
}
