// SPDX-License-Identifier: Apache-2.0
//! Recording part of a display instead of all of it.
//!
//! A region is a rectangle of one display, given in whole pixels measured from that display's top left
//! corner: how far from the left, how far from the top, how wide, how tall. Entering the four numbers
//! is how it is chosen (story A1 in `docs/prd.md`), so it works without a mouse and without seeing
//! the screen. Windows' screen capture always delivers the whole display; the part wanted is cut
//! out of each picture before it is encoded.
//!
//! H.264 video needs an even width and height, so an odd size is made one pixel smaller, and the
//! author is told. A region may also not be tiny: Windows' video encoders refuse very small pictures.

use crate::{Frame, FrameSource};
use std::time::Duration;

/// The smallest width or height of a region, in pixels. A choice made to keep Windows' video
/// encoders working; not yet checked against every encoder.
pub const MIN_SIDE: u32 = 64;

/// A rectangle of a display, in pixels from the display's top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    /// From four whole numbers such as `100,200,1280,720` (commas or spaces between them): how far
    /// from the left, how far from the top, the width and the height.
    pub fn parse(text: &str) -> Result<Region, String> {
        let how = "A region is four whole numbers: how far from the left edge of the display, how far from the top edge, the width and the height, for example 100,200,1280,720.";
        let parts: Vec<&str> = text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()).collect();
        if parts.len() != 4 {
            return Err(format!(
                "\"{}\" has {} number{}. {how}",
                text.trim(),
                parts.len(),
                if parts.len() == 1 { "" } else { "s" }
            ));
        }
        let mut numbers = [0u32; 4];
        for (slot, part) in numbers.iter_mut().zip(&parts) {
            if part.starts_with('-') && part[1..].chars().all(|c| c.is_ascii_digit()) && part.len() > 1 {
                return Err(format!(
                    "{part} cannot be used: a region is measured from the top left corner of the display and cannot start outside it. {how}"
                ));
            }
            *slot = part.parse().map_err(|_| format!("\"{part}\" is not a whole number. {how}"))?;
        }
        Ok(Region { x: numbers[0], y: numbers[1], width: numbers[2], height: numbers[3] })
    }

    /// Check the region against a display of this size, and make the width and height even. Gives the
    /// region to use and, in words, anything that was changed; or says why it cannot be used.
    pub fn fit(self, display_width: u32, display_height: u32) -> Result<(Region, Vec<String>), String> {
        let mut notes = Vec::new();
        let mut region = self;
        if region.width < MIN_SIDE || region.height < MIN_SIDE {
            return Err(format!(
                "The region is {} by {} pixels. It must be at least {MIN_SIDE} by {MIN_SIDE}: Windows' video encoders refuse smaller pictures.",
                region.width, region.height
            ));
        }
        let right = region.x as u64 + region.width as u64;
        let bottom = region.y as u64 + region.height as u64;
        if right > display_width as u64 || bottom > display_height as u64 {
            return Err(format!(
                "The region reaches past the edge of the display. It starts {} pixels from the left and {} from the top and is {} by {}, so it ends {} pixels from the left and {} from the top, but the display is {} by {}.",
                region.x, region.y, region.width, region.height, right, bottom, display_width, display_height
            ));
        }
        if region.width & 1 == 1 {
            notes.push(format!(
                "The width {} was made {}, because H.264 video needs an even width.",
                region.width,
                region.width - 1
            ));
            region.width -= 1;
        }
        if region.height & 1 == 1 {
            notes.push(format!(
                "The height {} was made {}, because H.264 video needs an even height.",
                region.height,
                region.height - 1
            ));
            region.height -= 1;
        }
        Ok((region, notes))
    }

    /// In words, for the author to hear what will be recorded.
    pub fn describe(&self) -> String {
        format!(
            "the part of the display that starts {} pixels from the left and {} from the top, {} by {} pixels",
            self.x, self.y, self.width, self.height
        )
    }
}

