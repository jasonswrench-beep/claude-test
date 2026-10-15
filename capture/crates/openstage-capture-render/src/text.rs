// SPDX-License-Identifier: Apache-2.0
//! Putting words on a frame (story C7).
//!
//! Drawing letters well takes fonts and knowledge of every language's rules, which Windows has and this program
//! does not. So text is drawn by a [`TextRenderer`], and on Windows that is Windows' own text drawing. What is
//! decided here, the same everywhere, is the layout: where the box is, how lines are broken, how they sit across
//! the box, and what happens when the words do not fit.
//!
//! * The box is the overlay's rectangle. If it has a background colour, the whole box is filled with it.
//! * The text starts a little in from the top and sides of the box (a third of a line's height), so it never
//!   touches the edge of its background.
//! * Lines are broken at spaces to fit the box. A single word wider than the box is cut off at the box's edge.
//! * Lines that do not fit in the box's height are cut off, and the export says so, so the author can make the
//!   box bigger or the text smaller. Words that are cut off cannot be read by a viewer.

use crate::canvas::{Canvas, PixelRect, Rgb};
use openstage_capture_project::EditAlign;

/// One piece of text to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest {
    pub text: String,
    /// The box, in pixels of the frame.
    pub area: PixelRect,
    /// The height of a line, in pixels.
    pub font_px: f32,
    pub bold: bool,
    pub color: Rgb,
    pub background: Option<Rgb>,
    pub align: EditAlign,
}

/// Whether all of a piece of text was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFit {
    pub complete: bool,
}

/// Something that can draw text onto a frame.
pub trait TextRenderer {
    /// Fill the background (if any) and draw the text inside the box, laid out as described in this file's header.
    fn draw(&self, canvas: &mut Canvas, request: &TextRequest) -> Result<TextFit, String>;
}

/// How far in from the box's edges the text starts, in pixels.
pub fn padding(font_px: f32) -> i32 {
    (font_px / 3.0).round().max(1.0) as i32
}

/// The line height as a multiple of the font height.
pub const LINE_SPACING: f32 = 1.2;

