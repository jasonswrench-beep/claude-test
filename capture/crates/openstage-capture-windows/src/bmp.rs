// SPDX-License-Identifier: Apache-2.0
//! The simplest picture file there is, so a test frame can be looked at in any viewer.
//!
//! A 24-bit BMP has no compression and no transparency, which is what we want here: what the
//! screen showed, as plain pixels. (Real recordings are video, never BMP.)

use std::io::{self, Write};

/// Write `bgra` (4 bytes per pixel, rows top to bottom, no padding) as a 24-bit BMP.
pub fn write_bmp(out: &mut impl Write, width: u32, height: u32, bgra: &[u8]) -> io::Result<()> {
    let expected = width as usize * height as usize * 4;
    if width == 0 || height == 0 || bgra.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a {width} by {height} picture needs {expected} bytes, got {}", bgra.len()),
        ));
    }
    let row_bytes = (width as usize * 3).div_ceil(4) * 4;
    let image_bytes = row_bytes * height as usize;
    let file_bytes = 14 + 40 + image_bytes;
    if file_bytes > u32::MAX as usize {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the picture is too large for a BMP file"));
    }

    // File header (14 bytes).
    out.write_all(b"BM")?;
    out.write_all(&(file_bytes as u32).to_le_bytes())?;
    out.write_all(&[0; 4])?;
    out.write_all(&54u32.to_le_bytes())?;
    // Info header (40 bytes). A negative height means the rows are stored top to bottom.
    out.write_all(&40u32.to_le_bytes())?;
    out.write_all(&(width as i32).to_le_bytes())?;
    out.write_all(&(-(height as i32)).to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&24u16.to_le_bytes())?;
    out.write_all(&0u32.to_le_bytes())?;
    out.write_all(&(image_bytes as u32).to_le_bytes())?;
    out.write_all(&2835i32.to_le_bytes())?;
    out.write_all(&2835i32.to_le_bytes())?;
    out.write_all(&0u32.to_le_bytes())?;
    out.write_all(&0u32.to_le_bytes())?;

    let mut row = vec![0u8; row_bytes];
    for y in 0..height as usize {
        let src = &bgra[y * width as usize * 4..(y + 1) * width as usize * 4];
        for (x, px) in src.chunks_exact(4).enumerate() {
            row[x * 3..x * 3 + 3].copy_from_slice(&px[..3]);
        }
        out.write_all(&row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn a_small_picture_has_the_right_headers_and_pixels() {
        // 3 by 2: rows of 9 bytes are padded to 12.
        let mut bgra = Vec::new();
        for i in 0..6u8 {
            bgra.extend_from_slice(&[i, 10 + i, 20 + i, 255]);
        }
        let mut out = Vec::new();
        write_bmp(&mut out, 3, 2, &bgra).unwrap();

        assert_eq!(&out[0..2], b"BM");
        assert_eq!(out.len(), 54 + 12 * 2);
        assert_eq!(le32(&out, 2) as usize, out.len());
        assert_eq!(le32(&out, 10), 54);
        assert_eq!(le32(&out, 18), 3);
        assert_eq!(i32::from_le_bytes(out[22..26].try_into().unwrap()), -2);
        assert_eq!(u16::from_le_bytes(out[28..30].try_into().unwrap()), 24);
        // First pixel of the first row: blue, green, red, with alpha dropped.
        assert_eq!(&out[54..57], &[0, 10, 20]);
        // First pixel of the second row starts after the padded first row.
        assert_eq!(&out[54 + 12..54 + 15], &[3, 13, 23]);
        // The padding is zero.
        assert_eq!(&out[54 + 9..54 + 12], &[0, 0, 0]);
    }

    #[test]
    fn the_wrong_number_of_bytes_is_refused() {
        let mut out = Vec::new();
        assert!(write_bmp(&mut out, 2, 2, &[0; 15]).is_err());
        assert!(write_bmp(&mut out, 0, 2, &[]).is_err());
    }
}
