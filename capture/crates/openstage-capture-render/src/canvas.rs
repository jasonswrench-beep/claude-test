// SPDX-License-Identifier: Apache-2.0
//! Drawing pictures: the pixels of one frame of the finished video.
//!
//! A [`Canvas`] is a picture the size of the video, blue-green-red-unused bytes, rows from the top. Everything
//! that goes into a frame is drawn on one: the recorded screen (copied or resized), a blurred or blacked-out
//! region, shapes and arrows. Text is drawn by Windows' own text drawing (see `text.rs`), because it has the
//! fonts and knows how to lay out every language.
//!
//! **Every drawing step uses only whole numbers or the arithmetic every computer does the same way (add,
//! subtract, multiply, divide, square root).** A frame drawn twice from the same pictures is the same bytes,
//! and a video exported twice is the same video (story B3). Nothing here depends on the computer it runs on,
//! so all of it is tested on any computer, pixel by pixel.

/// A colour: red, green, blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Read `#RRGGBB` (capital or small letters).
    pub fn parse(text: &str) -> Result<Rgb, String> {
        let digits = text.strip_prefix('#').ok_or_else(|| format!("The colour \"{text}\" is not written #RRGGBB."))?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("The colour \"{text}\" is not written #RRGGBB."));
        }
        let part = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).unwrap_or(0);
        Ok(Rgb(part(0), part(2), part(4)))
    }
}

/// A rectangle of pixels. It may reach beyond the canvas; drawing is cut off at the edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PixelRect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    /// The part of this rectangle that lies inside `0..width` by `0..height`.
    pub fn clipped(&self, width: u32, height: u32) -> PixelRect {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = (self.x + self.w).min(width as i32);
        let y1 = (self.y + self.h).min(height as i32);
        PixelRect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }
}

/// A rectangle in a picture's own pixels, which may have fractions (a zoom reaches into the middle of pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A picture to draw from: blue, green, red, then how solid each pixel is (255 is fully solid, 0 is clear, as in a
/// logo with a see-through background), rows from the top, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    /// Whether every pixel is fully solid, which lets a picture be copied row by row. Found by looking at it.
    opaque: bool,
}

impl Picture {
    /// A picture from its bytes. Looks at every pixel once to see whether any is see-through.
    pub fn new(width: u32, height: u32, bgra: Vec<u8>) -> Self {
        let opaque = bgra.chunks_exact(4).all(|p| p[3] == 255);
        Self { width, height, bgra, opaque }
    }

    /// A picture whose pixels the caller knows are all fully solid (a frame of a video), so there is nothing to look at.
    pub fn solid_pixels(width: u32, height: u32, bgra: Vec<u8>) -> Self {
        Self { width, height, bgra, opaque: true }
    }

    pub fn solid(width: u32, height: u32, color: Rgb) -> Self {
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            bgra.extend_from_slice(&[color.2, color.1, color.0, 255]);
        }
        Self { width, height, bgra, opaque: true }
    }

    /// The pixel at (`x`, `y`) with how solid it is, clamped to the picture.
    fn at4(&self, x: i64, y: i64) -> [u8; 4] {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        let i = (y * self.width as usize + x) * 4;
        [self.bgra[i], self.bgra[i + 1], self.bgra[i + 2], self.bgra[i + 3]]
    }

    fn at(&self, x: i64, y: i64) -> [u8; 3] {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        let i = (y * self.width as usize + x) * 4;
        [self.bgra[i], self.bgra[i + 1], self.bgra[i + 2]]
    }
}

