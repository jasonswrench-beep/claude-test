// SPDX-License-Identifier: Apache-2.0
//! Finding the windows that can be recorded. Windows only.
//!
//! The list is meant to match what a person sees as their open windows (what Alt+Tab shows): visible, not
//! minimized, not hidden on another virtual desktop, with a title, not a tool window, and not owned by
//! another window. The program's own windows (the region picker and the frame round a region) are left
//! out. It has been type-checked for Windows and never run: treat it as unproven.

use crate::window::{number_windows, WindowInfo};
use crate::ScreenError;
use std::ffi::c_void;
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

/// The windows that can be recorded, numbered from 1, the one in front first.
pub(crate) fn list_windows() -> Result<Vec<WindowInfo>, ScreenError> {
    crate::win::prepare();
    let mut found: Vec<WindowInfo> = Vec::new();
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
        let found = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
        if let Some(window) = unsafe { describe(hwnd) } {
            found.push(window);
        }
        BOOL(1)
    }
    unsafe { EnumWindows(Some(each), LPARAM(&mut found as *mut Vec<WindowInfo> as isize)) }
        .map_err(|e| ScreenError::Failed(format!("Windows would not list the open windows: {e}")))?;
    Ok(number_windows(found))
}

/// What to say about a window, or `None` if it is not one a person would call an open window.
unsafe fn describe(hwnd: HWND) -> Option<WindowInfo> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        // Hidden on another virtual desktop, or a suspended app: visible to Windows but not to the person.
        let mut cloaked = 0u32;
        let asked = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        );
        if asked.is_ok() && cloaked != 0 {
            return None;
        }
        // The same rule as Alt+Tab: no tool windows, and nothing that belongs to another window.
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let tool = style & WS_EX_TOOLWINDOW.0 != 0 && style & WS_EX_APPWINDOW.0 == 0;
        let owned = GetWindow(hwnd, GW_OWNER).map(|owner| !owner.0.is_null()).unwrap_or(false);
        if tool || (owned && style & WS_EX_APPWINDOW.0 == 0) {
            return None;
        }
        let mut title = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        if length == 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&title[..length.min(title.len())]);
        if title == "Program Manager" {
            return None; // the desktop itself
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == GetCurrentProcessId() {
            return None; // this program's own windows
        }
        // The size a person sees: without the invisible borders Windows puts round most windows.
        let mut bounds = RECT::default();
        let measured = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut bounds as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
            || GetWindowRect(hwnd, &mut bounds).is_ok();
        let (width, height) = if measured {
            ((bounds.right - bounds.left).max(0) as u32, (bounds.bottom - bounds.top).max(0) as u32)
        } else {
            (0, 0)
        };
        if width == 0 || height == 0 {
            return None;
        }
        Some(WindowInfo { number: 0, handle: hwnd.0 as isize, title, process: program_name(process_id), width, height })
    }
}

/// The name of the program a process runs, such as `notepad.exe`.
unsafe fn program_name(process_id: u32) -> String {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) else {
            return "an unknown program".to_string();
        };
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return "an unknown program".to_string();
        }
        let path = String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]);
        path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string()
    }
}
