// SPDX-License-Identifier: Apache-2.0
//! The rectangle being chosen when the author picks a region by dragging (story A1 in `docs/prd.md`).
//!
//! This holds only the geometry: where the rectangle is, which handle a pointer is on, what dragging a
//! handle does, what the arrow keys do, and what to say about the result in words. It draws nothing and
//! knows nothing about Windows, so it can be tested thoroughly here. The picker window that draws it
//! and passes the mouse to it is in `win_region.rs`.
//!
//! Rules it keeps, so that dragging is predictable:
//!
//! * The rectangle never leaves the display and is never smaller than [`MIN_SIDE`] in either direction.
//! * Dragging a handle moves only the edges that handle belongs to; the opposite edges stay put. A
//!   handle dragged past the opposite edge stops at the smallest size instead of flipping over.
//! * Grabbing a handle does not make the rectangle jump: it follows the pointer from where it was
//!   grabbed.
//! * While dragging, sizes are exact. Making the width and height even (H.264 needs it) happens when the
//!   choice is accepted, and is said in words.

use crate::region::{Region, MIN_SIDE};

/// What came of asking the author to choose a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    /// The author chose this region (width and height already even) and was told these changes, if any.
    Chosen { region: Region, notes: Vec<String> },
    /// The author closed the picker without choosing.
    Cancelled,
}

/// Which part of the rectangle a pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// Inside the rectangle: dragging moves it.
    Inside,
}

impl Handle {
    /// The eight handles that are drawn, in the order a person would walk round the rectangle.
    pub const DRAWN: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    fn left(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Left | Handle::BottomLeft)
    }
    fn right(self) -> bool {
        matches!(self, Handle::TopRight | Handle::Right | Handle::BottomRight)
    }
    fn top(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Top | Handle::TopRight)
    }
    fn bottom(self) -> bool {
        matches!(self, Handle::BottomLeft | Handle::Bottom | Handle::BottomRight)
    }
}

/// The four edges of the rectangle. `right` and `bottom` are one past the last pixel, as in a [`Region`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Edges {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Edges {
    fn width(&self) -> i32 {
        self.right - self.left
    }
    fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// What a drag in progress is doing.
#[derive(Debug, Clone, Copy)]
enum Drag {
    /// Moving the edges of a handle. `offset` is how far the grabbed edges were from the pointer.
    Resize { handle: Handle, offset: (i32, i32) },
    /// Moving the whole rectangle from where it started.
    Move { from: (i32, i32), start: Edges },
    /// Drawing a new rectangle from a corner that stays where it was pressed.
    New { origin: (i32, i32) },
}

/// A rectangle on a display of a known size.
#[derive(Debug, Clone)]
pub struct RegionEditor {
    display: (i32, i32),
    edges: Edges,
    drag: Option<Drag>,
}

impl RegionEditor {
    /// A rectangle on a display `display_width` by `display_height` pixels. Starts at `initial` if that
    /// is a usable region of this display, else in the middle of the display at two thirds of its size.
    pub fn new(display_width: u32, display_height: u32, initial: Option<Region>) -> Self {
        let display = (display_width as i32, display_height as i32);
        let usable = initial.filter(|r| r.fit(display_width, display_height).is_ok());
        let edges = match usable {
            Some(r) => Edges {
                left: r.x as i32,
                top: r.y as i32,
                right: (r.x + r.width) as i32,
                bottom: (r.y + r.height) as i32,
            },
            None => {
                let min = Self::min_side(display);
                // Two thirds of the display, made even, and never smaller than the least allowed.
                let width = ((display.0 * 2 / 3) & !1).clamp(min.0, display.0);
                let height = ((display.1 * 2 / 3) & !1).clamp(min.1, display.1);
                let left = (display.0 - width) / 2;
                let top = (display.1 - height) / 2;
                Edges { left, top, right: left + width, bottom: top + height }
            }
        };
        Self { display, edges, drag: None }
    }

    /// The least width and height allowed on this display (a tiny display allows what fits).
    fn min_side(display: (i32, i32)) -> (i32, i32) {
        ((MIN_SIDE as i32).min(display.0), (MIN_SIDE as i32).min(display.1))
    }

    /// The rectangle as it is now. Its width or height may be odd; see [`RegionEditor::accepted`].
    pub fn region(&self) -> Region {
        Region {
            x: self.edges.left as u32,
            y: self.edges.top as u32,
            width: self.edges.width() as u32,
            height: self.edges.height() as u32,
        }
    }

