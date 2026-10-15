// SPDX-License-Identifier: Apache-2.0
//! Saying in words that the program stopped suddenly, and why Windows says it did.
//!
//! When a program is stopped by Windows itself (it touched memory it should not, for example) nothing of its own can
//! run, so by default it vanishes without a word. On Windows the program asks to be told first
//! (`win_console::install_crash_report`) and uses these words, so that a test log says what happened and where.

/// What a Windows exception code means, in plain words.
#[cfg_attr(not(windows), allow(dead_code))] // only the Windows build installs the report
pub fn describe_exception(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "the program used memory it must not (an access violation)",
        0xC000_00FD => "the program ran out of room to keep track of what it was doing (a stack overflow)",
        0xC000_0094 | 0xC000_0095 => "the program divided a number by zero",
        0xC000_001D | 0xC000_0096 => "the program was told to do something the processor cannot do",
        0xC000_0374 => "the program's memory was found to be damaged (heap corruption)",
        0xC000_0409 => "the program stopped itself after finding its memory damaged (a security check failed)",
        0xC000_0017 | 0xC000_009A => "the computer ran out of memory",
        0xC000_0420 => "the program stopped itself after an assertion failed",
        0xE06D_7363 => "a part of Windows that is written in C++ gave an error it could not recover from",
        0x8000_0003 => "the program reached a stopping point that was left in it",
        _ => "Windows stopped the program",
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
/// The line written when the program stops suddenly. `module` is the file the fault was in, if Windows could say.
pub fn crash_message(code: u32, address: usize, module: Option<&str>) -> String {
    let place = match module {
        Some(name) => format!(" in {name}"),
        None => String::new(),
    };
    format!(
        "THE PROGRAM STOPPED SUDDENLY: {}. Windows error code 0x{code:08X} at address 0x{address:X}{place}. Please send me this line and the log it is in.",
        describe_exception(code)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usual_codes_have_their_own_words_and_unknown_ones_still_say_something() {
        assert!(describe_exception(0xC000_0005).contains("access violation"));
        assert!(describe_exception(0xC000_00FD).contains("stack overflow"));
        assert!(describe_exception(0xC000_0409).contains("security check"));
        assert!(describe_exception(0xC000_0374).contains("heap corruption"));
        assert_eq!(describe_exception(0x1234_5678), "Windows stopped the program");
    }

    #[test]
    fn the_message_gives_the_code_the_address_and_the_file_in_one_line() {
        let m = crash_message(0xC000_0005, 0x7FF6_1234_ABCD, Some("nvwgf2umx.dll"));
        assert_eq!(
            m,
            "THE PROGRAM STOPPED SUDDENLY: the program used memory it must not (an access violation). Windows error code 0xC0000005 at address 0x7FF61234ABCD in nvwgf2umx.dll. Please send me this line and the log it is in."
        );
        assert!(!m.contains('\n'));
        assert!(!crash_message(0xC000_0005, 0, None).contains(" in "), "no file, no place");
        assert!(!m.contains('\u{2014}'));
    }
}
