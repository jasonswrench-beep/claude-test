// SPDX-License-Identifier: Apache-2.0
//! The thin frame shown around a region on the screen while it is being recorded. Windows only.
//!
//! The author should be able to see what is being kept (story A1). The frame is drawn just outside the
//! region, so it is not in the part that is recorded, and the window is also marked "leave me out of screen
//! captures" (Windows 10 version 2004 and later), so it is not in the full picture Windows hands over
//! either. Mouse clicks go straight through it.
//!
//! Unproven until the owner runs it: written and type-checked without a Windows computer.

use crate::region_paint::{recording_frame, Canvas};
use crate::win_layered::{dpi_scale, register_class, Surface};
use crate::{DisplayInfo, Region};
use std::ffi::c_void;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
    SetWindowDisplayAffinity, ShowWindow, MSG, SW_SHOWNOACTIVATE, WDA_EXCLUDEFROMCAPTURE, WM_CLOSE, WM_DESTROY,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// The frame on the screen. Dropping it takes the frame away.
pub struct FrameWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl FrameWindow {
    /// Show the frame around `region` of `display`. `Ok(None)` when there is nothing to show (the region
    /// is the whole display).
    pub fn show(display: &DisplayInfo, region: &Region) -> Result<Option<FrameWindow>, String> {
        let scale = dpi_scale(display.handle);
        let Some(canvas) = recording_frame(region, display.width, display.height, scale) else {
            return Ok(None);
        };
        let origin = (display.x, display.y);
        let (tx, rx) = mpsc::channel();
        // The window lives on a thread of its own, because a window needs a thread that handles its messages.
        let thread = std::thread::spawn(move || run(canvas, origin, tx));
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(hwnd)) => Ok(Some(FrameWindow { hwnd, thread: Some(thread) })),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("The frame around the region did not appear in time.".to_string()),
        }
    }
}

impl Drop for FrameWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut c_void)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

unsafe extern "system" fn frame_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn run(canvas: Canvas, origin: (i32, i32), ready: mpsc::Sender<Result<isize, String>>) {
    crate::win::prepare();
    let surface_and_window = make_window(&canvas, origin);
    let (hwnd, _surface) = match surface_and_window {
        Ok(made) => made,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(hwnd.0 as isize));
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            DispatchMessageW(&message);
        }
    }
}

fn make_window(canvas: &Canvas, origin: (i32, i32)) -> Result<(HWND, Surface), String> {
    register_class(w!("OpenStageCaptureRegionFrame"), Some(frame_proc))?;
    unsafe {
        let hinstance =
            GetModuleHandleW(None).map_err(|e| format!("Windows would not say which program this is: {e}"))?;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("OpenStageCaptureRegionFrame"),
            w!(""),
            WS_POPUP,
            origin.0 + canvas.left,
            origin.1 + canvas.top,
            canvas.width as i32,
            canvas.height as i32,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Windows would not make the frame around the region: {e}"))?;
        // Keep the frame out of screen captures. Older versions of Windows refuse; the frame is outside the
        // region anyway, so it is still not in the video.
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let surface = Surface::new(hwnd, canvas.width, canvas.height)?;
        surface.show(canvas, origin.0 + canvas.left, origin.1 + canvas.top)?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok((hwnd, surface))
    }
}
