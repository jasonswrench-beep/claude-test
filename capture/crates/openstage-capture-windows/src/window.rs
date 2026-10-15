// SPDX-License-Identifier: Apache-2.0
//! Recording one window instead of a whole display (story A1 in `docs/prd.md`).
//!
//! The author picks a window from a numbered list that is written out in words (so it works with a screen
//! reader and without a mouse), by number or by part of its title. Windows' screen capture then delivers
//! just that window, whatever is on top of it, and nothing else on the screen: no notification pop-ups,
//! no other programs. This file holds what can be checked without Windows: the list, choosing from it, the
//! size of the video, and keeping that size when the window is resized during the recording.
//!
//! Choices that could go either way, to be confirmed by the owner:
//!
//! * The video keeps the size the window had when the recording began. If the window is resized, its picture
//!   is made smaller to fit (never larger, so text stays sharp) and centred on black.
//! * A window that is minimized shows its last picture until it is restored.
//! * A window that is closed ends the recording at once, keeping everything recorded until then.

use crate::region::MIN_SIDE;
use crate::{CaptureWork, Frame, FrameSource};
use std::time::Duration;

/// One window that can be recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// Its place in the list, from 1.
    pub number: u32,
    /// Windows' own number for it (needed to capture it; not shown to the author).
    pub handle: isize,
    pub title: String,
    /// The program it belongs to, such as `notepad.exe`.
    pub process: String,
    /// Its size on the screen in pixels, as far as Windows says.
    pub width: u32,
    pub height: u32,
}

impl WindowInfo {
    /// For the author to read or hear, with the number to choose it by.
    pub fn describe(&self) -> String {
        format!("{}. {}: {} ({} by {})", self.number, self.process, self.title, self.width, self.height)
    }
}

/// Number a list of windows from 1 in the order given.
pub fn number_windows(mut windows: Vec<WindowInfo>) -> Vec<WindowInfo> {
    for (i, window) in windows.iter_mut().enumerate() {
        window.number = i as u32 + 1;
    }
    windows
}

fn list_text(windows: &[WindowInfo]) -> String {
    windows.iter().map(WindowInfo::describe).collect::<Vec<_>>().join("; ")
}

/// Choose a window by its number, or by part of its title or program name (capital letters do not matter).
/// Says in words what is wrong if there is none, or more than one, and lists the ones that matched.
pub fn choose_window<'a>(windows: &'a [WindowInfo], choice: &str) -> Result<&'a WindowInfo, String> {
    let choice = choice.trim();
    if windows.is_empty() {
        return Err("There are no open windows that can be recorded. Windows that are minimized, hidden or have no title are not listed.".to_string());
    }
    if choice.is_empty() {
        return Err(format!(
            "Say which window: its number, or part of its title. These can be recorded: {}",
            list_text(windows)
        ));
    }
    if let Ok(number) = choice.parse::<u32>() {
        return windows.iter().find(|w| w.number == number).ok_or_else(|| {
            format!(
                "There is no window number {number}. Choose a number from 1 to {}: {}",
                windows.len(),
                list_text(windows)
            )
        });
    }
    let wanted = choice.to_lowercase();
    let matches: Vec<&WindowInfo> = windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&wanted) || w.process.to_lowercase().contains(&wanted))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "No open window has \"{choice}\" in its title or program name. These can be recorded: {}",
            list_text(windows)
        )),
        many => Err(format!(
            "More than one window has \"{choice}\" in its title or program name: {}. Use a number, or more of the title.",
            many.iter().map(|w| w.describe()).collect::<Vec<_>>().join("; ")
        )),
    }
}

/// The size of the video for a window of this size: even (H.264 needs it), and not too small.
pub fn recording_size(width: u32, height: u32) -> Result<(u32, u32), String> {
    let (w, h) = (width & !1, height & !1);
    if w < MIN_SIDE || h < MIN_SIDE {
        return Err(format!(
            "The window is {width} by {height} pixels. It must be at least {MIN_SIDE} by {MIN_SIDE}: Windows' video encoders refuse smaller pictures. Make the window bigger and try again."
        ));
    }
    Ok((w, h))
}

/// Put a picture into a picture of a fixed size (`width` by `height`): as it is if it fits, made smaller to
/// fit if it is too big (never made larger), centred on black. `memory` is used for the result if it is the
/// right size, so a recording does not need new memory for every picture.
pub fn fit_into(frame: &Frame, width: u32, height: u32, mut memory: Vec<u8>) -> Frame {
    let (tw, th) = (width as usize, height as usize);
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    if memory.len() != tw * th * 4 {
        memory = vec![0u8; tw * th * 4];
    } else {
        memory.fill(0);
    }
    if fw > 0 && fh > 0 {
        // The size it is drawn at: its own, or the biggest that fits with the same shape.
        let (dw, dh) = if fw <= tw && fh <= th {
            (fw, fh)
        } else if fw * th > fh * tw {
            (tw, (fh * tw / fw).max(1))
        } else {
            ((fw * th / fh).max(1), th)
        };
        let (left, top) = ((tw - dw) / 2, (th - dh) / 2);
        for y in 0..dh {
            let source_row = if dh == fh { y } else { y * fh / dh };
            let to = ((top + y) * tw + left) * 4;
            if dw == fw {
                let from = source_row * fw * 4;
                memory[to..to + dw * 4].copy_from_slice(&frame.bgra[from..from + dw * 4]);
            } else {
                for x in 0..dw {
                    let from = (source_row * fw + x * fw / dw) * 4;
                    memory[to + x * 4..to + x * 4 + 4].copy_from_slice(&frame.bgra[from..from + 4]);
                }
            }
        }
    }
    Frame {
        width,
        height,
        t_ms: frame.t_ms,
        captured_100ns: frame.captured_100ns,
        arrived_100ns: frame.arrived_100ns,
        bgra: memory,
    }
}

