//! Pictures as textures: a PNG or JPEG becomes a height map, bright is high.

use crate::heightmap::Heightmap;
use std::io::Cursor;

/// The longest side a picture keeps; larger ones are shrunk to it. Finer than this no printer
/// shows a tile, and a 4096² photo would cost 64 MB as heights.
pub const MAX_SIDE: u32 = 1024;

/// Why bytes are not a picture to use.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PictureError {
    #[error("only PNG and JPEG pictures can be used")]
    UnknownFormat,
    #[error("the picture cannot be read: {0}")]
    Broken(String),
    #[error("the picture is too large ({0} × {1} pixels)")]
    TooLarge(u32, u32),
}

/// Reads a PNG or JPEG as heights from 0 to 1 by brightness (transparent counts as black),
/// shrunk so its longest side is at most [`MAX_SIDE`].
pub fn read(bytes: &[u8]) -> Result<Heightmap, PictureError> {
    let (width, height, grey) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        read_png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        read_jpeg(bytes)?
    } else {
        return Err(PictureError::UnknownFormat);
    };
    let (width, height, grey) = shrink(width, height, &grey, MAX_SIDE);
    Heightmap::from_grey(width, height, &grey).map_err(|e| PictureError::Broken(e.to_string()))
}

/// Pictures beyond this many pixels are refused before they are unpacked.
const MAX_PIXELS: u64 = 64 << 20;

fn read_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PictureError> {
    let broken = |e: png::DecodingError| PictureError::Broken(e.to_string());
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(broken)?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(PictureError::TooLarge(width, height));
    }
    let size = reader.output_buffer_size().ok_or(PictureError::TooLarge(width, height))?;
    let mut buffer = vec![0u8; size];
    let frame = reader.next_frame(&mut buffer).map_err(broken)?;
    let channels = frame.color_type.samples();
    let line = frame.line_size;
    let mut grey = Vec::with_capacity((width * height) as usize);
    for y in 0..height as usize {
        let row = &buffer[y * line..y * line + width as usize * channels];
        for px in row.chunks_exact(channels) {
            grey.push(match channels {
                1 => px[0],
                2 => (u16::from(px[0]) * u16::from(px[1]) / 255) as u8,
                3 => luma(px[0], px[1], px[2]),
                _ => (u16::from(luma(px[0], px[1], px[2])) * u16::from(px[3]) / 255) as u8,
            });
        }
    }
    Ok((width, height, grey))
}

fn read_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PictureError> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::Luma);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(|e| PictureError::Broken(e.to_string()))?;
    let info = decoder.info().ok_or_else(|| PictureError::Broken("no picture in the file".into()))?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(PictureError::TooLarge(width, height));
    }
    let pixels = decoder.decode().map_err(|e| PictureError::Broken(e.to_string()))?;
    let channels = pixels.len() / (width as usize * height as usize).max(1);
    let grey = match channels {
        1 => pixels,
        3 => pixels.as_chunks::<3>().0.iter().map(|p| luma(p[0], p[1], p[2])).collect(),
        _ => return Err(PictureError::Broken(format!("{channels} colour channels"))),
    };
    Ok((width, height, grey))
}

/// Brightness as the eye sees it (Rec. 709 weights).
#[inline]
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 2126 + u32::from(g) * 7152 + u32::from(b) * 722) / 10_000) as u8
}

/// The picture shrunk by averaging so its longest side is at most `max`.
fn shrink(width: u32, height: u32, grey: &[u8], max: u32) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height);
    if longest <= max {
        return (width, height, grey.to_vec());
    }
    let (w, h) = (
        ((u64::from(width) * u64::from(max) / u64::from(longest)) as u32).max(1),
        ((u64::from(height) * u64::from(max) / u64::from(longest)) as u32).max(1),
    );
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let (y0, y1) = (y * height / h, ((y + 1) * height / h).max(y * height / h + 1));
        for x in 0..w {
            let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
            let mut sum = 0u64;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    sum += u64::from(grey[(yy * width + xx) as usize]);
                }
            }
            out.push((sum / u64::from((x1 - x0) * (y1 - y0))) as u8);
        }
    }
    (w, h, out)
}

/// The height map as a greyscale PNG (row by row from the top), to keep a picture once it was
/// read.
pub fn to_png(map: &Heightmap) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, map.width(), map.height());
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(&map.to_grey());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
        bytes
    }

    #[test]
    fn a_grey_png_reads_back_as_written() {
        let map = Heightmap::from_fn(16, 8, |u, v| (u + v) / 2.0);
        let back = read(&to_png(&map)).unwrap();
        assert_eq!((back.width(), back.height()), (16, 8));
        assert_eq!(back.aspect, 0.5);
        for (a, b) in back.values().iter().zip(map.values()) {
            assert!((a - b).abs() <= 0.5 / 255.0 + 1e-6);
        }
    }

    #[test]
    fn colour_and_transparency_turn_into_brightness() {
        // White, red, transparent white, half transparent white.
        let rgba = png(
            4,
            1,
            png::ColorType::Rgba,
            &[255, 255, 255, 255, 255, 0, 0, 255, 255, 255, 255, 0, 255, 255, 255, 128],
        );
        let map = read(&rgba).unwrap();
        let values: Vec<u8> = map.values().iter().map(|v| (v * 255.0).round() as u8).collect();
        assert_eq!(values, vec![255, 54, 0, 128]);
    }

    #[test]
    fn large_pictures_are_shrunk_and_other_files_refused() {
        let big = png(2048, 1024, png::ColorType::Grayscale, &vec![200u8; 2048 * 1024]);
        let map = read(&big).unwrap();
        assert_eq!((map.width(), map.height()), (1024, 512));
        assert!(map.values().iter().all(|v| (v - 200.0 / 255.0).abs() < 1e-6));
        assert_eq!(read(b"GIF89a....").unwrap_err(), PictureError::UnknownFormat);
        assert!(matches!(read(b"\x89PNG\r\n\x1a\nbroken"), Err(PictureError::Broken(_))));
        assert!(matches!(read(&[0xFF, 0xD8, 0xFF, 0x00]), Err(PictureError::Broken(_))));
    }
}
