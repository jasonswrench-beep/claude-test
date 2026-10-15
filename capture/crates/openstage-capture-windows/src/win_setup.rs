// SPDX-License-Identifier: Apache-2.0
//! The Windows side of the first-run check: the privacy settings and the free disk space.
//!
//! This file only builds on Windows. It has been type-checked for Windows but never run; treat every
//! line of it as unproven until the owner has run `CHECK MY COMPUTER.bat`.

use crate::setup::{Access, SystemFacts};
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

pub struct WindowsFacts;

/// The text value `Value` under one of Windows' privacy keys, or `None` if it is not there.
fn consent(subkey: &str) -> Option<String> {
    let path = HSTRING::from(format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{subkey}"
    ));
    let mut buffer = [0u16; 32];
    let mut bytes = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &path,
            PCWSTR::from_raw(windows::core::w!("Value").as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let units = (bytes as usize / 2).saturating_sub(1).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..units]))
}

impl SystemFacts for WindowsFacts {
    fn microphone_access(&self) -> Access {
        let overall = consent("microphone");
        let desktop = consent("microphone\\NonPackaged");
        if overall.as_deref() == Some("Deny") {
            return Access::Blocked(
                "Microphone access is turned off for this account. Open Settings, Privacy and security, Microphone, and turn on Microphone access.".into(),
            );
        }
        if desktop.as_deref() == Some("Deny") {
            return Access::Blocked(
                "Desktop programs are not allowed to use the microphone. Open Settings, Privacy and security, Microphone, and turn on Let desktop apps access your microphone.".into(),
            );
        }
        if overall.is_none() && desktop.is_none() {
            Access::Unknown
        } else {
            Access::Allowed
        }
    }

    fn camera_access(&self) -> Access {
        let overall = consent("webcam");
        let desktop = consent("webcam\\NonPackaged");
        if overall.as_deref() == Some("Deny") {
            return Access::Blocked(
                "Camera access is turned off for this account. Open Settings, Privacy and security, Camera, and turn on Camera access.".into(),
            );
        }
        if desktop.as_deref() == Some("Deny") {
            return Access::Blocked(
                "Desktop programs are not allowed to use the camera. Open Settings, Privacy and security, Camera, and turn on Let desktop apps access your camera.".into(),
            );
        }
        if overall.is_none() && desktop.is_none() {
            Access::Unknown
        } else {
            Access::Allowed
        }
    }

    fn free_disk_bytes(&self, dir: &Path) -> Option<u64> {
        // Ask about the folder if it exists, otherwise the nearest folder above it that does.
        let mut folder = dir.to_path_buf();
        if folder.as_os_str().is_empty() {
            folder = std::env::current_dir().ok()?;
        }
        while !folder.exists() {
            if !folder.pop() {
                return None;
            }
        }
        let mut free = 0u64;
        unsafe { GetDiskFreeSpaceExW(&HSTRING::from(folder.as_os_str()), Some(&mut free), None, None) }.ok()?;
        Some(free)
    }
}
