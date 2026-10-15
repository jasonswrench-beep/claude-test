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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_asked_for_until_someone_asks() {
        install();
        assert!(!cancel_requested());
    }
}