/// One frame of the finished video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let mut canvas = Self { width, height, bgra: vec![0; width as usize * height as usize * 4] };
        canvas.clear(Rgb(0, 0, 0));
        canvas
    }

    pub fn clear(&mut self, color: Rgb) {
        for pixel in self.bgra.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[color.2, color.1, color.0, 255]);
        }
    }

    /// The colour of a pixel (red, green, blue). Panics outside the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Rgb(self.bgra[i + 2], self.bgra[i + 1], self.bgra[i])
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgb) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.bgra[i..i + 4].copy_from_slice(&[color.2, color.1, color.0, 255]);
    }

    /// Lay `color` over the pixel at (`x`, `y`) with the given share (0 to 255) of it: 255 replaces the pixel, 0
    /// leaves it. Used for the smoothed edges of letters. Panics outside the canvas.
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: Rgb, share: u32) {
        self.blend(x, y, color, share);
    }

    /// Lay `color` over a pixel with the given share (0 to 255) of it: 255 replaces the pixel, 0 leaves it.
    fn blend(&mut self, x: u32, y: u32, color: Rgb, share: u32) {
        if share == 0 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let mix = |old: u8, new: u8| -> u8 { ((old as u32 * (255 - share) + new as u32 * share + 127) / 255) as u8 };
        self.bgra[i] = mix(self.bgra[i], color.2);
        self.bgra[i + 1] = mix(self.bgra[i + 1], color.1);
        self.bgra[i + 2] = mix(self.bgra[i + 2], color.0);
        self.bgra[i + 3] = 255;
    }

    /// Cover a rectangle with a solid colour.
    pub fn fill_rect(&mut self, rect: PixelRect, color: Rgb) {
        let r = rect.clipped(self.width, self.height);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.set_pixel(x as u32, y as u32, color);
            }
        }
    }

    /// Draw the edge of a rectangle, `line` pixels thick, inside the rectangle.
    pub fn stroke_rect(&mut self, rect: PixelRect, line: i32, color: Rgb) {
        let line = line.max(1).min(rect.w / 2 + 1).min(rect.h / 2 + 1);
        self.fill_rect(PixelRect::new(rect.x, rect.y, rect.w, line), color);
        self.fill_rect(PixelRect::new(rect.x, rect.y + rect.h - line, rect.w, line), color);
        self.fill_rect(PixelRect::new(rect.x, rect.y + line, line, rect.h - 2 * line), color);
        self.fill_rect(PixelRect::new(rect.x + rect.w - line, rect.y + line, line, rect.h - 2 * line), color);
    }

    /// Draw `picture` (the part `source` of it) into `target`, resized to fit. A picture that is made
    /// smaller is averaged, so thin lines and small text stay readable; one that is made larger is smoothed.
    /// Whatever falls outside the canvas is not drawn.
    pub fn draw_picture(&mut self, picture: &Picture, source: SourceRect, target: PixelRect) {
        if picture.width == 0 || picture.height == 0 || target.is_empty() || source.w <= 0.0 || source.h <= 0.0 {
            return;
        }
        let clip = target.clipped(self.width, self.height);
        if clip.is_empty() {
            return;
        }
        let (sx, sy) = (source.w / target.w as f64, source.h / target.h as f64);
        // The common case, a copy: same size, whole pixels.
        let whole = sx == 1.0 && sy == 1.0 && source.x.fract() == 0.0 && source.y.fract() == 0.0;
        let inside = source.x >= 0.0
            && source.y >= 0.0
            && source.x + source.w <= picture.width as f64
            && source.y + source.h <= picture.height as f64;
        if whole && inside && picture.opaque {
            let (ox, oy) = (source.x as i64 - target.x as i64, source.y as i64 - target.y as i64);
            for y in clip.y..clip.y + clip.h {
                let from = (((y as i64 + oy) as usize) * picture.width as usize + (clip.x as i64 + ox) as usize) * 4;
                let to = (y as usize * self.width as usize + clip.x as usize) * 4;
                let n = clip.w as usize * 4;
                self.bgra[to..to + n].copy_from_slice(&picture.bgra[from..from + n]);
                for a in self.bgra[to..to + n].chunks_exact_mut(4) {
                    a[3] = 255;
                }
            }
            return;
        }
        self.draw_picture_to(
            picture,
            source,
            SourceRect { x: target.x as f64, y: target.y as f64, w: target.w as f64, h: target.h as f64 },
        );
    }

    /// The same, into a rectangle that may start and end in the middle of pixels (a zoom in the middle of its move).
    /// A pixel that is only partly inside the rectangle is drawn whole.
    pub fn draw_picture_to(&mut self, picture: &Picture, source: SourceRect, dest: SourceRect) {
        if picture.width == 0
            || picture.height == 0
            || dest.w <= 0.0
            || dest.h <= 0.0
            || source.w <= 0.0
            || source.h <= 0.0
        {
            return;
        }
        let x0 = dest.x.floor().max(0.0) as i32;
        let y0 = dest.y.floor().max(0.0) as i32;
        let x1 = (dest.x + dest.w).ceil().min(self.width as f64) as i32;
        let y1 = (dest.y + dest.h).ceil().min(self.height as f64) as i32;
        let (sx, sy) = (source.w / dest.w, source.h / dest.h);
        let smoothing = sx <= 1.0 && sy <= 1.0;
        if !picture.opaque {
            // A picture with see-through parts (a logo): each pixel is laid over what is already there by how solid it is.
            for y in y0..y1 {
                for x in x0..x1 {
                    let (u0, v0) = (source.x + (x as f64 - dest.x) * sx, source.y + (y as f64 - dest.y) * sy);
                    let ([b, g, r], solid) = if smoothing {
                        bilinear_a(picture, u0 + sx / 2.0, v0 + sy / 2.0)
                    } else {
                        average_a(picture, u0, v0, sx, sy)
                    };
                    self.blend(x as u32, y as u32, Rgb(r, g, b), solid as u32);
                }
            }
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let (u0, v0) = (source.x + (x as f64 - dest.x) * sx, source.y + (y as f64 - dest.y) * sy);
                let [b, g, r] = if smoothing {
                    bilinear(picture, u0 + sx / 2.0, v0 + sy / 2.0)
                } else {
                    average(picture, u0, v0, sx, sy)
                };
                self.set_pixel(x as u32, y as u32, Rgb(r, g, b));
            }
        }
    }

    /// Blur a rectangle so strongly that small text in it cannot be read: three passes of a box blur whose
    /// size grows with the rectangle. Blur is for hiding detail, not secrets: a password or a name that must
    /// not be recovered should be blacked out, not blurred.
    pub fn blur(&mut self, rect: PixelRect) {
        let r = rect.clipped(self.width, self.height);
        if r.is_empty() {
            return;
        }
        let radius = (r.w.min(r.h) / 8).clamp(6, 48);
        let (w, h) = (r.w as usize, r.h as usize);
        let mut block: Vec<[u8; 3]> = Vec::with_capacity(w * h);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                block.push([self.bgra[i], self.bgra[i + 1], self.bgra[i + 2]]);
            }
        }
        for _ in 0..3 {
            block = blur_pass(&block, w, h, radius as usize, true);
            block = blur_pass(&block, w, h, radius as usize, false);
        }
        let mut k = 0;
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                self.bgra[i] = block[k][0];
                self.bgra[i + 1] = block[k][1];
                self.bgra[i + 2] = block[k][2];
                self.bgra[i + 3] = 255;
                k += 1;
            }
        }
    }

    /// Draw an ellipse that fills `rect`: its edge `line` pixels thick (inside the rectangle), and the inside
    /// filled with `fill` if there is one. The edge is smoothed.
    pub fn ellipse(&mut self, rect: PixelRect, line: f32, outline: Rgb, fill: Option<Rgb>) {
        if rect.is_empty() {
            return;
        }
        let (a, b) = (rect.w as f32 / 2.0, rect.h as f32 / 2.0);
        let (cx, cy) = (rect.x as f32 + a, rect.y as f32 + b);
        let line = line.max(1.0).min(a).min(b);
        let area = PixelRect::new(rect.x - 1, rect.y - 1, rect.w + 2, rect.h + 2).clipped(self.width, self.height);
        for y in area.y..area.y + area.h {
            for x in area.x..area.x + area.w {
                let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let outer = ellipse_cover(px, py, a, b);
                if outer == 0.0 {
                    continue;
                }
                let inner = ellipse_cover(px, py, a - line, b - line);
                if let Some(fill) = fill {
                    self.blend(x as u32, y as u32, fill, share(outer));
                    self.blend(x as u32, y as u32, outline, share((outer - inner).max(0.0)));
                } else {
                    self.blend(x as u32, y as u32, outline, share((outer - inner).max(0.0)));
                }
            }
        }
    }

    /// Draw a straight line `width` pixels thick, with smoothed edges and square ends.
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: Rgb) {
        let half = width.max(1.0) / 2.0;
        let pad = half + 1.0;
        let x0 = (from.0.min(to.0) - pad).floor() as i32;
        let y0 = (from.1.min(to.1) - pad).floor() as i32;
        let x1 = (from.0.max(to.0) + pad).ceil() as i32;
        let y1 = (from.1.max(to.1) + pad).ceil() as i32;
        let area = PixelRect::new(x0, y0, x1 - x0, y1 - y0).clipped(self.width, self.height);
        for y in area.y..area.y + area.h {
            for x in area.x..area.x + area.w {
                let d = distance_to_segment((x as f32 + 0.5, y as f32 + 0.5), from, to);
                self.blend(x as u32, y as u32, color, share(half + 0.5 - d));
            }
        }
    }

    /// Draw an arrow from `from` to `to`: a line with a head at `to`.
    pub fn arrow(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: Rgb) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length < 1.0 {
            return;
        }
        let (ux, uy) = (dx / length, dy / length);
        let width = width.max(1.0);
        // The head is as long as four lines are thick, but never less than 12 pixels or more than half the arrow.
        let head = (width * 4.0).max(12.0).min(length * 0.5);
        let base = (to.0 - ux * head, to.1 - uy * head);
        // The line stops inside the head, so its square end cannot poke out of the sides.
        self.line(from, (to.0 - ux * head * 0.8, to.1 - uy * head * 0.8), width, color);
        let (nx, ny) = (-uy, ux);
        let wing = head * 0.5;
        let corners = [to, (base.0 + nx * wing, base.1 + ny * wing), (base.0 - nx * wing, base.1 - ny * wing)];
        let x0 = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor() as i32 - 1;
        let y0 = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor() as i32 - 1;
        let x1 = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil() as i32 + 1;
        let y1 = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil() as i32 + 1;
        let area = PixelRect::new(x0, y0, x1 - x0, y1 - y0).clipped(self.width, self.height);
        for y in area.y..area.y + area.h {
            for x in area.x..area.x + area.w {
                let depth = inside_triangle((x as f32 + 0.5, y as f32 + 0.5), corners);
                self.blend(x as u32, y as u32, color, share(depth + 0.5));
            }
        }
    }
}

