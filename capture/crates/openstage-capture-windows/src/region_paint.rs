// SPDX-License-Identifier: Apache-2.0
//! What is drawn for a region: the picker's overlay and the thin frame shown while recording.
//!
//! Both are pictures with a see-through background that Windows lays over the screen (a "layered
//! window" with a transparency value for every pixel). This file only makes the pixels, so the drawing can
//! be checked here; `win_region.rs` and `win_outline.rs` put them on the screen.
//!
//! Pixels are 32 bits, `0xAARRGGBB`, with the colour already multiplied by the transparency (what Windows
//! expects). All the colours used are black or opaque, so multiplying changes nothing except for the dim.

use crate::region::Region;
use crate::region_editor::{Handle, RegionEditor};

/// The dimming over everything outside the rectangle: black at 60 percent.
pub const DIM: u32 = 0x9900_0000;
/// Inside the rectangle: all but invisible, yet not completely see-through, because Windows lets the
/// mouse pass through pixels that are completely see-through, and a click inside has to reach the picker.
pub const INSIDE: u32 = 0x0100_0000;
pub const BLACK: u32 = 0xFF00_0000;
pub const WHITE: u32 = 0xFFFF_FFFF;
/// The colour of the frame shown while recording: an orange that shows on light and dark screens.
pub const FRAME: u32 = 0xFFFF_B000;
/// The inside of a handle: a blue that stands out from the white border.
pub const HANDLE_BLUE: u32 = 0xFF00_78D4;
/// How thick the frame around a region is while recording, at 96 dots per inch.
pub const FRAME_THICKNESS: i32 = 4;

/// A picture to lay over part of a display. `left` and `top` are where its corner is, in pixels from the
/// display's top left corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Canvas {
    pub fn at(&self, x: i32, y: i32) -> u32 {
        let (cx, cy) = (x - self.left, y - self.top);
        if cx < 0 || cy < 0 || cx >= self.width as i32 || cy >= self.height as i32 {
            0
        } else {
            self.pixels[(cy as u32 * self.width + cx as u32) as usize]
        }
    }
}

/// How far outside the rectangle a point is; zero or less means inside it.
fn outside_by(r: &Region, x: i32, y: i32) -> i32 {
    let (left, top) = (r.x as i32, r.y as i32);
    let (right, bottom) = (left + r.width as i32 - 1, top + r.height as i32 - 1);
    (left - x).max(x - right).max(top - y).max(y - bottom)
}

/// The frame shown around a region while it is being recorded: a band just outside the rectangle (so it is
/// not in the part that is kept), dark on both sides of a bright middle so that it shows on any screen.
/// Cut off at the display's edges, so it never spills onto a neighbouring display. `None` when nothing
/// of it would be on the display (the region is the whole display).
pub fn recording_frame(region: &Region, display_width: u32, display_height: u32, scale: i32) -> Option<Canvas> {
    let band = FRAME_THICKNESS * scale.max(1);
    let left = (region.x as i32 - band).max(0);
    let top = (region.y as i32 - band).max(0);
    let right = (region.x as i32 + region.width as i32 + band).min(display_width as i32);
    let bottom = (region.y as i32 + region.height as i32 + band).min(display_height as i32);
    if right <= left || bottom <= top {
        return None;
    }
    let (width, height) = ((right - left) as u32, (bottom - top) as u32);
    let mut pixels = vec![0u32; width as usize * height as usize];
    let mut any = false;
    for row in 0..height as i32 {
        for col in 0..width as i32 {
            let d = outside_by(region, left + col, top + row);
            // One dark line against the picture, a bright band, one dark line against the screen.
            let colour = match d {
                d if d < 1 || d > band => continue,
                1 => BLACK,
                d if d == band => BLACK,
                _ => FRAME,
            };
            pixels[row as usize * width as usize + col as usize] = colour;
            any = true;
        }
    }
    any.then_some(Canvas { left, top, width, height, pixels })
}

/// A rectangle of pixels: `right` and `bottom` are one past the last pixel.
#[derive(Clone, Copy)]
struct Area {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Area {
    /// The area grown by `by` pixels on every side (or shrunk, if negative).
    fn grown(self, by: i32) -> Area {
        Area { left: self.left - by, top: self.top - by, right: self.right + by, bottom: self.bottom + by }
    }

