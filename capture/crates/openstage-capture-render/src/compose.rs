// SPDX-License-Identifier: Apache-2.0
//! Drawing one frame of the finished video from its recipe.
//!
//! [`compose_frame`] takes what the plan says is in a frame (which pictures, which overlays, how far the zoom has
//! moved) and draws it, in this order, bottom to top:
//!
//! 1. **The main picture** (the screen, or an imported image or video): fitted into the frame, or into the place
//!    the author gave it, and zoomed.
//! 2. **Things that belong to the picture** (blur, blacked-out boxes, shapes, arrows), in the order the edit lists
//!    them. Their positions are fractions of the frame, and they move and grow with the zoom, so a blurred or
//!    blacked-out box always covers the same part of the picture whatever the zoom has done. A box that ends in the
//!    middle of a pixel is made bigger to take in the whole pixel, never smaller.
//! 3. **The webcam**: a small picture in the bottom right corner (or where the author put it), not zoomed. When
//!    there is no main picture at that moment it fills the frame.
//! 4. **Text**, not zoomed, so titles and labels stay the same size and in the same place while the picture moves.
//!
//! Choices that could go either way, for the owner to veto (also in `docs/capture-edit-format.md`):
//!
//! * Blur, blackout, shapes and arrows zoom with the picture (they point at, or hide, part of it); text does not.
//! * The webcam is not zoomed and is drawn above shapes, but below text.
//! * A picture is never stretched: if its shape differs from its box, it is made as large as fits and centred, with
//!   black either side. (A zoom whose rectangle is not the shape of the frame does stretch, which the edit's
//!   `zoom-aspect` warning already tells the author.)

use crate::canvas::{Canvas, Picture, PixelRect, Rgb, SourceRect};
use crate::plan::{FrameRecipe, Layer};
use crate::text::{TextRenderer, TextRequest};
use openstage_capture_project::{EditItem, EditRect, EditShapeKind, EditTrackKind, EDIT_FULL};
use std::rc::Rc;

/// Where pictures come from.
pub trait PictureSource {
    /// The picture of a layer at its moment: the frame of the recording, or the image. `Err` says in words why it
    /// cannot be had (the file is missing, or cannot be read).
    fn picture(&mut self, layer: &Layer) -> Result<Rc<Picture>, String>;
}

/// Where a webcam sits when the edit does not say: a small picture in the bottom right corner. The same share of
/// the width and the height, so the shape of the frame is kept.
pub const DEFAULT_WEBCAM_PLACE: EditRect = EditRect { x: 7550.0, y: 7350.0, width: 2200.0, height: 2200.0 };

/// A rectangle in pixels, with fractions.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Area {
    fn source_rect(self) -> SourceRect {
        SourceRect { x: self.x, y: self.y, w: self.w, h: self.h }
    }
    /// The whole pixels that touch this area: never smaller.
    fn covering(self) -> PixelRect {
        let x0 = self.x.floor();
        let y0 = self.y.floor();
        let x1 = (self.x + self.w).ceil();
        let y1 = (self.y + self.h).ceil();
        PixelRect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
    }
    /// The nearest whole pixels.
    fn nearest(self) -> PixelRect {
        let x0 = self.x.round();
        let y0 = self.y.round();
        let x1 = (self.x + self.w).round();
        let y1 = (self.y + self.h).round();
        PixelRect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
    }
}

