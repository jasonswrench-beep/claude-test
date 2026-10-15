// SPDX-License-Identifier: Apache-2.0
//! The Windows side of text on a frame: Windows' own text drawing, so the letters have the computer's fonts.
//!
//! This file only builds on Windows. It has been type-checked for Windows but, until the owner runs
//! `TEST EXPORT.bat`, it has never been run. Treat every line of it as unproven.
//!
//! The text is drawn white on black into a picture of its own the size of the box, with smoothed edges but not
//! ClearType's coloured fringes (those would not survive being laid over a video). How white each pixel came out is
//! how much of the text's colour goes over the frame there. Lines are broken by the layout in the render crate,
//! measured with this font. Scripts that need shaping (Arabic, Hindi and others) are drawn by Windows' text
//! drawing, which does that for text it is given in order; right-to-left paragraphs are not laid out from the right.

use openstage_capture_project::EditAlign;
use openstage_capture_render::{
    line_start, padding, wrap_lines, Canvas, TextFit, TextRenderer, TextRequest, LINE_SPACING,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetTextExtentPoint32W,
    SelectObject, SetBkMode, SetTextColor, TextOutW, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, FF_DONTCARE, HDC, HFONT, OUT_DEFAULT_PRECIS,
    TRANSPARENT,
};

/// Whether the first piece of text has been announced on standard error yet (a diagnosis for the test logs).
static ANNOUNCED: AtomicBool = AtomicBool::new(false);

/// Draws text with Windows' fonts.
pub struct WindowsText;

impl TextRenderer for WindowsText {
    fn draw(&self, canvas: &mut Canvas, request: &TextRequest) -> Result<TextFit, String> {
        if let Some(background) = request.background {
            canvas.fill_rect(request.area, background);
        }
        if request.area.is_empty() || request.text.trim().is_empty() {
            return Ok(TextFit { complete: true });
        }
        let first = !ANNOUNCED.swap(true, Ordering::Relaxed);
        if first {
            eprintln!("export: drawing the first piece of text with Windows' fonts");
        }
        let drawn = unsafe { draw_text(canvas, request) };
        if first {
            eprintln!("export: the first piece of text was drawn");
        }
        drawn
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

unsafe fn width_of(dc: HDC, text: &str) -> f32 {
    let letters = wide(text);
    if letters.is_empty() {
        return 0.0;
    }
    let mut size = SIZE::default();
    if GetTextExtentPoint32W(dc, &letters, &mut size).as_bool() {
        size.cx as f32
    } else {
        text.chars().count() as f32 * 8.0
    }
}

unsafe fn draw_text(canvas: &mut Canvas, request: &TextRequest) -> Result<TextFit, String> {
    let (box_w, box_h) = (request.area.w, request.area.h);
    if box_w > 16_384 || box_h > 16_384 {
        return Err("A text box this large cannot be drawn.".to_string());
    }
    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return Err("Windows would not make a surface to draw text on.".to_string());
    }
    let font_px = request.font_px.round().max(1.0) as i32;
    let font: HFONT = CreateFontW(
        -font_px,
        0,
        0,
        0,
        if request.bold { 700 } else { 400 },
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        ANTIALIASED_QUALITY,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    );
    let old_font = SelectObject(dc, font.into());

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: box_w,
            biHeight: -box_h, // rows from the top
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0);
    let result = match bitmap {
        Ok(bitmap) if !bits.is_null() => {
            let old_bitmap = SelectObject(dc, bitmap.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));

            let pad = padding(request.font_px);
            let inner = (box_w - 2 * pad) as f32;
            let lines = wrap_lines(&request.text, inner, &|s| width_of(dc, s));
            let step = request.font_px * LINE_SPACING;
            let mut complete = true;
            for (i, line) in lines.iter().enumerate() {
                let top = pad + (i as f32 * step).round() as i32;
                if top + font_px > box_h - pad / 2 {
                    complete = false;
                    break;
                }
                let width = width_of(dc, line);
                if width > inner {
                    complete = false;
                }
                let left = line_start(request.align, box_w, pad, width);
                let letters = wide(line);
                if !letters.is_empty() {
                    let _ = TextOutW(dc, left, top, &letters);
                }
            }
            let _ = GdiFlush();

            // How white each pixel came out is how much of the text's colour goes over the frame there.
            let mask = std::slice::from_raw_parts(bits as *const u8, box_w as usize * box_h as usize * 4);
            for y in 0..box_h {
                let cy = request.area.y + y;
                if cy < 0 || cy >= canvas.height as i32 {
                    continue;
                }
                for x in 0..box_w {
                    let cx = request.area.x + x;
                    if cx < 0 || cx >= canvas.width as i32 {
                        continue;
                    }
                    let coverage = mask[(y as usize * box_w as usize + x as usize) * 4 + 1] as u32;
                    if coverage > 0 {
                        canvas.blend_pixel(cx as u32, cy as u32, request.color, coverage);
                    }
                }
            }
            SelectObject(dc, old_bitmap);
            let _ = DeleteObject(bitmap.into());
            Ok(TextFit { complete })
        }
        _ => Err("Windows would not make a surface to draw text on.".to_string()),
    };
    SelectObject(dc, old_font);
    let _ = DeleteObject(font.into());
    let _ = DeleteDC(dc);
    let _ = EditAlign::Left;
    result
}