    /// A square of `half` pixels either side of a point, the point's own pixel included.
    fn around(x: i32, y: i32, half: i32) -> Area {
        Area { left: x - half, top: y - half, right: x + half + 1, bottom: y + half + 1 }
    }
}

/// Colour an area of a picture `width` by `height`, cut off at the picture's edges.
fn fill(pixels: &mut [u32], (width, height): (i32, i32), area: Area, colour: u32) {
    let (left, top) = (area.left.max(0), area.top.max(0));
    let (right, bottom) = (area.right.min(width), area.bottom.min(height));
    if right <= left {
        return;
    }
    for y in top..bottom {
        let row = y as usize * width as usize;
        pixels[row + left as usize..row + right as usize].fill(colour);
    }
}

/// The picker's overlay for a whole display: everything dimmed except the rectangle, which has a white
/// border (black along its outside, so it shows on anything) and eight handles. `scale` is 1 at 96 dots
/// per inch, 2 at 192, and so on, so that the border and handles stay a usable size on a sharp screen.
pub fn picker_overlay(editor: &RegionEditor, scale: i32) -> Canvas {
    let scale = scale.max(1);
    let (dw, dh) = editor.display_size();
    let (w, h) = (dw as i32, dh as i32);
    let mut pixels = vec![DIM; w as usize * h as usize];
    let r = editor.region();
    let rectangle =
        Area { left: r.x as i32, top: r.y as i32, right: (r.x + r.width) as i32, bottom: (r.y + r.height) as i32 };
    let size = (w, h);
    // A dark line just outside, a white border (two pixels thick at 96 dots per inch), then the inside.
    fill(&mut pixels, size, rectangle.grown(scale), BLACK);
    fill(&mut pixels, size, rectangle, WHITE);
    fill(&mut pixels, size, rectangle.grown(-2 * scale), INSIDE);
    for handle in Handle::DRAWN {
        let (cx, cy) = editor.handle_position(handle);
        for (half, colour) in [(6, BLACK), (5, WHITE), (3, HANDLE_BLUE)] {
            fill(&mut pixels, size, Area::around(cx, cy, half * scale), colour);
        }
    }
    Canvas { left: 0, top: 0, width: dw, height: dh, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
        Region { x, y, width, height }
    }

    #[test]
    fn the_recording_frame_is_a_band_just_outside_the_region_and_never_inside_it() {
        let frame = recording_frame(&region(100, 100, 200, 150), 1920, 1080, 1).unwrap();
        // Nothing inside the region, not even on its edge pixels: that is the part that is kept.
        for (x, y) in [(100, 100), (299, 249), (150, 120), (200, 200)] {
            assert_eq!(frame.at(x, y), 0, "({x}, {y}) is inside the region");
        }
        // Along the left side: dark against the picture, bright, bright, dark against the screen.
        assert_eq!(frame.at(99, 150), BLACK);
        assert_eq!(frame.at(98, 150), FRAME);
        assert_eq!(frame.at(97, 150), FRAME);
        assert_eq!(frame.at(96, 150), BLACK);
        assert_eq!(frame.at(95, 150), 0, "beyond the band is see-through");
        // The corners are filled too, so there is no gap.
        assert_eq!(frame.at(98, 98), FRAME);
        assert_eq!(frame.at(96, 96), BLACK);
        // And the other three sides.
        assert_eq!(frame.at(300, 150), BLACK);
        assert_eq!(frame.at(150, 99), BLACK);
        assert_eq!(frame.at(150, 250), BLACK);
        assert_eq!(frame.at(150, 253), BLACK);
    }

    #[test]
    fn a_sharper_screen_gets_a_thicker_frame() {
        let frame = recording_frame(&region(100, 100, 200, 150), 1920, 1080, 2).unwrap();
        assert_eq!(frame.at(99, 150), BLACK);
        assert_eq!(frame.at(92, 150), BLACK, "eight pixels out is the far dark line");
        assert_eq!(frame.at(91, 150), 0);
        assert_eq!(frame.at(95, 150), FRAME);
    }

    #[test]
    fn the_frame_is_cut_off_at_the_display_so_it_never_lands_on_the_next_display() {
        let at_corner = recording_frame(&region(0, 0, 640, 360), 1920, 1080, 1).unwrap();
        assert_eq!((at_corner.left, at_corner.top), (0, 0), "nothing is drawn at negative positions");
        assert_eq!(at_corner.at(640, 100), BLACK);
        assert_eq!(at_corner.at(100, 360), BLACK);
        assert_eq!(at_corner.at(0, 0), 0, "inside the region");
        let at_far = recording_frame(&region(1280, 720, 640, 360), 1920, 1080, 1).unwrap();
        assert_eq!((at_far.left + at_far.width as i32, at_far.top + at_far.height as i32), (1920, 1080));
        assert_eq!(at_far.at(1279, 800), BLACK);
        assert_eq!(at_far.at(1919, 1079), 0);
    }

    #[test]
    fn a_region_that_is_the_whole_display_has_no_frame_to_show() {
        assert_eq!(recording_frame(&region(0, 0, 1920, 1080), 1920, 1080, 1), None);
    }

    #[test]
    fn the_picker_dims_everything_but_the_rectangle() {
        let editor = RegionEditor::new(1920, 1080, Some(region(400, 300, 800, 450)));
        let overlay = picker_overlay(&editor, 1);
        assert_eq!((overlay.width, overlay.height, overlay.pixels.len()), (1920, 1080, 1920 * 1080));
        assert_eq!(overlay.at(10, 10), DIM, "outside is dimmed");
        assert_eq!(overlay.at(1900, 1000), DIM);
        assert_eq!(overlay.at(800, 500), INSIDE, "inside is almost clear, but still takes the mouse");
        assert_ne!(overlay.at(800, 500) >> 24, 0, "not completely see-through, or the mouse would go straight through");
    }

    #[test]
    fn the_rectangle_has_a_white_border_with_a_dark_line_outside_it() {
        let editor = RegionEditor::new(1920, 1080, Some(region(400, 300, 800, 450)));
        let overlay = picker_overlay(&editor, 1);
        // Down the left side at a height away from the handles: dark, white, white, then inside.
        assert_eq!(overlay.at(399, 400), BLACK);
        assert_eq!(overlay.at(400, 400), WHITE);
        assert_eq!(overlay.at(401, 400), WHITE);
        assert_eq!(overlay.at(402, 400), INSIDE);
        assert_eq!(overlay.at(398, 400), DIM);
        // The far side.
        assert_eq!(overlay.at(1199, 400), WHITE);
        assert_eq!(overlay.at(1200, 400), BLACK);
        assert_eq!(overlay.at(1201, 400), DIM);
        // Top and bottom.
        assert_eq!(overlay.at(600, 299), BLACK);
        assert_eq!(overlay.at(600, 300), WHITE);
        assert_eq!(overlay.at(600, 749), WHITE);
        assert_eq!(overlay.at(600, 750), BLACK);
    }

    #[test]
    fn all_eight_handles_are_drawn_on_the_border_with_a_blue_centre() {
        let editor = RegionEditor::new(1920, 1080, Some(region(400, 300, 800, 450)));
        let overlay = picker_overlay(&editor, 1);
        for handle in Handle::DRAWN {
            let (x, y) = editor.handle_position(handle);
            assert_eq!(overlay.at(x, y), HANDLE_BLUE, "{handle:?} has a blue centre");
            assert_eq!(overlay.at(x + 4, y), WHITE, "{handle:?} has a white ring");
            assert_eq!(overlay.at(x + 6, y), BLACK, "{handle:?} has a dark outline");
        }
        // The handle on an edge sticks out on both sides of the border.
        let (x, y) = editor.handle_position(Handle::Left);
        assert_eq!(overlay.at(x - 5, y), WHITE);
        assert_eq!(overlay.at(x + 5, y), WHITE);
    }

    #[test]
    fn handles_at_the_edge_of_the_display_are_cut_off_not_a_crash() {
        let mut editor = RegionEditor::new(640, 480, None);
        editor.use_whole_display();
        let overlay = picker_overlay(&editor, 1);
        assert_eq!(overlay.pixels.len(), 640 * 480);
        assert_eq!(overlay.at(0, 0), HANDLE_BLUE, "the top left handle is centred on the corner");
        assert_eq!(overlay.at(639, 479), HANDLE_BLUE, "the bottom right handle is centred one past the last pixel");
        assert_eq!(overlay.at(300, 479), WHITE, "between the handles the last row is the border");
    }

    #[test]
    fn a_sharper_screen_gets_bigger_handles_and_a_thicker_border() {
        let editor = RegionEditor::new(1920, 1080, Some(region(400, 300, 800, 450)));
        let overlay = picker_overlay(&editor, 2);
        assert_eq!(overlay.at(400, 400), WHITE);
        assert_eq!(overlay.at(403, 400), WHITE, "the border is four pixels wide");
        assert_eq!(overlay.at(404, 400), INSIDE);
        let (x, y) = editor.handle_position(Handle::Top);
        assert_eq!(overlay.at(x + 10, y), WHITE);
        assert_eq!(overlay.at(x + 12, y), BLACK);
    }
}