/// The view that fills the frame, as fractions of it.
struct View {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Frame {
    width: f64,
    height: f64,
    view: View,
}

impl Frame {
    /// A rectangle given as fractions of the frame (ten-thousandths), where it lands once the zoom has moved the picture.
    fn zoomed(&self, r: &EditRect) -> Area {
        let (fx, fy, fw, fh) = (r.x / EDIT_FULL, r.y / EDIT_FULL, r.width / EDIT_FULL, r.height / EDIT_FULL);
        Area {
            x: (fx - self.view.x) / self.view.w * self.width,
            y: (fy - self.view.y) / self.view.h * self.height,
            w: fw / self.view.w * self.width,
            h: fh / self.view.h * self.height,
        }
    }
    /// The same rectangle, not moved by the zoom.
    fn fixed(&self, r: &EditRect) -> Area {
        Area {
            x: r.x / EDIT_FULL * self.width,
            y: r.y / EDIT_FULL * self.height,
            w: r.width / EDIT_FULL * self.width,
            h: r.height / EDIT_FULL * self.height,
        }
    }
    /// A point, moved by the zoom.
    fn zoomed_point(&self, x: f64, y: f64) -> (f32, f32) {
        (
            ((x / EDIT_FULL - self.view.x) / self.view.w * self.width) as f32,
            ((y / EDIT_FULL - self.view.y) / self.view.h * self.height) as f32,
        )
    }
    /// How many times larger things are drawn than at the whole picture.
    fn magnification(&self) -> f64 {
        1.0 / self.view.h
    }
    fn whole(&self) -> Area {
        Area { x: 0.0, y: 0.0, w: self.width, h: self.height }
    }
}

/// The biggest rectangle with a picture's shape that fits in `area`, in the middle of it.
fn fit(area: Area, picture: &Picture) -> Area {
    let (pw, ph) = (picture.width as f64, picture.height as f64);
    if pw <= 0.0 || ph <= 0.0 {
        return area;
    }
    let scale = (area.w / pw).min(area.h / ph);
    let (w, h) = (pw * scale, ph * scale);
    Area { x: area.x + (area.w - w) / 2.0, y: area.y + (area.h - h) / 2.0, w, h }
}

fn is_whole(n: f64) -> bool {
    (n - n.round()).abs() < 1e-9
}

/// Draw `picture` all over `area`. Copies when the area is whole pixels of the picture's own size.
fn draw(canvas: &mut Canvas, picture: &Picture, area: Area) {
    let whole = SourceRect { x: 0.0, y: 0.0, w: picture.width as f64, h: picture.height as f64 };
    if is_whole(area.x)
        && is_whole(area.y)
        && is_whole(area.w - picture.width as f64)
        && is_whole(area.h - picture.height as f64)
        && (area.w - picture.width as f64).abs() < 1e-9
        && (area.h - picture.height as f64).abs() < 1e-9
    {
        canvas.draw_picture(
            picture,
            whole,
            PixelRect::new(area.x.round() as i32, area.y.round() as i32, picture.width as i32, picture.height as i32),
        );
    } else {
        canvas.draw_picture_to(picture, whole, area.source_rect());
    }
}

fn colour(text: &str, what: &str) -> Result<Rgb, String> {
    Rgb::parse(text).map_err(|e| format!("{what}: {e}"))
}

/// Draw frame `recipe` onto `canvas`, which is cleared first. `notes` collects things the author should know about
/// the result (text that did not fit); the same note may come from many frames, so the caller keeps each once.
pub fn compose_frame(
    recipe: &FrameRecipe,
    sources: &mut dyn PictureSource,
    text: &dyn TextRenderer,
    canvas: &mut Canvas,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    canvas.clear(Rgb(0, 0, 0));
    let z = recipe.zoom.rect;
    let frame = Frame {
        width: canvas.width as f64,
        height: canvas.height as f64,
        view: View {
            x: z.x / EDIT_FULL,
            y: z.y / EDIT_FULL,
            w: (z.width / EDIT_FULL).max(1e-6),
            h: (z.height / EDIT_FULL).max(1e-6),
        },
    };

    // 1. The main picture.
    let mut has_main = false;
    for layer in recipe.layers.iter().filter(|l| l.track_kind != EditTrackKind::Webcam) {
        has_main = true;
        let picture = sources.picture(layer)?;
        let target = match &layer.placement {
            Some(place) => frame.fixed(place),
            None => frame.whole(),
        };
        let fitted = fit(target, &picture);
        // Where that lands once zoomed.
        let zoomed = Area {
            x: (fitted.x - frame.view.x * frame.width) / frame.view.w,
            y: (fitted.y - frame.view.y * frame.height) / frame.view.h,
            w: fitted.w / frame.view.w,
            h: fitted.h / frame.view.h,
        };
        draw(canvas, &picture, zoomed);
    }

    // 2. What belongs to the picture, in the order the edit lists it.
    for item in &recipe.overlays {
        match item {
            EditItem::Blur { rect, .. } => canvas.blur(frame.zoomed(rect).covering()),
            EditItem::Redact { rect, color, name, .. } => {
                canvas.fill_rect(frame.zoomed(rect).covering(), colour(color, &format!("The blackout \"{name}\""))?);
            }
            EditItem::Shape { shape, rect, outline, fill, line_width_bp, name, .. } => {
                let line = (line_width_bp / EDIT_FULL * frame.height * frame.magnification()).max(1.0);
                let outline = colour(outline, &format!("The shape \"{name}\""))?;
                let fill = match fill {
                    Some(f) => Some(colour(f, &format!("The shape \"{name}\""))?),
                    None => None,
                };
                let area = frame.zoomed(rect);
                match shape {
                    EditShapeKind::Rectangle => {
                        let r = area.nearest();
                        if let Some(fill) = fill {
                            canvas.fill_rect(r, fill);
                        }
                        canvas.stroke_rect(r, line.round() as i32, outline);
                    }
                    EditShapeKind::Ellipse => {
                        let r = area.nearest();
                        canvas.ellipse(r, line as f32, outline, fill);
                    }
                }
            }
            EditItem::Arrow { from, to, color, line_width_bp, name, .. } => {
                let line = (line_width_bp / EDIT_FULL * frame.height * frame.magnification()).max(1.0) as f32;
                canvas.arrow(
                    frame.zoomed_point(from.x, from.y),
                    frame.zoomed_point(to.x, to.y),
                    line,
                    colour(color, &format!("The arrow \"{name}\""))?,
                );
            }
            EditItem::Text { .. } | EditItem::Zoom { .. } => {}
        }
    }

    // 3. The webcam.
    for layer in recipe.layers.iter().filter(|l| l.track_kind == EditTrackKind::Webcam) {
        let picture = sources.picture(layer)?;
        let target = match (&layer.placement, has_main) {
            (Some(place), _) => frame.fixed(place),
            (None, true) => frame.fixed(&DEFAULT_WEBCAM_PLACE),
            (None, false) => frame.whole(),
        };
        draw(canvas, &picture, fit(target, &picture));
    }

    // 4. Text.
    for item in &recipe.overlays {
        if let EditItem::Text { text: words, rect, font_size_bp, color, background, bold, align, name, .. } = item {
            let request = TextRequest {
                text: words.clone(),
                area: frame.fixed(rect).nearest(),
                font_px: (font_size_bp / EDIT_FULL * frame.height) as f32,
                bold: *bold,
                color: colour(color, &format!("The text \"{name}\""))?,
                background: match background {
                    Some(b) => Some(colour(b, &format!("The text \"{name}\""))?),
                    None => None,
                },
                align: *align,
            };
            let fit = text.draw(canvas, &request)?;
            if !fit.complete {
                notes.push(format!(
                    "The text overlay \"{name}\" does not fit in its box, so some of its words are cut off and cannot be read. Make the box bigger or the text smaller."
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::FrameRecipe;
    use crate::text::BlockText;
    use crate::zoom::ZoomView;
    use openstage_capture_project::{EditAlign, EditPoint, EditSourceKind, EditSourceRef};
    use std::collections::HashMap;

    /// Pictures by the file name of their source. A picture colours each pixel by where it is, so a test can tell which
    /// part of it was drawn where: blue is the column, green the row, red the picture's own mark.
    struct Pictures {
        by_media: HashMap<String, Rc<Picture>>,
        asked: Vec<(String, u64)>,
    }

    impl PictureSource for Pictures {
        fn picture(&mut self, layer: &Layer) -> Result<Rc<Picture>, String> {
            let EditSourceRef::Take { media, .. } = &layer.source else { return Err("only takes here".into()) };
            self.asked.push((media.clone(), layer.source_time_ms));
            self.by_media.get(media).cloned().ok_or_else(|| format!("{media} is missing"))
        }
    }

    fn positional(width: u32, height: u32, mark: u8) -> Rc<Picture> {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgra.extend_from_slice(&[(x * 255 / width.max(1)) as u8, (y * 255 / height.max(1)) as u8, mark, 255]);
            }
        }
        Rc::new(Picture::new(width, height, bgra))
    }

    fn pictures() -> Pictures {
        let mut by_media = HashMap::new();
        by_media.insert("screen.mp4".to_string(), Rc::new(Picture::solid(160, 90, Rgb(10, 20, 30))));
        by_media.insert("webcam.mp4".to_string(), Rc::new(Picture::solid(64, 36, Rgb(200, 0, 0))));
        Pictures { by_media, asked: Vec::new() }
    }

    fn layer(media: &str, track: EditTrackKind, kind: EditSourceKind, placement: Option<EditRect>) -> Layer {
        Layer {
            clip_id: format!("clip-{media}"),
            track_kind: track,
            source: EditSourceRef::Take { take: 1, media: media.to_string() },
            kind,
            placement,
            source_time_ms: 1234,
        }
    }

    fn screen() -> Layer {
        layer("screen.mp4", EditTrackKind::Screen, EditSourceKind::Screen, None)
    }
    fn webcam(placement: Option<EditRect>) -> Layer {
        layer("webcam.mp4", EditTrackKind::Webcam, EditSourceKind::Webcam, placement)
    }

    fn whole_view() -> ZoomView {
        ZoomView { rect: EditRect { x: 0.0, y: 0.0, width: EDIT_FULL, height: EDIT_FULL }, moving: false }
    }

    fn recipe(layers: Vec<Layer>, overlays: Vec<EditItem>, zoom: ZoomView) -> FrameRecipe {
        FrameRecipe { index: 0, time_us: 0, layers, overlays, zoom }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> EditRect {
        EditRect { x, y, width, height }
    }

    fn draw_frame(r: &FrameRecipe, sources: &mut Pictures, size: (u32, u32)) -> (Canvas, Vec<String>) {
        let mut canvas = Canvas::new(size.0, size.1);
        let mut notes = Vec::new();
        compose_frame(r, sources, &BlockText, &mut canvas, &mut notes).unwrap();
        (canvas, notes)
    }

    #[test]
    fn the_screen_fills_the_frame_and_is_asked_for_at_the_moment_the_plan_says() {
        let mut p = pictures();
        let (canvas, _) = draw_frame(&recipe(vec![screen()], vec![], whole_view()), &mut p, (160, 90));
        assert!((0..90).all(|y| (0..160).all(|x| canvas.pixel(x, y) == Rgb(10, 20, 30))));
        assert_eq!(p.asked, [("screen.mp4".to_string(), 1234)]);
    }

    #[test]
    fn a_picture_of_another_shape_is_fitted_with_black_either_side_never_stretched() {
        let mut p = pictures();
        p.by_media.insert("screen.mp4".into(), Rc::new(Picture::solid(90, 90, Rgb(10, 20, 30))));
        let (canvas, _) = draw_frame(&recipe(vec![screen()], vec![], whole_view()), &mut p, (160, 90));
        assert_eq!(canvas.pixel(80, 45), Rgb(10, 20, 30), "the picture is in the middle");
        assert_eq!(canvas.pixel(40, 45), Rgb(10, 20, 30), "and 90 wide");
        assert_eq!(canvas.pixel(30, 45), Rgb(0, 0, 0), "with black beside it");
        assert_eq!(canvas.pixel(130, 45), Rgb(0, 0, 0));
    }

    #[test]
    fn a_webcam_is_a_small_picture_in_the_bottom_right_corner_by_default() {
        let mut p = pictures();
        let (canvas, _) = draw_frame(&recipe(vec![screen(), webcam(None)], vec![], whole_view()), &mut p, (400, 225));
        // 22 percent of the width from 75.5 percent, and of the height from 73.5 percent.
        assert_eq!(canvas.pixel(350, 190), Rgb(200, 0, 0), "inside the webcam picture");
        assert_eq!(canvas.pixel(100, 100), Rgb(10, 20, 30), "the screen elsewhere");
        assert_eq!(canvas.pixel(399, 224), Rgb(10, 20, 30), "the picture does not run to the very corner");
        assert_eq!(canvas.pixel(290, 190), Rgb(10, 20, 30), "or start further left");
    }

    #[test]
    fn a_webcam_with_no_other_picture_fills_the_frame_and_one_with_a_place_goes_there() {
        let mut p = pictures();
        let (alone, _) = draw_frame(&recipe(vec![webcam(None)], vec![], whole_view()), &mut p, (128, 72));
        assert_eq!(alone.pixel(0, 0), Rgb(200, 0, 0));
        assert_eq!(alone.pixel(127, 71), Rgb(200, 0, 0));
        let (placed, _) = draw_frame(
            &recipe(vec![screen(), webcam(Some(rect(500.0, 500.0, 2000.0, 2000.0)))], vec![], whole_view()),
            &mut p,
            (400, 225),
        );
        assert_eq!(placed.pixel(60, 30), Rgb(200, 0, 0), "in the top left area the author gave it");
        assert_eq!(placed.pixel(350, 190), Rgb(10, 20, 30), "and not in the default corner");
    }

    #[test]
    fn a_zoom_shows_just_the_chosen_part_of_the_picture_filling_the_frame() {
        let mut p = pictures();
        p.by_media.insert("screen.mp4".into(), positional(200, 100, 7));
        // The top left quarter of the picture.
        let zoom = ZoomView { rect: rect(0.0, 0.0, 5000.0, 5000.0), moving: false };
        let (canvas, _) = draw_frame(&recipe(vec![screen()], vec![], zoom), &mut p, (200, 100));
        // The frame's middle is the picture's quarter point: column 50 of 200, row 25 of 100.
        let Rgb(red, green, blue) = canvas.pixel(100, 50);
        assert_eq!(red, 7);
        assert!((blue as i32 - 50 * 255 / 200).abs() <= 3, "column {blue}");
        assert!((green as i32 - 25 * 255 / 100).abs() <= 3, "row {green}");
        // The frame's top left is still the picture's top left.
        assert_eq!(canvas.pixel(0, 0).2, 0);
    }

    #[test]
    fn a_blackout_covers_the_same_part_of_the_picture_whatever_the_zoom_has_done() {
        // The picture's pixel colours say where they are (blue is the column). A box is blacked out over
        // part of the picture; after zooming, every frame pixel that shows that part must be black, and no pixel
        // that shows something well outside it.
        let mut p = pictures();
        p.by_media.insert("screen.mp4".into(), positional(256, 128, 7));
        let hidden = rect(2000.0, 3000.0, 3000.0, 2500.0);
        let redact = EditItem::Redact {
            id: "r".into(),
            name: "Secret".into(),
            start_ms: 0.0,
            duration_ms: 1000.0,
            rect: hidden,
            color: "#000001".into(),
        };
        for view in [
            rect(0.0, 0.0, 10_000.0, 10_000.0),
            rect(1000.0, 1000.0, 6000.0, 6000.0),
            rect(1234.0, 2345.0, 4321.0, 4321.0),
            rect(3000.0, 2800.0, 2500.0, 2500.0),
        ] {
            let zoom = ZoomView { rect: view, moving: false };
            let (canvas, _) = draw_frame(&recipe(vec![screen()], vec![redact.clone()], zoom), &mut p, (256, 128));
            let mut wrong = 0;
            let (pw, ph) = (view.width / EDIT_FULL / 256.0, view.height / EDIT_FULL / 128.0);
            let (hx0, hy0) = (hidden.x / EDIT_FULL, hidden.y / EDIT_FULL);
            let (hx1, hy1) = ((hidden.x + hidden.width) / EDIT_FULL, (hidden.y + hidden.height) / EDIT_FULL);
            for y in 0..128u32 {
                for x in 0..256u32 {
                    // The part of the whole picture this frame pixel shows, as fractions: from its left edge to its right.
                    let (fx0, fy0) = (view.x / EDIT_FULL + x as f64 * pw, view.y / EDIT_FULL + y as f64 * ph);
                    let (fx1, fy1) = (fx0 + pw, fy0 + ph);
                    // A pixel that touches the hidden part at all must be covered, even by a sliver.
                    let touches = fx0 < hx1 - pw * 1e-6
                        && fx1 > hx0 + pw * 1e-6
                        && fy0 < hy1 - ph * 1e-6
                        && fy1 > hy0 + ph * 1e-6;
                    // One that is a pixel or more away must not be.
                    let away = fx1 < hx0 - pw || fx0 > hx1 + pw || fy1 < hy0 - ph || fy0 > hy1 + ph;
                    let black = canvas.pixel(x, y) == Rgb(0, 0, 1);
                    if (touches && !black) || (away && black) {
                        wrong += 1;
                    }
                }
            }
            assert_eq!(wrong, 0, "zoom {view:?}: {wrong} pixels are wrongly covered or uncovered");
        }
    }

    #[test]
    fn a_blur_and_a_shape_and_an_arrow_are_drawn_where_the_edit_puts_them() {
        let mut p = pictures();
        let mut checker = Picture::solid(100, 100, Rgb(0, 0, 0));
        for y in 0..100u32 {
            for x in 0..100u32 {
                if (x / 2 + y / 2) % 2 == 0 {
                    let i = (y as usize * 100 + x as usize) * 4;
                    checker.bgra[i..i + 3].copy_from_slice(&[255, 255, 255]);
                }
            }
        }
        p.by_media.insert("screen.mp4".into(), Rc::new(checker));
        let items = vec![
            EditItem::Blur {
                id: "b".into(),
                name: "Blur".into(),
                start_ms: 0.0,
                duration_ms: 1.0,
                rect: rect(1000.0, 1000.0, 3000.0, 3000.0),
            },
            EditItem::Shape {
                id: "s".into(),
                name: "Box".into(),
                start_ms: 0.0,
                duration_ms: 1.0,
                shape: EditShapeKind::Rectangle,
                rect: rect(6000.0, 6000.0, 3000.0, 3000.0),
                outline: "#FF0000".into(),
                fill: None,
                line_width_bp: 300.0,
                alt_text: None,
                decorative: Some(true),
            },
            EditItem::Arrow {
                id: "a".into(),
                name: "Arrow".into(),
                start_ms: 0.0,
                duration_ms: 1.0,
                from: EditPoint { x: 5000.0, y: 1000.0 },
                to: EditPoint { x: 9500.0, y: 1000.0 },
                color: "#00FF00".into(),
                line_width_bp: 200.0,
                alt_text: None,
                decorative: Some(true),
            },
        ];
        let (canvas, _) = draw_frame(&recipe(vec![screen()], items, whole_view()), &mut p, (100, 100));
        let grey = canvas.pixel(25, 25).0 as i32;
        assert!((grey - 128).abs() < 40, "the blurred part is grey, not black and white: {grey}");
        assert_eq!(canvas.pixel(60, 75), Rgb(255, 0, 0), "the box's left edge");
        assert_eq!(canvas.pixel(60, 50), canvas.pixel(60, 50), "untouched elsewhere");
        let green = (0..100).filter(|&x| canvas.pixel(x, 10) == Rgb(0, 255, 0)).count();
        assert!(green > 20, "the arrow's shaft is {green} pixels long");
    }

    #[test]
    fn text_is_not_zoomed_and_is_above_the_webcam() {
        let mut p = pictures();
        let title = EditItem::Text {
            id: "t".into(),
            name: "Title".into(),
            start_ms: 0.0,
            duration_ms: 1.0,
            text: "Hello".into(),
            rect: rect(7500.0, 7300.0, 2400.0, 2400.0),
            font_size_bp: 600.0,
            color: "#FFFFFF".into(),
            background: Some("#0000FF".into()),
            bold: false,
            align: EditAlign::Left,
        };
        // Zoomed in, the text box stays where it was written and the webcam does not move.
        let zoom = ZoomView { rect: rect(0.0, 0.0, 5000.0, 5000.0), moving: false };
        let (canvas, _) = draw_frame(&recipe(vec![screen(), webcam(None)], vec![title], zoom), &mut p, (400, 225));
        assert_eq!(canvas.pixel(390, 215), Rgb(0, 0, 255), "the text's background is over the webcam picture");
    }

    #[test]
    fn text_that_does_not_fit_is_reported_so_the_author_can_fix_it() {
        let mut p = pictures();
        let long = EditItem::Text {
            id: "t".into(),
            name: "Long".into(),
            start_ms: 0.0,
            duration_ms: 1.0,
            text: "a great many words that cannot possibly fit in a tiny box".into(),
            rect: rect(0.0, 0.0, 1500.0, 800.0),
            font_size_bp: 600.0,
            color: "#FFFFFF".into(),
            background: None,
            bold: true,
            align: EditAlign::Center,
        };
        let (_, notes) = draw_frame(&recipe(vec![screen()], vec![long], whole_view()), &mut p, (400, 225));
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].contains("\"Long\" does not fit in its box") && notes[0].contains("cannot be read"),
            "{notes:?}"
        );
    }

    #[test]
    fn a_picture_that_cannot_be_had_stops_the_frame_with_the_reason() {
        let mut p = pictures();
        p.by_media.clear();
        let mut canvas = Canvas::new(16, 9);
        let e = compose_frame(
            &recipe(vec![screen()], vec![], whole_view()),
            &mut p,
            &BlockText,
            &mut canvas,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert_eq!(e, "screen.mp4 is missing");
    }

    #[test]
    fn a_colour_that_is_not_a_colour_is_named_not_drawn_wrongly() {
        let mut p = pictures();
        let bad = EditItem::Redact {
            id: "r".into(),
            name: "Secret".into(),
            start_ms: 0.0,
            duration_ms: 1.0,
            rect: rect(0.0, 0.0, 1000.0, 1000.0),
            color: "black".into(),
        };
        let mut canvas = Canvas::new(16, 9);
        let e = compose_frame(
            &recipe(vec![screen()], vec![bad], whole_view()),
            &mut p,
            &BlockText,
            &mut canvas,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(e.contains("The blackout \"Secret\"") && e.contains("not written #RRGGBB"), "{e}");
    }

    #[test]
    fn the_same_frame_drawn_twice_is_the_same_bytes_even_in_the_middle_of_a_zoom() {
        let mut p = pictures();
        p.by_media.insert("screen.mp4".into(), positional(173, 97, 3));
        let zoom = ZoomView { rect: rect(1234.0, 876.0, 6789.0, 6789.0), moving: true };
        let r = recipe(vec![screen(), webcam(None)], vec![], zoom);
        let (a, _) = draw_frame(&r, &mut p, (200, 112));
        let (b, _) = draw_frame(&r, &mut p, (200, 112));
        assert_eq!(a, b);
    }

    /// Not run with the others: says how long frames take to draw at 1080p. Run it with
    /// `cargo test -p openstage-capture-render --release -- --ignored --nocapture how_long`.
    #[test]
    #[ignore]
    fn how_long_a_1080p_frame_takes_to_draw() {
        use std::time::Instant;
        let mut p = pictures();
        p.by_media.insert("screen.mp4".into(), positional(1920, 1080, 7));
        p.by_media.insert("webcam.mp4".into(), positional(1280, 720, 9));
        let overlays = vec![
            EditItem::Blur {
                id: "b".into(),
                name: "Blur".into(),
                start_ms: 0.0,
                duration_ms: 1.0,
                rect: rect(6500.0, 1500.0, 3000.0, 1500.0),
            },
            EditItem::Redact {
                id: "r".into(),
                name: "Secret".into(),
                start_ms: 0.0,
                duration_ms: 1.0,
                rect: rect(6500.0, 3500.0, 3000.0, 1000.0),
                color: "#000000".into(),
            },
        ];
        let mut time = |label: &str, r: FrameRecipe| {
            let mut canvas = Canvas::new(1920, 1080);
            let started = Instant::now();
            let frames = 20;
            for _ in 0..frames {
                compose_frame(&r, &mut p, &BlockText, &mut canvas, &mut Vec::new()).unwrap();
            }
            println!("{label}: {:.1} ms a frame", started.elapsed().as_secs_f64() * 1000.0 / frames as f64);
        };
        time("screen only", recipe(vec![screen()], vec![], whole_view()));
        time("screen, webcam, blur and blackout", recipe(vec![screen(), webcam(None)], overlays.clone(), whole_view()));
        let zoom = ZoomView { rect: rect(1000.0, 1000.0, 6000.0, 6000.0), moving: true };
        time("the same, in the middle of a zoom", recipe(vec![screen(), webcam(None)], overlays, zoom));
    }
}