/// Break `text` into lines no wider than `max_width`, at spaces. `width_of` says how wide a piece of text is.
/// Line breaks in the text itself are kept. A word wider than `max_width` goes on a line of its own (the renderer
/// cuts it off at the edge of the box).
pub fn wrap_lines(text: &str, max_width: f32, width_of: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if line.is_empty() || width_of(&candidate) <= max_width {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Where a line of `width` starts across a box, in pixels from the box's left edge.
pub fn line_start(align: EditAlign, box_width: i32, pad: i32, width: f32) -> i32 {
    let room = box_width as f32 - 2.0 * pad as f32 - width;
    match align {
        EditAlign::Left => pad,
        EditAlign::Center => pad + (room / 2.0).max(0.0).round() as i32,
        EditAlign::Right => pad + room.max(0.0).round() as i32,
    }
}

/// A plain renderer for tests: each letter is a block. It is not for real videos.
pub struct BlockText;

impl BlockText {
    /// How wide a letter is, as a share of the font height.
    pub const LETTER_WIDTH: f32 = 0.6;
}

impl TextRenderer for BlockText {
    fn draw(&self, canvas: &mut Canvas, request: &TextRequest) -> Result<TextFit, String> {
        if let Some(background) = request.background {
            canvas.fill_rect(request.area, background);
        }
        let pad = padding(request.font_px);
        let letter = (request.font_px * Self::LETTER_WIDTH).max(1.0);
        let width_of = |s: &str| s.chars().count() as f32 * letter;
        let inner = (request.area.w - 2 * pad) as f32;
        let lines = wrap_lines(&request.text, inner, &width_of);
        let step = request.font_px * LINE_SPACING;
        let mut complete = true;
        for (i, line) in lines.iter().enumerate() {
            let top = request.area.y + pad + (i as f32 * step).round() as i32;
            if top + request.font_px.round() as i32 > request.area.y + request.area.h - pad / 2 {
                complete = false;
                break;
            }
            let start = request.area.x + line_start(request.align, request.area.w, pad, width_of(line));
            if width_of(line) > inner {
                complete = false;
            }
            for (j, ch) in line.chars().enumerate() {
                if ch.is_whitespace() {
                    continue;
                }
                let left = start + (j as f32 * letter).round() as i32;
                let block =
                    PixelRect::new(left, top, (letter * 0.8).round().max(1.0) as i32, request.font_px.round() as i32);
                // Never outside the box.
                let inside = PixelRect::new(request.area.x, request.area.y, request.area.w, request.area.h);
                let clipped = block.clipped_to(inside);
                canvas.fill_rect(clipped, request.color);
            }
        }
        Ok(TextFit { complete })
    }
}

impl PixelRect {
    /// The part of this rectangle inside `other`.
    pub fn clipped_to(&self, other: PixelRect) -> PixelRect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        PixelRect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &str) -> f32 {
        s.chars().count() as f32 * 10.0
    }

    #[test]
    fn lines_are_broken_at_spaces_to_fit() {
        let lines = wrap_lines("one two three four", 105.0, &width);
        assert_eq!(lines, ["one two", "three four"]);
        assert_eq!(wrap_lines("one two", 1000.0, &width), ["one two"]);
        assert_eq!(wrap_lines("", 100.0, &width), [""]);
    }

    #[test]
    fn a_word_too_wide_for_the_box_gets_a_line_of_its_own() {
        let lines = wrap_lines("a extraordinarily b", 50.0, &width);
        assert_eq!(lines, ["a", "extraordinarily", "b"]);
    }

    #[test]
    fn breaks_in_the_text_itself_are_kept() {
        assert_eq!(wrap_lines("first\nsecond line", 1000.0, &width), ["first", "second line"]);
    }

    #[test]
    fn lines_sit_left_centre_or_right_of_the_box() {
        // A 200 wide box with 10 pixels of padding, and a line 60 wide.
        assert_eq!(line_start(EditAlign::Left, 200, 10, 60.0), 10);
        assert_eq!(line_start(EditAlign::Center, 200, 10, 60.0), 10 + 60);
        assert_eq!(line_start(EditAlign::Right, 200, 10, 60.0), 10 + 120);
        // A line wider than the box starts at the padding whatever the alignment.
        assert_eq!(line_start(EditAlign::Right, 100, 10, 500.0), 10);
    }

    fn request(text: &str, area: PixelRect, align: EditAlign) -> TextRequest {
        TextRequest {
            text: text.into(),
            area,
            font_px: 10.0,
            bold: false,
            color: Rgb(255, 255, 255),
            background: Some(Rgb(0, 0, 80)),
            align,
        }
    }

    #[test]
    fn the_background_fills_the_whole_box_and_the_letters_stay_inside_it() {
        let mut canvas = Canvas::new(100, 60);
        let area = PixelRect::new(10, 10, 60, 20);
        let fit = BlockText.draw(&mut canvas, &request("hello", area, EditAlign::Left)).unwrap();
        assert!(fit.complete);
        assert_eq!(canvas.pixel(10, 10), Rgb(0, 0, 80), "the background reaches the corner");
        assert_eq!(canvas.pixel(69, 29), Rgb(0, 0, 80));
        assert_eq!(canvas.pixel(9, 10), Rgb(0, 0, 0), "and stops at the edge");
        assert_eq!(canvas.pixel(70, 29), Rgb(0, 0, 0));
        // Somewhere inside there are letters.
        let white = (10..70)
            .flat_map(|x| (10..30).map(move |y| (x, y)))
            .filter(|&(x, y)| canvas.pixel(x, y) == Rgb(255, 255, 255))
            .count();
        assert!(white > 50);
    }

    #[test]
    fn words_that_do_not_fit_are_cut_off_and_the_renderer_says_so() {
        let mut canvas = Canvas::new(100, 60);
        // Room for one line only.
        let area = PixelRect::new(0, 0, 50, 16);
        let fit = BlockText.draw(&mut canvas, &request("one two three four five", area, EditAlign::Left)).unwrap();
        assert!(!fit.complete);
        // Nothing was drawn below the box.
        assert!((0..100).all(|x| (16..60).all(|y| canvas.pixel(x, y) == Rgb(0, 0, 0))));
    }

    #[test]
    fn centred_text_is_further_right_than_left_text() {
        let first_white = |align| {
            let mut canvas = Canvas::new(120, 30);
            BlockText.draw(&mut canvas, &request("ab", PixelRect::new(0, 0, 120, 20), align)).unwrap();
            (0..120).find(|&x| (0..20).any(|y| canvas.pixel(x, y) == Rgb(255, 255, 255))).unwrap()
        };
        let (left, centre, right) =
            (first_white(EditAlign::Left), first_white(EditAlign::Center), first_white(EditAlign::Right));
        assert!(left < centre && centre < right, "{left} {centre} {right}");
    }

    #[test]
    fn a_box_with_no_background_leaves_the_picture_under_the_letters_alone() {
        let mut canvas = Canvas::new(40, 20);
        canvas.clear(Rgb(9, 9, 9));
        let mut r = request("x", PixelRect::new(0, 0, 40, 20), EditAlign::Left);
        r.background = None;
        BlockText.draw(&mut canvas, &r).unwrap();
        assert_eq!(canvas.pixel(39, 19), Rgb(9, 9, 9));
    }
}