/// A coverage from 0.0 to 1.0 as a share of 255.
fn share(coverage: f32) -> u32 {
    (coverage.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// How much of the pixel at (`px`, `py`) from the centre lies inside an ellipse with radii `a` and `b`, as
/// 0.0 (outside) to 1.0 (inside). The edge is found as the distance from the pixel's centre to the curve,
/// estimated from how fast the ellipse's equation changes there.
fn ellipse_cover(px: f32, py: f32, a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return 0.0;
    }
    let f = (px / a) * (px / a) + (py / b) * (py / b) - 1.0;
    let gx = 2.0 * px / (a * a);
    let gy = 2.0 * py / (b * b);
    let slope = (gx * gx + gy * gy).sqrt();
    if slope < 1e-6 {
        return if f < 0.0 { 1.0 } else { 0.0 };
    }
    (0.5 - f / slope).clamp(0.0, 1.0)
}

fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let (apx, apy) = (p.0 - a.0, p.1 - a.1);
    let length2 = abx * abx + aby * aby;
    let t = if length2 <= 0.0 { 0.0 } else { ((apx * abx + apy * aby) / length2).clamp(0.0, 1.0) };
    let (qx, qy) = (a.0 + abx * t, a.1 + aby * t);
    ((p.0 - qx) * (p.0 - qx) + (p.1 - qy) * (p.1 - qy)).sqrt()
}

