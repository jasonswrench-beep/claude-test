// SPDX-License-Identifier: Apache-2.0
//! Playing the cue sounds with Windows' own `PlaySound`.
//!
//! This file only builds on Windows. It has been type-checked for Windows but never run; treat
//! every line of it as unproven until the owner has run `TEST CUES.bat`.
//!
//! Each sound is made once, kept as the bytes of a small WAV file, and played straight from memory.
//! Windows keeps reading those bytes while the sound plays after the call returns, so they are
//! never freed: six tiny files, kept until the program ends.

use openstage_capture::cues::{render, to_wav, Cue, CuePlayer};
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

pub struct WindowsCues {
    sounds: Vec<(Cue, &'static [u8], u64)>,
}

impl WindowsCues {
    pub fn new(volume: u8) -> Self {
        let sounds = Cue::ALL
            .iter()
            .map(|cue| {
                let bytes: &'static [u8] = Box::leak(to_wav(&render(*cue, volume)).into_boxed_slice());
                (*cue, bytes, cue.duration_ms())
            })
            .collect();
        Self { sounds }
    }
}

impl CuePlayer for WindowsCues {
    fn play(&self, cue: Cue) -> u64 {
        let Some((_, bytes, ms)) = self.sounds.iter().find(|(c, _, _)| *c == cue) else { return 0 };
        // SND_MEMORY: the first argument points at a WAV file held in memory. SND_ASYNC: return at once,
        // and let the sound end by itself. SND_NODEFAULT: if it cannot be played, stay quiet rather
        // than play Windows' default beep.
        let played =
            unsafe { PlaySoundW(PCWSTR(bytes.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
        if played.as_bool() {
            *ms
        } else {
            0
        }
    }
}
