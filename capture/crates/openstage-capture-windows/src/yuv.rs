// SPDX-License-Identifier: Apache-2.0
//! Turning the pictures Windows' H.264 decoder produces into plain blue-green-red pixels, and cutting them to size.
//!
//! Recorded videos are read back when an edit is exported. There are two ways to ask Windows for the pictures:
//!
//! * [`PictureReader::Rgb32`]: Windows converts the colours itself (its "video processing"), and this program receives
//!   plain pixels. This is the way tried first.
//! * [`PictureReader::Nv12`]: Windows only decodes, and this program converts the colours itself with [`nv12_to_bgra`].
//!   It exists because on the first Windows run the first way stopped the program inside Windows' own file reader, and
//!   nothing in this program's own code could be found at fault. It is chosen with the hidden option
//!   `--picture-reader nv12`, which `TEST EXPORT.bat` uses by itself if the first way makes the program stop.
//!
//! Either way the picture must be cut to the video's real size. H.264 pictures are coded in blocks of 16, so a 1080 line
//! video is decoded as 1088 lines; the last 8 are padding that must not be shown (the first Windows run exported a
//! 1920 by 1088 video because of this).
//!
//! Everything here is plain arithmetic, the same on every computer, and is tested without Windows.

/// How the recorded videos are read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PictureReader {
    /// Windows decodes and converts the colours.
    #[default]
    Rgb32,
    /// Windows only decodes; this program converts the colours.
    Nv12,
}

impl PictureReader {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "rgb32" => Ok(PictureReader::Rgb32),
            "nv12" => Ok(PictureReader::Nv12),
            other => Err(format!("--picture-reader must be rgb32 or nv12, not \"{other}\".")),
        }
    }
}

/// Which formula turns brightness and colour differences into red, green and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matrix {
    /// Used for high definition video (720 lines and up).
    Bt709,
    /// Used for standard definition video.
    Bt601,
}

impl Matrix {
    /// The matrix to use when the video does not say: 709 from 720 lines up, 601 below.
    pub fn assumed_for(height: u32) -> Self {
        if height >= 720 {
            Matrix::Bt709
        } else {
            Matrix::Bt601
        }
    }

    /// Windows' own number for a matrix (`MFVideoTransferMatrix`): 1 is BT.709 and 2 is BT.601. `None` for any other.
    pub fn from_windows(value: u32) -> Option<Self> {
        match value {
            1 => Some(Matrix::Bt709),
            2 => Some(Matrix::Bt601),
            _ => None,
        }
    }
}

