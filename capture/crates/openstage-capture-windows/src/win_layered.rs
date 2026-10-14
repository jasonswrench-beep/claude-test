// SPDX-License-Identifier: Apache-2.0
//! What the windows that lay a picture over the screen have in common. Windows only.
//!
//! The picker's dimming and the frame shown while recording are both "layered windows": a window whose
//! contents are a picture with a transparency value for every pixel. This file makes the window class,
//! keeps the picture's memory, and puts a [`Canvas`](crate::Canvas) into the window.
//!
//! Like all of the Windows code in this program, it has been type-checked on Linux but not run until the
//! owner runs it. Treat it as unproven.

use crate::region_paint::Canvas;
use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadCursorW, RegisterClassW, UpdateLayeredWindow, IDC_ARROW, ULW_ALPHA, WNDCLASSW, WNDPROC,
};

/// The display's dots per inch (96 for a normal screen).
pub(crate) fn dpi_for(monitor: isize) -> u32 {
    let (mut x, mut y) = (96u32, 96u32);
    let ok = unsafe { GetDpiForMonitor(HMONITOR(monitor as *mut c_void), MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if ok.is_err() || x == 0 {
        96
    } else {
        x
    }
}

/// How many times 96 dots per inch the display is, rounded: 1 for a normal screen, 2 for a sharp one.
/// Used to keep borders and handles a usable size.
pub(crate) fn dpi_scale(monitor: isize) -> i32 {
    ((dpi_for(monitor) + 48) / 96).max(1) as i32
}

/// Make a window class once. Asking again for a class that exists is fine.
pub(crate) fn register_class(name: PCWSTR, procedure: WNDPROC) -> Result<(), String> {
    let hinstance =
        unsafe { GetModuleHandleW(None) }.map_err(|e| format!("Windows would not say which program this is: {e}"))?;
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.ok();
    let class = WNDCLASSW {
        lpfnWndProc: procedure,
        hInstance: hinstance.into(),
        lpszClassName: name,
        hCursor: cursor.unwrap_or_default(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("Windows would not make a window type (error {}).", error.0));
        }
    }
    Ok(())
}

/// A window's contents: the picture's memory, kept so that it can be changed and shown again and again.
pub(crate) struct Surface {
    hwnd: HWND,
    memory: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    width: i32,
    height: i32,
}

impl Surface {
    /// Memory for a picture `width` by `height` pixels to show in `hwnd`.
    pub(crate) fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, String> {
        let (width, height) = (width as i32, height as i32);
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // A negative height says the rows run from the top down, as in our pixels.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("Windows would not make room for the picture: {e}"))?;
            if bits.is_null() {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(memory);
                return Err("Windows gave no room for the picture.".to_string());
            }
            let old = SelectObject(memory, bitmap.into());
            Ok(Self { hwnd, memory, bitmap, old, bits: bits as *mut u32, width, height })
        }
    }

    /// Show `canvas` (which must be the size this surface was made for) with its top left corner at
    /// (`left`, `top`) on the screen.
    pub(crate) fn show(&self, canvas: &Canvas, left: i32, top: i32) -> Result<(), String> {
        if canvas.width as i32 != self.width || canvas.height as i32 != self.height {
            return Err("The picture is not the size of the window.".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(canvas.pixels.as_ptr(), self.bits, canvas.pixels.len());
            let screen = GetDC(None);
            let position = POINT { x: left, y: top };
            let size = SIZE { cx: self.width, cy: self.height };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let result = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&position),
                Some(&size),
                Some(self.memory),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            result.map_err(|e| format!("Windows would not show the picture over the screen: {e}"))
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.memory);
        }
    }
}
