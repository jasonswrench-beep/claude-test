// SPDX-License-Identifier: Apache-2.0
//! How a zoom moves.
//!
//! A zoom lasts from its start to its end. It takes `transition` to move in (the whole picture
//! grows to the chosen part), holds, and takes `transition` to move out. The move is smooth: it
//! starts and ends gently (a "smoothstep" curve), not at a constant speed. Because a learner cannot
//! turn off motion that is part of a video, the author can ask for **reduced motion**: the zoom then
//! cuts straight to the chosen part and straight back, with no movement at all (story C5).

use openstage_capture_project::{EditRect, EDIT_FULL};

/// A zoom, as the plan keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomSpan {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The part of the picture that fills the frame while zoomed.
    pub rect: EditRect,
    pub transition_ms: u64,
}

/// What part of the picture fills the frame at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomView {
    pub rect: EditRect,
    /// True while the view is moving.
    pub moving: bool,
}

fn whole_picture() -> EditRect {
    EditRect { x: 0.0, y: 0.0, width: EDIT_FULL, height: EDIT_FULL }
}

/// 0 to 1, starting and ending gently.
fn smoothstep(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

fn lerp(a: EditRect, b: EditRect, p: f64) -> EditRect {
    let mix = |x: f64, y: f64| x + (y - x) * p;
    EditRect { x: mix(a.x, b.x), y: mix(a.y, b.y), width: mix(a.width, b.width), height: mix(a.height, b.height) }
}

/// The view at `time_us` (microseconds). Zooms do not overlap, so at most one applies.
pub fn zoom_view(zooms: &[ZoomSpan], time_us: u64, reduce_motion: bool) -> ZoomView {
    let whole = whole_picture();
    for zoom in zooms {
        let (start, end) = (zoom.start_ms * 1000, zoom.end_ms * 1000);
        if time_us < start || time_us >= end {
            continue;
        }
        if reduce_motion || zoom.transition_ms == 0 {
            return ZoomView { rect: zoom.rect, moving: false };
        }
        let transition = zoom.transition_ms * 1000;
        if time_us < start + transition {
            let p = smoothstep((time_us - start) as f64 / transition as f64);
            return ZoomView { rect: lerp(whole, zoom.rect, p), moving: true };
        }
        if time_us >= end - transition {
            let p = smoothstep((end - time_us) as f64 / transition as f64);
            return ZoomView { rect: lerp(whole, zoom.rect, p), moving: true };
        }
        return ZoomView { rect: zoom.rect, moving: false };
    }
    ZoomView { rect: whole, moving: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom() -> ZoomSpan {
        ZoomSpan {
            id: "z".into(),
            start_ms: 10_000,
            end_ms: 14_000,
            rect: EditRect { x: 2500.0, y: 2500.0, width: 5000.0, height: 5000.0 },
            transition_ms: 500,
        }
    }

    fn at(ms: u64, reduce: bool) -> ZoomView {
        zoom_view(&[zoom()], ms * 1000, reduce)
    }

    #[test]
    fn outside_a_zoom_the_whole_picture_shows() {
        for ms in [0, 9_999, 14_000, 20_000] {
            let v = at(ms, false);
            assert_eq!(v.rect, whole_picture(), "{ms}");
            assert!(!v.moving);
        }
        assert_eq!(zoom_view(&[], 5_000_000, false).rect, whole_picture());
    }

    #[test]
    fn a_zoom_holds_on_its_part_between_its_moves() {
        for ms in [10_500, 12_000, 13_499] {
            let v = at(ms, false);
            assert_eq!(v.rect, zoom().rect, "{ms}");
            assert!(!v.moving);
        }
    }

    #[test]
    fn the_move_in_goes_from_the_whole_picture_to_the_part_and_is_gentle_at_both_ends() {
        assert_eq!(at(10_000, false).rect, whole_picture(), "it starts from the whole picture");
        let halfway = at(10_250, false);
        assert!(halfway.moving);
        assert!((halfway.rect.width - 7500.0).abs() < 1.0, "half way through, half way there: {}", halfway.rect.width);
        // Gentle: in the first tenth of the move, much less than a tenth of the distance is covered.
        let early = at(10_050, false);
        assert!(10_000.0 - early.rect.width < 0.05 * 5000.0, "starts slowly: {}", early.rect.width);
        // And the width only ever gets smaller on the way in.
        let mut last = f64::MAX;
        for ms in (10_000..=10_500).step_by(10) {
            let w = at(ms, false).rect.width;
            assert!(w <= last, "{ms}: {w} after {last}");
            last = w;
        }
        assert!((at(10_500, false).rect.width - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn the_move_out_mirrors_the_move_in() {
        for offset in [0u64, 100, 250, 400, 499] {
            let inside = at(10_000 + offset, false).rect;
            let outside = at(14_000 - 1 - offset, false).rect;
            assert!(
                (inside.width - outside.width).abs() < 25.0,
                "offset {offset}: {} vs {}",
                inside.width,
                outside.width
            );
        }
        assert!(at(13_700, false).moving);
    }

    #[test]
    fn reduced_motion_cuts_straight_there_and_straight_back_with_no_movement() {
        assert_eq!(at(9_999, true).rect, whole_picture());
        for ms in [10_000, 10_100, 12_000, 13_950] {
            let v = at(ms, true);
            assert_eq!(v.rect, zoom().rect, "{ms}");
            assert!(!v.moving, "nothing moves with reduced motion");
        }
        assert_eq!(at(14_000, true).rect, whole_picture());
    }

    #[test]
    fn a_zoom_with_no_transition_is_a_cut() {
        let mut z = zoom();
        z.transition_ms = 0;
        assert_eq!(zoom_view(&[z.clone()], 10_000_000, false).rect, z.rect);
    }
}