fn clip(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// One pixel: `y`, `u` (blue difference) and `v` (red difference) as stored; the result as blue, green, red.
fn pixel(y: u8, u: u8, v: u8, matrix: Matrix, full_range: bool) -> [u8; 3] {
    let (cb, cr) = (u as i32 - 128, v as i32 - 128);
    // Coefficients are the real ones times 256.
    let (luma, r_cr, g_cb, g_cr, b_cb) = match (matrix, full_range) {
        (Matrix::Bt709, false) => (298 * (y as i32 - 16), 459, 55, 136, 541),
        (Matrix::Bt601, false) => (298 * (y as i32 - 16), 409, 100, 208, 516),
        (Matrix::Bt709, true) => (256 * y as i32, 403, 48, 120, 475),
        (Matrix::Bt601, true) => (256 * y as i32, 359, 88, 183, 454),
    };
    let r = (luma + r_cr * cr + 128) >> 8;
    let g = (luma - g_cb * cb - g_cr * cr + 128) >> 8;
    let b = (luma + b_cb * cb + 128) >> 8;
    [clip(b), clip(g), clip(r)]
}

/// Convert an NV12 picture to blue-green-red-alpha pixels, rows from the top, and cut it to `out_width` by
/// `out_height` (the top left part of the decoded picture).
///
/// NV12 is a plane of brightness (`stride` bytes per row, `coded_height` rows) followed by one plane of blue and red
/// colour differences, interleaved, at half the width and half the height (`stride` bytes per row, `coded_height / 2`
/// rows). `data` holds both planes. Returns `false`, writing nothing, if `data` is too short for the planes or if the
/// asked-for size is bigger than the decoded picture.
#[allow(clippy::too_many_arguments)]
pub fn nv12_to_bgra(
    data: &[u8],
    stride: usize,
    coded_width: u32,
    coded_height: u32,
    out_width: u32,
    out_height: u32,
    matrix: Matrix,
    full_range: bool,
    out: &mut [u8],
) -> bool {
    let (cw, ch, ow, oh) = (coded_width as usize, coded_height as usize, out_width as usize, out_height as usize);
    if ow == 0 || oh == 0 || ow > cw || oh > ch || stride < cw || out.len() < ow * oh * 4 {
        return false;
    }
    // The colour plane starts after all the rows of the brightness plane, padding included. The colour rows are half as
    // many, rounded up.
    let colour_start = stride * ch;
    let colour_rows = ch.div_ceil(2);
    if data.len() < colour_start + stride * colour_rows {
        return false;
    }
    for row in 0..oh {
        let luma_row = &data[row * stride..row * stride + cw];
        let colour_row = &data[colour_start + (row / 2) * stride..colour_start + (row / 2) * stride + cw];
        let out_row = &mut out[row * ow * 4..(row + 1) * ow * 4];
        for x in 0..ow {
            let pair = (x / 2) * 2;
            let [b, g, r] = pixel(luma_row[x], colour_row[pair], colour_row[pair + 1], matrix, full_range);
            out_row[x * 4..x * 4 + 4].copy_from_slice(&[b, g, r, 255]);
        }
    }
    true
}

/// Cut a picture of blue-green-red-alpha pixels (rows from the top) down to its top left `to_width` by `to_height`.
/// Returns the picture unchanged when it is already that size or smaller.
pub fn crop_top_left(bgra: Vec<u8>, width: u32, height: u32, to_width: u32, to_height: u32) -> (Vec<u8>, u32, u32) {
    let (to_width, to_height) = (to_width.min(width), to_height.min(height));
    if (to_width, to_height) == (width, height) {
        return (bgra, width, height);
    }
    let mut out = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for row in 0..to_height as usize {
        let start = row * width as usize * 4;
        out.extend_from_slice(&bgra[start..start + to_width as usize * 4]);
    }
    (out, to_width, to_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 by 2 NV12 picture: one brightness plane of 4x2 and one colour plane of 2 pairs by 1 row.
    fn picture(y: [u8; 8], uv: [u8; 4]) -> Vec<u8> {
        let mut data = y.to_vec();
        data.extend_from_slice(&uv);
        data
    }

    fn convert(data: &[u8], matrix: Matrix, full: bool) -> Vec<u8> {
        let mut out = vec![0u8; 4 * 2 * 4];
        assert!(nv12_to_bgra(data, 4, 4, 2, 4, 2, matrix, full, &mut out));
        out
    }

    #[test]
    fn black_and_white_are_black_and_white_in_both_ranges() {
        // Limited range: 16 is black and 235 is white; colour differences at 128 are no colour.
        let out = convert(&picture([16, 16, 235, 235, 16, 16, 235, 235], [128; 4]), Matrix::Bt709, false);
        assert_eq!(&out[0..4], [0, 0, 0, 255]);
        assert_eq!(&out[8..12], [255, 255, 255, 255]);
        // Full range: 0 is black and 255 is white.
        let out = convert(&picture([0, 0, 255, 255, 0, 0, 255, 255], [128; 4]), Matrix::Bt709, true);
        assert_eq!(&out[0..4], [0, 0, 0, 255]);
        assert_eq!(&out[8..12], [255, 255, 255, 255]);
    }

    #[test]
    fn a_known_red_and_blue_come_out_red_and_blue() {
        // Pure red in BT.709, limited range: brightness 63, blue difference 102, red difference 240.
        let red = convert(&picture([63; 8], [102, 240, 102, 240]), Matrix::Bt709, false);
        assert!(red[2] >= 250 && red[1] <= 4 && red[0] <= 4, "{:?}", &red[0..4]);
        // Pure blue: brightness 32, blue difference 240, red difference 118.
        let blue = convert(&picture([32; 8], [240, 118, 240, 118]), Matrix::Bt709, false);
        assert!(blue[0] >= 250 && blue[1] <= 8 && blue[2] <= 8, "{:?}", &blue[0..4]);
    }

    #[test]
    fn the_two_matrices_and_the_two_ranges_give_different_colours() {
        let data = picture([100; 8], [90, 200, 90, 200]);
        let a = convert(&data, Matrix::Bt709, false);
        let b = convert(&data, Matrix::Bt601, false);
        let c = convert(&data, Matrix::Bt709, true);
        assert_ne!(a[0..4], b[0..4]);
        assert_ne!(a[0..4], c[0..4]);
    }

    #[test]
    fn each_pair_of_columns_and_each_pair_of_rows_share_their_colour() {
        // Two different brightnesses, one colour: only the brightness differs between neighbours.
        let out = convert(&picture([60, 160, 60, 160, 60, 160, 60, 160], [90, 200, 90, 200]), Matrix::Bt709, false);
        assert_ne!(out[0..4], out[4..8], "brightness changes across the pair");
        // The same colour over the pair of rows: the top and bottom pixels of a column are alike.
        assert_eq!(out[0..4], out[16..20]);
        // A different colour for the next pair of columns.
        let out = convert(&picture([100; 8], [90, 200, 200, 90]), Matrix::Bt709, false);
        assert_ne!(out[0..4], out[8..12]);
        assert_eq!(out[0..4], out[4..8], "the two columns of one pair are alike");
    }

    #[test]
    fn padding_rows_and_columns_are_left_out_and_the_planes_are_found_after_the_padding() {
        // Decoded 4 wide by 4 high (coded), shown 4 by 2, rows 6 bytes apart. The colour plane starts after 6 * 4 bytes.
        let mut data = vec![0u8; 6 * 4 + 6 * 2];
        for row in 0..4 {
            for x in 0..4 {
                data[row * 6 + x] = if row < 2 { 235 } else { 16 };
            }
        }
        for b in data[24..].iter_mut() {
            *b = 128;
        }
        let mut out = vec![0u8; 4 * 2 * 4];
        assert!(nv12_to_bgra(&data, 6, 4, 4, 4, 2, Matrix::Bt709, false, &mut out));
        assert!(out.chunks(4).all(|p| p == [255, 255, 255, 255]), "only the white top rows are shown");
    }

    #[test]
    fn a_picture_that_is_too_short_or_a_size_that_does_not_fit_is_refused_and_nothing_is_written() {
        let data = picture([16; 8], [128; 4]);
        let mut out = vec![7u8; 4 * 2 * 4];
        assert!(!nv12_to_bgra(&data[..data.len() - 1], 4, 4, 2, 4, 2, Matrix::Bt709, false, &mut out));
        assert!(!nv12_to_bgra(&data, 4, 4, 2, 6, 2, Matrix::Bt709, false, &mut out), "wider than decoded");
        assert!(!nv12_to_bgra(&data, 4, 4, 2, 4, 3, Matrix::Bt709, false, &mut out), "taller than decoded");
        assert!(!nv12_to_bgra(&data, 3, 4, 2, 4, 2, Matrix::Bt709, false, &mut out), "rows shorter than the width");
        assert!(!nv12_to_bgra(&data, 4, 4, 2, 4, 2, Matrix::Bt709, false, &mut out[..8]), "room too small");
        assert!(out.iter().all(|b| *b == 7));
    }

    #[test]
    fn a_picture_with_an_odd_number_of_rows_still_has_its_last_colour_row() {
        // 2 wide by 3 high: colour rows are ceil(3 / 2) = 2.
        let mut data = vec![16u8; 2 * 3];
        data.extend_from_slice(&[128; 2 * 2]);
        let mut out = vec![0u8; 2 * 3 * 4];
        assert!(nv12_to_bgra(&data, 2, 2, 3, 2, 3, Matrix::Bt709, false, &mut out));
        assert!(!nv12_to_bgra(&data[..data.len() - 1], 2, 2, 3, 2, 3, Matrix::Bt709, false, &mut out));
    }

    #[test]
    fn cutting_a_picture_keeps_its_top_left_part_and_leaves_one_that_fits_alone() {
        // 3 by 3, each pixel's first byte says where it is.
        let mut p = vec![0u8; 3 * 3 * 4];
        for i in 0..9 {
            p[i * 4] = i as u8;
        }
        let (cut, w, h) = crop_top_left(p.clone(), 3, 3, 2, 2);
        assert_eq!((w, h), (2, 2));
        assert_eq!(cut.chunks(4).map(|px| px[0]).collect::<Vec<_>>(), [0, 1, 3, 4]);
        let (same, w, h) = crop_top_left(p.clone(), 3, 3, 3, 3);
        assert_eq!((w, h, same), (3, 3, p.clone()));
        let (same, w, h) = crop_top_left(p.clone(), 3, 3, 9, 9);
        assert_eq!((w, h, same), (3, 3, p), "never made bigger");
    }

    #[test]
    fn the_reader_and_the_matrix_are_chosen_by_name_or_number() {
        assert_eq!(PictureReader::parse("rgb32").unwrap(), PictureReader::Rgb32);
        assert_eq!(PictureReader::parse(" NV12 ").unwrap(), PictureReader::Nv12);
        assert!(PictureReader::parse("yuv").unwrap_err().contains("rgb32 or nv12"));
        assert_eq!(PictureReader::default(), PictureReader::Rgb32);
        assert_eq!(Matrix::from_windows(1), Some(Matrix::Bt709));
        assert_eq!(Matrix::from_windows(2), Some(Matrix::Bt601));
        assert_eq!(Matrix::from_windows(0), None);
        assert_eq!(Matrix::assumed_for(1080), Matrix::Bt709);
        assert_eq!(Matrix::assumed_for(720), Matrix::Bt709);
        assert_eq!(Matrix::assumed_for(480), Matrix::Bt601);
    }
}