/// Pictures from another source, all made the same size. A picture already that size passes through
/// untouched, which is nearly always, so this costs nothing until a window is resized.
pub struct FittedSource {
    inner: Box<dyn FrameSource>,
    width: u32,
    height: u32,
}

impl FittedSource {
    pub fn new(inner: Box<dyn FrameSource>, width: u32, height: u32) -> Self {
        Self { inner, width, height }
    }
}

impl FrameSource for FittedSource {
    fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
        let frame = self.inner.next_frame(timeout)?;
        if frame.width == self.width && frame.height == self.height {
            return Some(frame);
        }
        let fitted = fit_into(&frame, self.width, self.height, Vec::new());
        self.inner.recycle(frame);
        Some(fitted)
    }
    fn dropped(&self) -> u64 {
        self.inner.dropped()
    }
    fn received(&self) -> u64 {
        self.inner.received()
    }
    fn recycle(&mut self, frame: Frame) {
        // Pictures that passed through belong to the source; any other size is simply let go there.
        self.inner.recycle(frame);
    }
    fn work(&self) -> CaptureWork {
        self.inner.work()
    }
    fn ended(&self) -> Option<String> {
        self.inner.ended()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(number: u32, process: &str, title: &str) -> WindowInfo {
        WindowInfo {
            number,
            handle: 1000 + number as isize,
            title: title.into(),
            process: process.into(),
            width: 800,
            height: 600,
        }
    }

    fn list() -> Vec<WindowInfo> {
        number_windows(vec![
            window(0, "notepad.exe", "Untitled - Notepad"),
            window(0, "chrome.exe", "Lesson plan - Google Docs - Chrome"),
            window(0, "chrome.exe", "Inbox - Mail - Chrome"),
            window(0, "explorer.exe", "Documents"),
        ])
    }

    #[test]
    fn windows_are_numbered_from_one_and_read_out_in_words() {
        let windows = list();
        assert_eq!(windows.iter().map(|w| w.number).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        assert_eq!(windows[0].describe(), "1. notepad.exe: Untitled - Notepad (800 by 600)");
    }

    #[test]
    fn a_window_is_chosen_by_number_or_by_part_of_its_title_or_program() {
        let windows = list();
        assert_eq!(choose_window(&windows, "4").unwrap().process, "explorer.exe");
        assert_eq!(choose_window(&windows, " 1 ").unwrap().title, "Untitled - Notepad");
        assert_eq!(choose_window(&windows, "lesson plan").unwrap().number, 2, "capital letters do not matter");
        assert_eq!(choose_window(&windows, "NOTEPAD").unwrap().number, 1, "the program name works too");
        assert_eq!(choose_window(&windows, "documents").unwrap().number, 4);
    }

    #[test]
    fn a_choice_that_fits_none_or_several_is_explained_with_the_list() {
        let windows = list();
        let none = choose_window(&windows, "spreadsheet").unwrap_err();
        assert!(none.contains("No open window has \"spreadsheet\"") && none.contains("1. notepad.exe"), "{none}");
        let many = choose_window(&windows, "chrome").unwrap_err();
        assert!(
            many.contains("More than one window has \"chrome\"")
                && many.contains("2. chrome.exe: Lesson plan")
                && many.contains("3. chrome.exe: Inbox")
                && !many.contains("notepad"),
            "{many}"
        );
        let past = choose_window(&windows, "9").unwrap_err();
        assert!(past.contains("There is no window number 9. Choose a number from 1 to 4"), "{past}");
        let blank = choose_window(&windows, "  ").unwrap_err();
        assert!(blank.contains("Say which window"), "{blank}");
        let nothing = choose_window(&[], "1").unwrap_err();
        assert!(nothing.contains("no open windows that can be recorded"), "{nothing}");
    }

    #[test]
    fn the_video_is_even_and_not_too_small() {
        assert_eq!(recording_size(801, 601), Ok((800, 600)));
        assert_eq!(recording_size(1280, 720), Ok((1280, 720)));
        let small = recording_size(40, 700).unwrap_err();
        assert!(small.contains("at least 64 by 64") && small.contains("40 by 700"), "{small}");
    }

    fn picture(width: u32, height: u32, value: u8) -> Frame {
        Frame {
            width,
            height,
            t_ms: 12,
            captured_100ns: Some(3),
            arrived_100ns: Some(4),
            bgra: vec![value; (width * height * 4) as usize],
        }
    }

    fn at(f: &Frame, x: u32, y: u32) -> u8 {
        f.bgra[((y * f.width + x) * 4) as usize]
    }

    #[test]
    fn a_smaller_picture_is_centred_on_black_at_its_own_size() {
        let fitted = fit_into(&picture(100, 50, 9), 200, 100, Vec::new());
        assert_eq!((fitted.width, fitted.height, fitted.bgra.len()), (200, 100, 200 * 100 * 4));
        assert_eq!(at(&fitted, 0, 0), 0, "black at the corner");
        assert_eq!(at(&fitted, 50, 25), 9, "the picture starts at 50, 25");
        assert_eq!(at(&fitted, 149, 74), 9, "and ends at 149, 74");
        assert_eq!(at(&fitted, 150, 74), 0);
        assert_eq!(at(&fitted, 149, 75), 0);
        assert_eq!((fitted.t_ms, fitted.captured_100ns, fitted.arrived_100ns), (12, Some(3), Some(4)));
    }

    #[test]
    fn a_bigger_picture_is_made_smaller_to_fit_keeping_its_shape() {
        // 400 by 100 into 200 by 100: half the size, 200 by 50, centred top and bottom.
        let fitted = fit_into(&picture(400, 100, 7), 200, 100, Vec::new());
        assert_eq!(at(&fitted, 100, 24), 0, "black above");
        assert_eq!(at(&fitted, 100, 25), 7, "picture from row 25");
        assert_eq!(at(&fitted, 100, 74), 7, "to row 74");
        assert_eq!(at(&fitted, 100, 75), 0, "black below");
        assert_eq!(at(&fitted, 0, 25), 7, "full width");
        // A tall picture is limited by its height.
        let tall = fit_into(&picture(100, 400, 5), 200, 100, Vec::new());
        assert_eq!(at(&tall, 86, 50), 0, "black to the left of a 25 pixel wide picture");
        assert_eq!(at(&tall, 87, 50), 5, "it starts at 87");
        assert_eq!(at(&tall, 111, 50), 5, "and ends at 111");
        assert_eq!(at(&tall, 112, 50), 0);
    }

    #[test]
    fn scaling_down_takes_the_right_pixels() {
        // Columns carry their own number, so which ones survive can be seen.
        let mut p = picture(8, 2, 0);
        for y in 0..2 {
            for x in 0..8 {
                p.bgra[((y * 8 + x) * 4) as usize] = x as u8 + 1;
            }
        }
        let fitted = fit_into(&p, 4, 2, Vec::new());
        let row: Vec<u8> = (0..4).map(|x| at(&fitted, x, 0)).collect();
        assert_eq!(row, vec![1, 3, 5, 7]);
    }

    #[test]
    fn old_memory_is_cleared_not_shown() {
        let fitted = fit_into(&picture(10, 10, 9), 20, 20, vec![0xEE; 20 * 20 * 4]);
        assert_eq!(at(&fitted, 0, 0), 0, "the black round the picture is black, not what the memory held");
        assert_eq!(at(&fitted, 5, 5), 9);
        assert_eq!(fitted.bgra.len(), 20 * 20 * 4);
    }

    struct Scripted {
        sizes: Vec<(u32, u32)>,
        given: usize,
        recycled: usize,
        ended_after: Option<usize>,
    }
    impl FrameSource for Scripted {
        fn next_frame(&mut self, _: Duration) -> Option<Frame> {
            let (w, h) = *self.sizes.get(self.given)?;
            self.given += 1;
            Some(picture(w, h, 6))
        }
        fn dropped(&self) -> u64 {
            0
        }
        fn recycle(&mut self, _: Frame) {
            self.recycled += 1;
        }
        fn ended(&self) -> Option<String> {
            self.ended_after.filter(|n| self.given >= *n).map(|_| "The window was closed.".to_string())
        }
    }

    #[test]
    fn a_fitted_source_keeps_one_size_through_a_resize_and_passes_normal_pictures_untouched() {
        let inner = Scripted {
            sizes: vec![(200, 100), (200, 100), (300, 150), (100, 50)],
            given: 0,
            recycled: 0,
            ended_after: Some(4),
        };
        let mut source = FittedSource::new(Box::new(inner), 200, 100);
        let a = source.next_frame(Duration::ZERO).unwrap();
        assert_eq!((a.width, a.height), (200, 100));
        let b = source.next_frame(Duration::ZERO).unwrap();
        assert_eq!((b.width, b.height), (200, 100));
        let big = source.next_frame(Duration::ZERO).unwrap();
        assert_eq!((big.width, big.height), (200, 100), "a window that grew is fitted into the video's size");
        assert_eq!(at(&big, 100, 50), 6);
        let small = source.next_frame(Duration::ZERO).unwrap();
        assert_eq!((small.width, small.height), (200, 100), "and one that shrank is centred");
        assert_eq!(at(&small, 0, 0), 0);
        assert!(source.next_frame(Duration::ZERO).is_none());
        assert_eq!(source.ended().as_deref(), Some("The window was closed."), "the end of the window is passed on");
    }
}