/// Cut a region out of a picture. If the picture is smaller than the region needs (the display changed
/// size during a recording), the part that is missing is black, so the video keeps its size.
pub fn crop(frame: &Frame, region: &Region) -> Frame {
    let (w, h) = (region.width as usize, region.height as usize);
    let mut bgra = vec![0u8; w * h * 4];
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    let (x, y) = (region.x as usize, region.y as usize);
    if x < fw && y < fh {
        let columns = w.min(fw - x);
        for row in 0..h.min(fh - y) {
            let from = ((y + row) * fw + x) * 4;
            let to = row * w * 4;
            bgra[to..to + columns * 4].copy_from_slice(&frame.bgra[from..from + columns * 4]);
        }
    }
    Frame {
        width: region.width,
        height: region.height,
        t_ms: frame.t_ms,
        captured_100ns: frame.captured_100ns,
        arrived_100ns: frame.arrived_100ns,
        bgra,
    }
}

/// Pictures from another source, each cut down to a region.
pub struct CroppedSource {
    inner: Box<dyn FrameSource>,
    region: Region,
}

impl CroppedSource {
    pub fn new(inner: Box<dyn FrameSource>, region: Region) -> Self {
        Self { inner, region }
    }
}

impl FrameSource for CroppedSource {
    fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
        self.inner.next_frame(timeout).map(|f| crop(&f, &self.region))
    }
    fn dropped(&self) -> u64 {
        self.inner.dropped()
    }
    fn received(&self) -> u64 {
        self.inner.received()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_numbers_are_a_region_however_they_are_separated() {
        let want = Region { x: 100, y: 200, width: 1280, height: 720 };
        for text in ["100,200,1280,720", "100, 200, 1280, 720", " 100 200 1280 720 ", "100,200 1280,720"] {
            assert_eq!(Region::parse(text), Ok(want), "{text}");
        }
    }

    #[test]
    fn a_region_that_cannot_be_read_is_explained_with_an_example() {
        for (text, fragment) in [
            ("", "has 0 numbers"),
            ("10,20,30", "has 3 numbers"),
            ("1,2,3,4,5", "has 5 numbers"),
            ("1,2,3", "for example 100,200,1280,720"),
            ("a,2,640,480", "\"a\" is not a whole number"),
            ("1.5,2,640,480", "\"1.5\" is not a whole number"),
            ("-5,2,640,480", "-5 cannot be used"),
        ] {
            let message = Region::parse(text).expect_err(text);
            assert!(message.contains(fragment), "{text}: {message}");
        }
    }

    #[test]
    fn a_region_that_fits_is_kept_as_it_is() {
        let r = Region { x: 0, y: 0, width: 1920, height: 1080 };
        assert_eq!(r.fit(1920, 1080), Ok((r, vec![])));
        let r = Region { x: 960, y: 540, width: 960, height: 540 };
        assert_eq!(r.fit(1920, 1080), Ok((r, vec![])), "touching the right and bottom edges is fine");
    }

    #[test]
    fn an_odd_size_is_made_even_and_said() {
        let (r, notes) = Region { x: 10, y: 10, width: 1281, height: 721 }.fit(1920, 1080).unwrap();
        assert_eq!((r.width, r.height), (1280, 720));
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("The width 1281 was made 1280") && notes[1].contains("The height 721 was made 720"));
        let (r, notes) = Region { x: 0, y: 0, width: 1280, height: 721 }.fit(1920, 1080).unwrap();
        assert_eq!((r.width, r.height, notes.len()), (1280, 720, 1));
    }

    #[test]
    fn a_region_past_the_edge_says_where_it_ends_and_how_big_the_display_is() {
        let message = Region { x: 1000, y: 0, width: 1000, height: 500 }.fit(1920, 1080).unwrap_err();
        assert!(
            message.contains("ends 2000 pixels from the left and 500 from the top")
                && message.contains("the display is 1920 by 1080"),
            "{message}"
        );
        assert!(Region { x: 0, y: 600, width: 500, height: 500 }.fit(1920, 1080).is_err());
        // Numbers so big that adding them would overflow are still just "past the edge".
        assert!(Region { x: u32::MAX, y: 0, width: u32::MAX, height: 100 }.fit(1920, 1080).is_err());
    }

    #[test]
    fn a_tiny_region_is_refused_in_words() {
        let message = Region { x: 0, y: 0, width: 32, height: 500 }.fit(1920, 1080).unwrap_err();
        assert!(message.contains("32 by 500") && message.contains("at least 64 by 64"), "{message}");
        assert!(Region { x: 0, y: 0, width: 64, height: 64 }.fit(1920, 1080).is_ok());
        assert!(Region { x: 0, y: 0, width: 65, height: 64 }.fit(1920, 1080).is_ok());
    }

    /// A picture where every pixel says where it is: blue is the column, green the row.
    fn picture(width: u32, height: u32) -> Frame {
        let mut bgra = Vec::new();
        for row in 0..height {
            for col in 0..width {
                bgra.extend_from_slice(&[col as u8, row as u8, 7, 255]);
            }
        }
        Frame { width, height, t_ms: 40, captured_100ns: Some(5), arrived_100ns: Some(6), bgra }
    }

    #[test]
    fn cutting_a_region_keeps_exactly_those_pixels_and_the_times() {
        let cut = crop(&picture(200, 100), &Region { x: 30, y: 20, width: 64, height: 64 });
        assert_eq!((cut.width, cut.height, cut.bgra.len()), (64, 64, 64 * 64 * 4));
        assert_eq!(&cut.bgra[0..4], &[30, 20, 7, 255], "the top left pixel is the region's corner");
        let last = cut.bgra.len() - 4;
        assert_eq!(
            &cut.bgra[last..],
            &[30 + 63, 20 + 63, 7, 255],
            "the bottom right pixel is the region's other corner"
        );
        let row5_col9 = (5 * 64 + 9) * 4;
        assert_eq!(&cut.bgra[row5_col9..row5_col9 + 4], &[39, 25, 7, 255]);
        assert_eq!((cut.t_ms, cut.captured_100ns, cut.arrived_100ns), (40, Some(5), Some(6)));
    }

    #[test]
    fn the_whole_picture_as_a_region_is_unchanged() {
        let p = picture(128, 96);
        let cut = crop(&p, &Region { x: 0, y: 0, width: 128, height: 96 });
        assert_eq!(cut.bgra, p.bgra);
    }

    #[test]
    fn a_picture_smaller_than_the_region_needs_leaves_black_not_a_smaller_video() {
        // The display shrank to 100 by 80 while the region was 64 by 64 at 50, 40.
        let cut = crop(&picture(100, 80), &Region { x: 50, y: 40, width: 64, height: 64 });
        assert_eq!((cut.width, cut.height, cut.bgra.len()), (64, 64, 64 * 64 * 4));
        assert_eq!(&cut.bgra[0..4], &[50, 40, 7, 255], "what exists is kept");
        let at = |row: usize, col: usize| &cut.bgra[(row * 64 + col) * 4..(row * 64 + col) * 4 + 4];
        assert_eq!(at(0, 49), &[99, 40, 7, 255], "the last column that exists");
        assert_eq!(at(0, 50), &[0, 0, 0, 0], "past the right edge is black");
        assert_eq!(at(39, 0), &[50, 79, 7, 255], "the last row that exists");
        assert_eq!(at(40, 0), &[0, 0, 0, 0], "past the bottom edge is black");
        // A region wholly outside is all black, never a crash.
        let outside = crop(&picture(100, 80), &Region { x: 500, y: 500, width: 64, height: 64 });
        assert!(outside.bgra.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_cropped_source_cuts_every_picture_and_passes_the_counts_on() {
        struct Fake(u32);
        impl FrameSource for Fake {
            fn next_frame(&mut self, _: Duration) -> Option<Frame> {
                self.0 += 1;
                (self.0 <= 2).then(|| picture(200, 100))
            }
            fn dropped(&self) -> u64 {
                7
            }
            fn received(&self) -> u64 {
                9
            }
        }
        let mut source = CroppedSource::new(Box::new(Fake(0)), Region { x: 0, y: 0, width: 64, height: 64 });
        assert_eq!(source.next_frame(Duration::ZERO).map(|f| (f.width, f.height)), Some((64, 64)));
        assert_eq!(source.next_frame(Duration::ZERO).map(|f| (f.width, f.height)), Some((64, 64)));
        assert!(source.next_frame(Duration::ZERO).is_none());
        assert_eq!((source.dropped(), source.received()), (7, 9));
    }
}