/// How far inside a triangle a point is (negative outside): the distance to the nearest edge.
fn inside_triangle(p: (f32, f32), c: [(f32, f32); 3]) -> f32 {
    // Make the corners run the same way round whichever way they were given.
    let area = (c[1].0 - c[0].0) * (c[2].1 - c[0].1) - (c[2].0 - c[0].0) * (c[1].1 - c[0].1);
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut nearest = f32::MAX;
    for i in 0..3 {
        let (a, b) = (c[i], c[(i + 1) % 3]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let length = (ex * ex + ey * ey).sqrt().max(1e-6);
        let side = sign * (ex * (p.1 - a.1) - ey * (p.0 - a.0)) / length;
        nearest = nearest.min(side);
    }
    nearest
}

fn bilinear(picture: &Picture, u: f64, v: f64) -> [u8; 3] {
    let (u, v) = (u - 0.5, v - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = ((u - x0) as f32, (v - y0) as f32);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let taps = [picture.at(x0, y0), picture.at(x0 + 1, y0), picture.at(x0, y0 + 1), picture.at(x0 + 1, y0 + 1)];
    let weights = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let mut out = [0u8; 3];
    for (channel, slot) in out.iter_mut().enumerate() {
        let sum: f32 = taps.iter().zip(weights).map(|(t, w)| t[channel] as f32 * w).sum();
        *slot = (sum + 0.5).clamp(0.0, 255.0) as u8;
    }
    out
}

/// The average colour of the part of `picture` from (`u`, `v`) that is `w` wide and `h` high, counting
/// partly covered pixels by how much of them is covered.
fn average(picture: &Picture, u: f64, v: f64, w: f64, h: f64) -> [u8; 3] {
    let (x_end, y_end) = (u + w, v + h);
    let mut sum = [0.0f64; 3];
    let mut weight = 0.0f64;
    let mut y = v.floor();
    while y < y_end {
        let wy = (y + 1.0).min(y_end) - y.max(v);
        let mut x = u.floor();
        while x < x_end {
            let wx = (x + 1.0).min(x_end) - x.max(u);
            let pixel = picture.at(x as i64, y as i64);
            let k = wx * wy;
            for c in 0..3 {
                sum[c] += pixel[c] as f64 * k;
            }
            weight += k;
            x += 1.0;
        }
        y += 1.0;
    }
    if weight <= 0.0 {
        return [0; 3];
    }
    [(sum[0] / weight + 0.5) as u8, (sum[1] / weight + 0.5) as u8, (sum[2] / weight + 0.5) as u8]
}

/// Like [`bilinear`] for a picture with see-through parts: colours are mixed by how solid they are, so the
/// clear pixels at an edge do not dim it. Gives the colour and how solid the result is (0 to 255).
fn bilinear_a(picture: &Picture, u: f64, v: f64) -> ([u8; 3], u8) {
    let (u, v) = (u - 0.5, v - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = ((u - x0) as f32, (v - y0) as f32);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let taps = [picture.at4(x0, y0), picture.at4(x0 + 1, y0), picture.at4(x0, y0 + 1), picture.at4(x0 + 1, y0 + 1)];
    let weights = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let mut colour = [0.0f32; 3];
    let mut solid = 0.0f32;
    for (t, w) in taps.iter().zip(weights) {
        let a = t[3] as f32 / 255.0 * w;
        solid += a;
        for c in 0..3 {
            colour[c] += t[c] as f32 * a;
        }
    }
    if solid <= 0.0 {
        return ([0; 3], 0);
    }
    (
        [(colour[0] / solid + 0.5) as u8, (colour[1] / solid + 0.5) as u8, (colour[2] / solid + 0.5) as u8],
        (solid * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
    )
}

/// Like [`average`] for a picture with see-through parts.
fn average_a(picture: &Picture, u: f64, v: f64, w: f64, h: f64) -> ([u8; 3], u8) {
    let (x_end, y_end) = (u + w, v + h);
    let mut colour = [0.0f64; 3];
    let mut solid = 0.0f64;
    let mut weight = 0.0f64;
    let mut y = v.floor();
    while y < y_end {
        let wy = (y + 1.0).min(y_end) - y.max(v);
        let mut x = u.floor();
        while x < x_end {
            let wx = (x + 1.0).min(x_end) - x.max(u);
            let pixel = picture.at4(x as i64, y as i64);
            let a = pixel[3] as f64 / 255.0 * wx * wy;
            solid += a;
            weight += wx * wy;
            for c in 0..3 {
                colour[c] += pixel[c] as f64 * a;
            }
            x += 1.0;
        }
        y += 1.0;
    }
    if solid <= 0.0 || weight <= 0.0 {
        return ([0; 3], 0);
    }
    (
        [(colour[0] / solid + 0.5) as u8, (colour[1] / solid + 0.5) as u8, (colour[2] / solid + 0.5) as u8],
        (solid / weight * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
    )
}

/// One pass of a box blur along rows (`across`) or columns, of window `2 * radius + 1`, with the edge pixel
/// repeated beyond the edge. Whole-number sums, so the result is the same everywhere.
fn blur_pass(block: &[[u8; 3]], w: usize, h: usize, radius: usize, across: bool) -> Vec<[u8; 3]> {
    let mut out = vec![[0u8; 3]; block.len()];
    let (lines, length) = if across { (h, w) } else { (w, h) };
    let index = |line: usize, i: usize| if across { line * w + i } else { i * w + line };
    let window = (2 * radius + 1) as u32;
    for line in 0..lines {
        let at = |i: i64| block[index(line, i.clamp(0, length as i64 - 1) as usize)];
        let mut sum = [0u32; 3];
        for i in -(radius as i64)..=(radius as i64) {
            let p = at(i);
            for c in 0..3 {
                sum[c] += p[c] as u32;
            }
        }
        for i in 0..length {
            let mut p = [0u8; 3];
            for c in 0..3 {
                p[c] = ((sum[c] + window / 2) / window) as u8;
            }
            out[index(line, i)] = p;
            let leaving = at(i as i64 - radius as i64);
            let entering = at(i as i64 + radius as i64 + 1);
            for c in 0..3 {
                sum[c] = sum[c] + entering[c] as u32 - leaving[c] as u32;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb(255, 0, 0);
    const WHITE: Rgb = Rgb(255, 255, 255);
    const BLACK: Rgb = Rgb(0, 0, 0);

    #[test]
    fn colours_are_read_as_red_green_blue() {
        assert_eq!(Rgb::parse("#FF8000").unwrap(), Rgb(255, 128, 0));
        assert_eq!(Rgb::parse("#ff8000").unwrap(), Rgb(255, 128, 0));
        for bad in ["FF8000", "#FF80", "#GG0000", "", "#FF80001"] {
            assert!(Rgb::parse(bad).unwrap_err().contains("not written #RRGGBB"), "{bad}");
        }
    }

    #[test]
    fn a_rectangle_is_cut_off_at_the_edges_of_the_canvas() {
        let r = PixelRect::new(-5, 3, 10, 100).clipped(8, 8);
        assert_eq!(r, PixelRect::new(0, 3, 5, 5));
        assert!(PixelRect::new(20, 20, 4, 4).clipped(8, 8).is_empty());
        assert!(PixelRect::new(0, 0, 0, 5).is_empty());
    }

    #[test]
    fn filling_and_outlining_a_rectangle_colours_exactly_the_pixels_asked_for() {
        let mut c = Canvas::new(10, 10);
        c.fill_rect(PixelRect::new(2, 2, 4, 3), RED);
        let red = (0..10).flat_map(|y| (0..10).map(move |x| (x, y))).filter(|&(x, y)| c.pixel(x, y) == RED).count();
        assert_eq!(red, 12);
        let mut c = Canvas::new(10, 10);
        c.stroke_rect(PixelRect::new(1, 1, 8, 8), 2, RED);
        assert_eq!(c.pixel(1, 1), RED);
        assert_eq!(c.pixel(2, 4), RED, "the left edge is 2 thick");
        assert_eq!(c.pixel(3, 4), BLACK, "inside is untouched");
        assert_eq!(c.pixel(8, 8), RED);
        assert_eq!(c.pixel(0, 0), BLACK, "outside is untouched");
        // Drawing beyond the canvas does not fail.
        c.fill_rect(PixelRect::new(-50, -50, 500, 500), WHITE);
        assert_eq!(c.pixel(9, 9), WHITE);
    }

    fn gradient(width: u32, height: u32) -> Picture {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgra.extend_from_slice(&[x as u8, y as u8, (x + y) as u8, 255]);
            }
        }
        Picture::new(width, height, bgra)
    }

    #[test]
    fn a_picture_copied_at_its_own_size_is_copied_exactly() {
        let p = gradient(16, 8);
        let mut c = Canvas::new(16, 8);
        c.draw_picture(&p, SourceRect { x: 0.0, y: 0.0, w: 16.0, h: 8.0 }, PixelRect::new(0, 0, 16, 8));
        assert_eq!(c.bgra, p.bgra);
        // A piece of it, put somewhere else, and partly off the canvas.
        let mut c = Canvas::new(16, 8);
        c.draw_picture(&p, SourceRect { x: 4.0, y: 2.0, w: 8.0, h: 4.0 }, PixelRect::new(12, 6, 8, 4));
        assert_eq!(c.pixel(12, 6), Rgb(2 + 4, 2, 4), "the source's pixel (4, 2) lands at (12, 6)");
        assert_eq!(c.pixel(15, 7), Rgb(7 + 3, 3, 7), "and the canvas ends at the edge: source pixel (7, 3)");
        assert_eq!(c.pixel(0, 0), BLACK);
    }

    #[test]
    fn a_picture_made_smaller_is_averaged_not_sampled() {
        // Black and white stripes one pixel wide, shown at half size: every pixel is grey, none is black or white.
        let mut bgra = Vec::new();
        for _y in 0..4 {
            for x in 0..8 {
                let v = if x % 2 == 0 { 0 } else { 255 };
                bgra.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let stripes = Picture::new(8, 4, bgra);
        let mut c = Canvas::new(4, 2);
        c.draw_picture(&stripes, SourceRect { x: 0.0, y: 0.0, w: 8.0, h: 4.0 }, PixelRect::new(0, 0, 4, 2));
        for y in 0..2 {
            for x in 0..4 {
                let Rgb(r, g, b) = c.pixel(x, y);
                assert!((127..=128).contains(&r) && r == g && g == b, "({x}, {y}) is {r}");
            }
        }
    }

    #[test]
    fn a_picture_made_larger_is_smooth_and_keeps_its_corners() {
        let two = Picture::new(2, 1, vec![0, 0, 0, 255, 200, 200, 200, 255]);
        let mut c = Canvas::new(8, 1);
        c.draw_picture(&two, SourceRect { x: 0.0, y: 0.0, w: 2.0, h: 1.0 }, PixelRect::new(0, 0, 8, 1));
        let values: Vec<u8> = (0..8).map(|x| c.pixel(x, 0).0).collect();
        assert_eq!(values[0], 0);
        assert_eq!(values[7], 200);
        assert!(values.windows(2).all(|w| w[0] <= w[1]), "it only brightens from left to right: {values:?}");
        assert!(values[3] > 0 && values[4] < 200, "the middle is a blend: {values:?}");
    }

    #[test]
    fn zooming_into_the_middle_of_a_picture_shows_just_that_part_larger() {
        let p = gradient(16, 16);
        let mut c = Canvas::new(8, 8);
        c.draw_picture(&p, SourceRect { x: 4.0, y: 4.0, w: 8.0, h: 8.0 }, PixelRect::new(0, 0, 8, 8));
        // The canvas now holds the middle 8 by 8 of the picture at its own size.
        assert_eq!(c.pixel(0, 0), p_pixel(&p, 4, 4));
        assert_eq!(c.pixel(7, 7), p_pixel(&p, 11, 11));
    }

    fn p_pixel(p: &Picture, x: u32, y: u32) -> Rgb {
        let i = (y as usize * p.width as usize + x as usize) * 4;
        Rgb(p.bgra[i + 2], p.bgra[i + 1], p.bgra[i])
    }

    #[test]
    fn the_see_through_parts_of_a_picture_show_what_is_behind_and_half_clear_parts_blend() {
        // A logo: a solid red pixel, a clear pixel, and a half-clear white one.
        let logo = Picture::new(3, 1, vec![0, 0, 255, 255, 9, 9, 9, 0, 255, 255, 255, 128]);
        let mut c = Canvas::new(3, 1);
        c.clear(Rgb(0, 0, 100));
        c.draw_picture(&logo, SourceRect { x: 0.0, y: 0.0, w: 3.0, h: 1.0 }, PixelRect::new(0, 0, 3, 1));
        assert_eq!(c.pixel(0, 0), RED, "a solid pixel replaces what is behind");
        assert_eq!(c.pixel(1, 0), Rgb(0, 0, 100), "a clear pixel leaves what is behind");
        let Rgb(r, g, b) = c.pixel(2, 0);
        assert!(
            (r as i32 - 128).abs() <= 1 && (g as i32 - 128).abs() <= 1 && (b as i32 - 177).abs() <= 1,
            "{r} {g} {b}: half white over dark blue"
        );
        // Made larger, the edge of a solid shape on a clear background is not dimmed by the clear pixels.
        let dot = Picture::new(2, 1, vec![0, 0, 255, 255, 0, 0, 0, 0]);
        let mut c = Canvas::new(8, 1);
        c.clear(Rgb(0, 0, 0));
        c.draw_picture(&dot, SourceRect { x: 0.0, y: 0.0, w: 2.0, h: 1.0 }, PixelRect::new(0, 0, 8, 1));
        assert_eq!(c.pixel(0, 0), RED);
        assert!(
            c.pixel(3, 0).0 > 100 && c.pixel(3, 0).1 == 0,
            "the colour stays red where it fades: {:?}",
            c.pixel(3, 0)
        );
    }

    #[test]
    fn drawing_the_same_thing_twice_gives_the_same_bytes() {
        let p = gradient(37, 23);
        let draw = || {
            let mut c = Canvas::new(61, 29);
            c.draw_picture(&p, SourceRect { x: 3.3, y: 1.7, w: 30.1, h: 19.3 }, PixelRect::new(-4, 2, 61, 33));
            c.blur(PixelRect::new(10, 5, 30, 20));
            c.ellipse(PixelRect::new(5, 3, 31, 17), 3.0, RED, Some(WHITE));
            c.arrow((2.0, 25.0), (50.0, 6.0), 3.0, RED);
            c
        };
        assert_eq!(draw(), draw());
    }

    #[test]
    fn a_blur_leaves_nothing_of_fine_detail_and_stays_inside_its_rectangle() {
        // A checkerboard of 2-pixel squares: the smallest print there is.
        let mut c = Canvas::new(80, 60);
        for y in 0..60 {
            for x in 0..80 {
                c.set_pixel(x, y, if (x / 2 + y / 2) % 2 == 0 { BLACK } else { WHITE });
            }
        }
        let before = c.clone();
        c.blur(PixelRect::new(20, 10, 40, 40));
        // Inside, every pixel is close to the middle grey; outside nothing changed.
        let mut worst = 0i32;
        for y in 10..50 {
            for x in 20..60 {
                worst = worst.max((c.pixel(x, y).0 as i32 - 128).abs());
            }
        }
        assert!(worst <= 20, "the checkerboard is {worst} away from grey after blurring");
        assert_eq!(c.pixel(19, 20), before.pixel(19, 20));
        assert_eq!(c.pixel(60, 20), before.pixel(60, 20));
        assert_eq!(c.pixel(30, 9), before.pixel(30, 9));
        assert_eq!(c.pixel(30, 50), before.pixel(30, 50));
    }

    #[test]
    fn a_blur_keeps_a_flat_colour_flat() {
        let mut c = Canvas::new(30, 30);
        c.clear(Rgb(40, 90, 200));
        c.blur(PixelRect::new(5, 5, 20, 20));
        assert!((0..30).all(|y| (0..30).all(|x| c.pixel(x, y) == Rgb(40, 90, 200))));
    }

    #[test]
    fn an_ellipse_fills_its_middle_and_leaves_its_corners_alone() {
        let mut c = Canvas::new(40, 20);
        c.ellipse(PixelRect::new(0, 0, 40, 20), 2.0, RED, Some(WHITE));
        assert_eq!(c.pixel(20, 10), WHITE, "the middle is filled");
        assert_eq!(c.pixel(0, 0), BLACK, "the corner of the rectangle is outside the ellipse");
        assert_eq!(c.pixel(39, 19), BLACK);
        assert_eq!(c.pixel(1, 10), RED, "the left edge is the outline");
        assert_eq!(c.pixel(20, 0), RED, "and the top");
        // Without a fill the middle is untouched.
        let mut c = Canvas::new(40, 20);
        c.ellipse(PixelRect::new(0, 0, 40, 20), 2.0, RED, None);
        assert_eq!(c.pixel(20, 10), BLACK);
        assert_eq!(c.pixel(20, 0), RED);
    }

    #[test]
    fn a_line_is_as_thick_as_asked_and_smooth_at_its_edge() {
        let mut c = Canvas::new(30, 12);
        c.line((2.0, 6.0), (28.0, 6.0), 4.0, WHITE);
        let column: Vec<u8> = (0..12).map(|y| c.pixel(15, y).0).collect();
        assert_eq!(&column[4..8], &[255, 255, 255, 255], "{column:?}");
        assert_eq!((column[3], column[8]), (0, 0), "{column:?}");
        // A slanted line has in-between greys along its edge.
        let mut d = Canvas::new(30, 30);
        d.line((2.0, 2.0), (28.0, 20.0), 3.0, WHITE);
        let greys = (0..30).flat_map(|y| (0..30).map(move |x| (x, y))).filter(|&(x, y)| {
            let v = d.pixel(x, y).0;
            v > 10 && v < 245
        });
        assert!(greys.count() > 20);
    }

    #[test]
    fn an_arrow_has_a_head_at_the_end_it_points_to() {
        let mut c = Canvas::new(60, 30);
        c.arrow((4.0, 15.5), (56.0, 15.5), 2.0, RED);
        assert_eq!(c.pixel(54, 15), RED, "near the tip");
        assert_eq!(c.pixel(30, 15), RED, "the shaft");
        // The head is wider than the shaft; the start is not.
        let thickness = |x: u32| (0..30).filter(|&y| c.pixel(x, y).0 > 128).count();
        assert!(thickness(48) > thickness(20) + 4, "head {} shaft {}", thickness(48), thickness(20));
        assert_eq!(c.pixel(57, 15), BLACK, "nothing beyond the tip");
        // An arrow that starts where it ends draws nothing and does not fail.
        let mut d = Canvas::new(10, 10);
        d.arrow((5.0, 5.0), (5.0, 5.0), 2.0, RED);
        assert_eq!(d, Canvas::new(10, 10));
    }

    #[test]
    fn an_arrow_pointing_up_has_its_head_at_the_top() {
        let mut c = Canvas::new(30, 60);
        c.arrow((15.5, 56.0), (15.5, 4.0), 2.0, RED);
        assert_eq!(c.pixel(15, 6), RED);
        let thickness = |y: u32| (0..30).filter(|&x| c.pixel(x, y).0 > 128).count();
        assert!(thickness(12) > thickness(40) + 4);
    }
}
