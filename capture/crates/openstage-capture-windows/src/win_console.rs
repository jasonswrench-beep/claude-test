// SPDX-License-Identifier: Apache-2.0
//! Ctrl+C (and closing the console window) as a request to stop what is being done, not a kill.
//!
//! The exporter asks [`cancel_requested`] before every frame, so a stopped export deletes its half-made file and says
//! so. This file only builds on Windows; elsewhere the same functions exist and never report a request.

use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL: AtomicBool = AtomicBool::new(false);

/// Whether the author has asked to stop (pressed Ctrl+C, or Ctrl+Break) since [`install`] was called.
pub fn cancel_requested() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

/// From now on, Ctrl+C and Ctrl+Break set the flag instead of ending the program at once.
#[cfg(windows)]
pub fn install() {
    use windows::core::BOOL;
    use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT};
    unsafe extern "system" fn handler(kind: u32) -> BOOL {
        if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
            CANCEL.store(true, Ordering::SeqCst);
            return BOOL(1);
        }
        BOOL(0)
    }
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handler), true);
    }
}

#[cfg(not(windows))]
pub fn install() {}

/// From now on, if Windows stops the program suddenly (it used memory it must not, for example), one line saying
/// so, with Windows' error code and the file the fault was in, is written to standard error before the program ends.
/// Without this such a stop is silent: the program just vanishes, and a log shows nothing.
#[cfg(windows)]
pub fn install_crash_report() {
    use crate::crash_words::crash_message;
    use std::io::Write;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::Diagnostics::Debug::{SetUnhandledExceptionFilter, EXCEPTION_POINTERS};
    use windows::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };

    unsafe extern "system" fn report(info: *const EXCEPTION_POINTERS) -> i32 {
        let (mut code, mut address) = (0u32, 0usize);
        if !info.is_null() {
            let record = unsafe { (*info).ExceptionRecord };
            if !record.is_null() {
                code = unsafe { (*record).ExceptionCode.0 } as u32;
                address = unsafe { (*record).ExceptionAddress } as usize;
            }
        }
        let mut module_name = None;
        let mut module = HMODULE::default();
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(address as *const u16),
                &mut module,
            )
        };
        if found.is_ok() {
            let mut name = [0u16; 260];
            let length = unsafe { GetModuleFileNameW(Some(module), &mut name) } as usize;
            if length > 0 {
                let path = String::from_utf16_lossy(&name[..length.min(name.len())]);
                module_name = Some(path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string());
            }
        }
        let mut err = std::io::stderr();
        let _ = writeln!(err, "{}", crash_message(code, address, module_name.as_deref()));
        let _ = err.flush();
        // Let Windows end the program now.
        1
    }
    unsafe {
        let _ = SetUnhandledExceptionFilter(Some(report));
    }
}

#[cfg(not(windows))]
pub fn install_crash_report() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_asked_for_until_someone_asks() {
        install();
        assert!(!cancel_requested());
    }
}