    /// The display's size.
    pub fn display_size(&self) -> (u32, u32) {
        (self.display.0 as u32, self.display.1 as u32)
    }

    /// The rectangle with an even width and height, ready to record, and anything that was changed in
    /// words. This is what the author gets when they accept.
    pub fn accepted(&self) -> Result<(Region, Vec<String>), String> {
        self.region().fit(self.display.0 as u32, self.display.1 as u32)
    }

    /// Whether the rectangle is the whole display.
    pub fn is_whole_display(&self) -> bool {
        self.edges == Edges { left: 0, top: 0, right: self.display.0, bottom: self.display.1 }
    }

    // ---- the pointer ----

    /// What the pointer at (`x`, `y`) is on, if it is on the rectangle or one of its handles. `grab` is how
    /// many pixels away still counts. Corners come before edges, edges before the inside.
    pub fn hit_test(&self, x: i32, y: i32, grab: i32) -> Option<Handle> {
        let Edges { left, top, right, bottom } = self.edges;
        let near = |a: i32, b: i32| (a - b).abs() <= grab;
        let between = |v: i32, a: i32, b: i32| v >= a - grab && v <= b + grab;
        if near(x, left) && near(y, top) {
            return Some(Handle::TopLeft);
        }
        if near(x, right) && near(y, top) {
            return Some(Handle::TopRight);
        }
        if near(x, left) && near(y, bottom) {
            return Some(Handle::BottomLeft);
        }
        if near(x, right) && near(y, bottom) {
            return Some(Handle::BottomRight);
        }
        if near(y, top) && between(x, left, right) {
            return Some(Handle::Top);
        }
        if near(y, bottom) && between(x, left, right) {
            return Some(Handle::Bottom);
        }
        if near(x, left) && between(y, top, bottom) {
            return Some(Handle::Left);
        }
        if near(x, right) && between(y, top, bottom) {
            return Some(Handle::Right);
        }
        if x > left && x < right && y > top && y < bottom {
            return Some(Handle::Inside);
        }
        None
    }

    /// The point a handle is drawn at: the corners, and the middle of each edge.
    pub fn handle_position(&self, handle: Handle) -> (i32, i32) {
        let Edges { left, top, right, bottom } = self.edges;
        let (mid_x, mid_y) = ((left + right) / 2, (top + bottom) / 2);
        match handle {
            Handle::TopLeft => (left, top),
            Handle::Top => (mid_x, top),
            Handle::TopRight => (right, top),
            Handle::Right => (right, mid_y),
            Handle::BottomRight => (right, bottom),
            Handle::Bottom => (mid_x, bottom),
            Handle::BottomLeft => (left, bottom),
            Handle::Left => (left, mid_y),
            Handle::Inside => (mid_x, mid_y),
        }
    }

    /// The pointer was pressed at (`x`, `y`). On a handle, that handle is dragged; inside, the rectangle is
    /// moved; anywhere else on the display, a new rectangle is started from that point. Returns what is
    /// being dragged (`None` for a new rectangle).
    pub fn press(&mut self, x: i32, y: i32, grab: i32) -> Option<Handle> {
        let hit = self.hit_test(x, y, grab);
        self.drag = Some(match hit {
            Some(Handle::Inside) => Drag::Move { from: (x, y), start: self.edges },
            Some(handle) => {
                let offset_x = if handle.left() {
                    self.edges.left - x
                } else if handle.right() {
                    self.edges.right - x
                } else {
                    0
                };
                let offset_y = if handle.top() {
                    self.edges.top - y
                } else if handle.bottom() {
                    self.edges.bottom - y
                } else {
                    0
                };
                Drag::Resize { handle, offset: (offset_x, offset_y) }
            }
            None => Drag::New { origin: (x.clamp(0, self.display.0), y.clamp(0, self.display.1)) },
        });
        hit
    }

    /// The pointer moved to (`x`, `y`) while pressed. Does nothing if nothing is being dragged.
    pub fn drag_to(&mut self, x: i32, y: i32) {
        let Some(drag) = self.drag else { return };
        let (min_w, min_h) = Self::min_side(self.display);
        let (w, h) = self.display;
        match drag {
            Drag::Move { from, start } => {
                let left = (start.left + x - from.0).clamp(0, w - start.width());
                let top = (start.top + y - from.1).clamp(0, h - start.height());
                self.edges = Edges { left, top, right: left + start.width(), bottom: top + start.height() };
            }
            Drag::Resize { handle, offset } => {
                let (ex, ey) = (x + offset.0, y + offset.1);
                let mut e = self.edges;
                if handle.left() {
                    e.left = ex.clamp(0, e.right - min_w);
                }
                if handle.right() {
                    e.right = ex.clamp(e.left + min_w, w);
                }
                if handle.top() {
                    e.top = ey.clamp(0, e.bottom - min_h);
                }
                if handle.bottom() {
                    e.bottom = ey.clamp(e.top + min_h, h);
                }
                self.edges = e;
            }
            Drag::New { origin } => {
                let (px, py) = (x.clamp(0, w), y.clamp(0, h));
                let span = |from: i32, to: i32, min: i32, limit: i32| {
                    // From the pressed point towards the pointer, at least the least size, then slid back
                    // inside the display if that pushed it out.
                    let (mut a, mut b) =
                        if to >= from { (from, to.max(from + min)) } else { ((to).min(from - min), from) };
                    if b > limit {
                        a -= b - limit;
                        b = limit;
                    }
                    if a < 0 {
                        b -= a;
                        a = 0;
                    }
                    (a, b)
                };
                let (left, right) = span(origin.0, px, min_w, w);
                let (top, bottom) = span(origin.1, py, min_h, h);
                self.edges = Edges { left, top, right, bottom };
            }
        }
    }

    /// The pointer was let go.
    pub fn release(&mut self) {
        self.drag = None;
    }

    /// Whether a drag is in progress.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    // ---- the keyboard and the numbers ----

    /// Move the whole rectangle by (`dx`, `dy`) pixels, stopping at the display's edges.
    pub fn nudge(&mut self, dx: i32, dy: i32) {
        let (w, h) = (self.edges.width(), self.edges.height());
        let left = (self.edges.left + dx).clamp(0, self.display.0 - w);
        let top = (self.edges.top + dy).clamp(0, self.display.1 - h);
        self.edges = Edges { left, top, right: left + w, bottom: top + h };
    }

    /// Make the rectangle wider and taller (or, with negative numbers, narrower and shorter) by moving
    /// its right and bottom edges, within the least size and the display.
    pub fn grow(&mut self, dw: i32, dh: i32) {
        let (min_w, min_h) = Self::min_side(self.display);
        self.edges.right = (self.edges.right + dw).clamp(self.edges.left + min_w, self.display.0);
        self.edges.bottom = (self.edges.bottom + dh).clamp(self.edges.top + min_h, self.display.1);
    }

    /// Use four typed numbers: how far from the left, how far from the top, the width and the height.
    /// If they can be used the rectangle becomes that (with an even size) and anything changed is
    /// said; if not, the rectangle stays as it was and the reason is said in words.
    pub fn set_numbers(&mut self, x: u32, y: u32, width: u32, height: u32) -> Result<Vec<String>, String> {
        let (region, notes) = Region { x, y, width, height }.fit(self.display.0 as u32, self.display.1 as u32)?;
        self.edges = Edges {
            left: region.x as i32,
            top: region.y as i32,
            right: (region.x + region.width) as i32,
            bottom: (region.y + region.height) as i32,
        };
        self.drag = None;
        Ok(notes)
    }

    /// Use the whole display.
    pub fn use_whole_display(&mut self) {
        self.edges = Edges { left: 0, top: 0, right: self.display.0, bottom: self.display.1 };
        self.drag = None;
    }

    // ---- words ----

    /// What will be recorded, in a short sentence for the author to read or hear.
    pub fn describe(&self, display_number: u32) -> String {
        let r = self.region();
        if self.is_whole_display() {
            format!("Region: the whole of Display {display_number} ({} by {}).", self.display.0, self.display.1)
        } else {
            format!(
                "Region: {} by {} at {}, {} on Display {display_number} ({} by {}).",
                r.width, r.height, r.x, r.y, self.display.0, self.display.1
            )
        }
    }

    /// What to say after a drag, so that a screen reader hears where the rectangle is (the same words
    /// as [`RegionEditor::describe`] without the display).
    pub fn describe_short(&self) -> String {
        let r = self.region();
        format!("{} by {} at {}, {}", r.width, r.height, r.x, r.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> RegionEditor {
        // A 1920 by 1080 display with a 400 by 300 rectangle at 100, 200.
        RegionEditor::new(1920, 1080, Some(Region { x: 100, y: 200, width: 400, height: 300 }))
    }

    fn r(e: &RegionEditor) -> (u32, u32, u32, u32) {
        let r = e.region();
        (r.x, r.y, r.width, r.height)
    }

    #[test]
    fn it_starts_where_it_was_told_or_in_the_middle_at_two_thirds() {
        assert_eq!(r(&editor()), (100, 200, 400, 300));
        let middle = RegionEditor::new(1920, 1080, None);
        assert_eq!(r(&middle), (320, 180, 1280, 720));
        // A region that does not fit the display is not trusted.
        let too_big = RegionEditor::new(800, 600, Some(Region { x: 700, y: 0, width: 400, height: 300 }));
        assert_eq!(r(&too_big), (134, 100, 532, 400));
        // A tiny display still gets a rectangle that fits it.
        let tiny = RegionEditor::new(50, 40, None);
        assert_eq!(r(&tiny), (0, 0, 50, 40));
    }

    #[test]
    fn the_pointer_finds_corners_then_edges_then_the_inside() {
        let e = editor();
        let at = |x, y| e.hit_test(x, y, 8);
        assert_eq!(at(100, 200), Some(Handle::TopLeft));
        assert_eq!(at(505, 195), Some(Handle::TopRight));
        assert_eq!(at(93, 507), Some(Handle::BottomLeft));
        assert_eq!(at(500, 500), Some(Handle::BottomRight));
        assert_eq!(at(300, 203), Some(Handle::Top));
        assert_eq!(at(300, 497), Some(Handle::Bottom));
        assert_eq!(at(104, 350), Some(Handle::Left));
        assert_eq!(at(496, 350), Some(Handle::Right));
        assert_eq!(at(300, 350), Some(Handle::Inside));
        assert_eq!(at(50, 50), None, "empty display");
        assert_eq!(at(300, 150), None, "above the top handle's reach");
        // The edge handle does not extend past the corners: well beyond the left of the rectangle is nothing.
        assert_eq!(at(60, 300), None);
    }

    #[test]
    fn dragging_a_corner_moves_that_corner_and_keeps_the_opposite_one() {
        let mut e = editor();
        assert_eq!(e.press(502, 498, 8), Some(Handle::BottomRight));
        e.drag_to(602, 598);
        assert_eq!(r(&e), (100, 200, 500, 400), "grew by the distance the pointer moved: no jump on grabbing");
        e.release();
        assert_eq!(e.press(98, 203, 8), Some(Handle::TopLeft));
        e.drag_to(148, 253);
        assert_eq!(r(&e), (150, 250, 450, 350), "the bottom right corner stayed at 600, 600");
    }

    #[test]
    fn dragging_an_edge_moves_only_that_edge() {
        let mut e = editor();
        e.press(300, 200, 8);
        e.drag_to(999, 150);
        assert_eq!(r(&e), (100, 150, 400, 350), "only the top edge moved; the x of the pointer is ignored");
        e.release();
        e.press(500, 300, 8);
        e.drag_to(700, 0);
        assert_eq!(r(&e), (100, 150, 600, 350), "only the right edge moved");
    }

    #[test]
    fn a_handle_dragged_past_the_other_side_stops_at_the_smallest_size() {
        let mut e = editor();
        e.press(100, 350, 8); // the left edge
        e.drag_to(900, 350); // far past the right edge at 500
        assert_eq!(r(&e), (436, 200, 64, 300), "64 wide, still ending at the right edge");
        e.release();
        assert_eq!(e.press(470, 500, 8), Some(Handle::Bottom)); // the bottom edge of what is left of it
        e.drag_to(470, 0);
        assert_eq!(r(&e), (436, 200, 64, 64), "64 tall, still starting at the top edge");
    }

    #[test]
    fn nothing_leaves_the_display() {
        let mut e = editor();
        e.press(300, 350, 8); // inside: move
        e.drag_to(-5000, -5000);
        assert_eq!(r(&e), (0, 0, 400, 300), "moved to the top left and no further");
        e.drag_to(9000, 9000);
        assert_eq!(r(&e), (1520, 780, 400, 300), "moved to the bottom right and no further");
        e.release();
        // Resizing past the edge stops at the edge.
        let mut e = editor();
        e.press(500, 500, 8);
        e.drag_to(5000, 5000);
        assert_eq!(r(&e), (100, 200, 1820, 880));
        e.release();
        e.press(100, 200, 8);
        e.drag_to(-100, -100);
        assert_eq!(r(&e), (0, 0, 1920, 1080));
        assert!(e.is_whole_display());
    }

    #[test]
    fn moving_keeps_the_size_and_follows_the_pointer_from_where_it_was_grabbed() {
        let mut e = editor();
        assert_eq!(e.press(300, 350, 8), Some(Handle::Inside));
        e.drag_to(340, 330);
        assert_eq!(r(&e), (140, 180, 400, 300));
        e.drag_to(300, 350);
        assert_eq!(r(&e), (100, 200, 400, 300), "back where it began");
    }

    #[test]
    fn dragging_on_empty_display_draws_a_new_rectangle() {
        let mut e = editor();
        assert_eq!(e.press(1000, 600, 8), None);
        e.drag_to(1400, 800);
        assert_eq!(r(&e), (1000, 600, 400, 200));
        e.drag_to(800, 500);
        assert_eq!(
            r(&e),
            (800, 500, 200, 100),
            "dragging back past the start flips it: a new rectangle may go any way"
        );
        e.release();
        // A click with no drag makes the smallest rectangle, not nothing.
        assert_eq!(e.press(1700, 900, 8), None);
        e.drag_to(1701, 901);
        assert_eq!(r(&e), (1700, 900, 64, 64));
        // Near a corner of the display it slides back inside.
        e.release();
        assert_eq!(e.press(1900, 1070, 8), None);
        e.drag_to(1910, 1075);
        assert_eq!(r(&e), (1856, 1016, 64, 64));
    }

    #[test]
    fn the_arrow_keys_move_and_resize_within_the_limits() {
        let mut e = editor();
        e.nudge(1, 0);
        e.nudge(0, -10);
        assert_eq!(r(&e), (101, 190, 400, 300));
        e.nudge(-500, -500);
        assert_eq!((r(&e).0, r(&e).1), (0, 0));
        e.grow(10, -10);
        assert_eq!(r(&e), (0, 0, 410, 290));
        e.grow(-5000, -5000);
        assert_eq!(r(&e), (0, 0, 64, 64));
        e.grow(5000, 5000);
        assert_eq!(r(&e), (0, 0, 1920, 1080));
    }

    #[test]
    fn typed_numbers_are_used_when_they_fit_and_refused_in_words_when_not() {
        let mut e = editor();
        let notes = e.set_numbers(10, 20, 641, 361).unwrap();
        assert_eq!(r(&e), (10, 20, 640, 360), "odd sizes are made even");
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("even width"), "{notes:?}");
        let before = r(&e);
        let why = e.set_numbers(1900, 0, 640, 360).unwrap_err();
        assert!(why.contains("reaches past the edge of the display"), "{why}");
        let why = e.set_numbers(0, 0, 10, 10).unwrap_err();
        assert!(why.contains("at least 64 by 64"), "{why}");
        assert_eq!(r(&e), before, "a refused choice changes nothing");
    }

    #[test]
    fn accepting_makes_the_size_even_and_says_so() {
        let mut e = editor();
        e.press(500, 500, 8);
        e.drag_to(551, 531); // 451 by 331
        e.release();
        assert_eq!(r(&e), (100, 200, 451, 331), "while dragging the size is exact");
        let (region, notes) = e.accepted().unwrap();
        assert_eq!((region.width, region.height), (450, 330));
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn the_whole_display_is_one_choice_and_is_said_as_such() {
        let mut e = editor();
        assert_eq!(e.describe(1), "Region: 400 by 300 at 100, 200 on Display 1 (1920 by 1080).");
        assert_eq!(e.describe_short(), "400 by 300 at 100, 200");
        e.use_whole_display();
        assert!(e.is_whole_display());
        assert_eq!(e.describe(2), "Region: the whole of Display 2 (1920 by 1080).");
    }

    #[test]
    fn the_drawn_handles_sit_on_the_corners_and_the_middle_of_the_edges() {
        let e = editor();
        let at: Vec<_> = Handle::DRAWN.iter().map(|h| e.handle_position(*h)).collect();
        assert_eq!(
            at,
            vec![(100, 200), (300, 200), (500, 200), (500, 350), (500, 500), (300, 500), (100, 500), (100, 350)]
        );
    }

    #[test]
    fn dragging_with_nothing_pressed_does_nothing_and_a_drag_ends_on_release() {
        let mut e = editor();
        e.drag_to(1000, 1000);
        assert_eq!(r(&e), (100, 200, 400, 300));
        assert!(!e.dragging());
        e.press(300, 350, 8);
        assert!(e.dragging());
        e.release();
        assert!(!e.dragging());
        e.drag_to(0, 0);
        assert_eq!(r(&e), (100, 200, 400, 300), "after release the pointer no longer drags");
    }
}
