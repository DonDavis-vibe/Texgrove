//! Small pictures of the textures for the gallery, made from the height maps themselves.

use std::cell::RefCell;
use std::collections::HashMap;

use base64::Engine;
use texgrove_core::heightmap::{Heightmap, Tiling};
use texgrove_core::pattern::Pattern;

/// Pixels a side.
const SIZE: u32 = 72;

thread_local! {
    static CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// A texture (a built-in pattern's id, or one of the user's) as a base64 PNG, lit from the upper
/// left so heights read as relief; `None` if it cannot be read.
pub fn of(texture: &str) -> Option<String> {
    if let Some(done) = CACHE.with(|cache| cache.borrow().get(texture).cloned()) {
        return Some(done);
    }
    // Patterns twice across, so the picture shows how they repeat; pictures once, they are busy
    // enough.
    let (map, tiles) = match Pattern::from_id(texture) {
        Some(pattern) => (pattern.render(128), 2.0),
        None => (crate::textures::map(texture).ok()?, 1.0),
    };
    let (rows, pixels) = pixels(&map, tiles);
    let picture = base64::engine::general_purpose::STANDARD.encode(png(SIZE, rows, &pixels));
    CACHE.with(|cache| cache.borrow_mut().insert(texture.to_owned(), picture.clone()));
    Some(picture)
}

/// A texture's little picture as greys, row by row: how many rows, and the pixels.
fn pixels_of(texture: &str) -> Option<(u32, Vec<u8>)> {
    let (map, tiles) = match Pattern::from_id(texture) {
        Some(pattern) => (pattern.render(128), 2.0),
        None => (crate::textures::map(texture).ok()?, 1.0),
    };
    Some(pixels(&map, tiles))
}

fn png(width: u32, height: u32, grey: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(grey);
    }
    png
}

/// The digits 0 to 9, three pixels wide and five tall, a row per three bits.
const DIGITS: [u16; 10] = [
    0b111_101_101_101_111,
    0b010_110_010_010_111,
    0b111_001_111_100_111,
    0b111_001_111_001_111,
    0b101_101_111_001_001,
    0b111_100_111_001_111,
    0b111_100_111_101_111,
    0b111_001_001_001_001,
    0b111_101_111_101_111,
    0b111_101_111_001_111,
];

/// The textures' little pictures on one sheet, numbered from 1 in the order given, as a PNG:
/// for an AI to look at. None when there is nothing to show.
pub fn sheet(textures: &[&str]) -> Option<Vec<u8>> {
    const COLUMNS: usize = 8;
    const GAP: usize = 6;
    const LABEL: usize = 14;
    if textures.is_empty() {
        return None;
    }
    let cell = SIZE as usize + GAP;
    let row_height = SIZE as usize + LABEL + GAP;
    let rows = textures.len().div_ceil(COLUMNS);
    let (width, height) = (COLUMNS * cell + GAP, rows * row_height + GAP);
    let mut sheet = vec![32u8; width * height];
    for (i, texture) in textures.iter().enumerate() {
        let (left, top) = (GAP + (i % COLUMNS) * cell, GAP + (i / COLUMNS) * row_height);
        if let Some((thumb_rows, grey)) = pixels_of(texture) {
            for y in 0..thumb_rows as usize {
                let from = y * SIZE as usize;
                sheet[(top + y) * width + left..(top + y) * width + left + SIZE as usize]
                    .copy_from_slice(&grey[from..from + SIZE as usize]);
            }
        }
        // The number below, each digit drawn twice as large.
        for (place, digit) in (i + 1).to_string().bytes().enumerate() {
            let bits = DIGITS[(digit - b'0') as usize];
            for y in 0..5 {
                for x in 0..3 {
                    if bits >> (14 - (y * 3 + x)) & 1 == 1 {
                        for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                            let px = left + place * 8 + x * 2 + dx;
                            let py = top + SIZE as usize + 3 + y * 2 + dy;
                            sheet[py * width + px] = 235;
                        }
                    }
                }
            }
        }
    }
    Some(png(width as u32, height as u32, &sheet))
}

fn pixels(map: &Heightmap, tiles: f32) -> (u32, Vec<u8>) {
    // What is finer than a pixel of the little picture only flickers there: smoothed away first.
    let per_pixel = map.width() as f32 * tiles / SIZE as f32;
    let smoothed;
    let map = if per_pixel > 1.5 {
        smoothed = map.blurred(per_pixel * 0.6, Tiling::Repeat);
        &smoothed
    } else {
        map
    };
    let rows = ((SIZE as f32) * map.aspect.min(1.0)).round().max(8.0) as u32;
    let mut pixels = Vec::with_capacity((SIZE * rows) as usize);
    let step = 1.0 / SIZE as f32;
    for y in 0..rows {
        for x in 0..SIZE {
            let u = (x as f32 + 0.5) / SIZE as f32 * tiles;
            // The picture's top row shows the texture's highest v, as on a wall.
            let v = (1.0 - (y as f32 + 0.5) / SIZE as f32) * tiles / map.aspect;
            let h = map.sample(u, v, Tiling::Repeat);
            // A light from the upper left: the slope towards it brightens.
            let dx = map.sample(u + step, v, Tiling::Repeat) - map.sample(u - step, v, Tiling::Repeat);
            let dy = map.sample(u, v + step, Tiling::Repeat) - map.sample(u, v - step, Tiling::Repeat);
            let shade = (0.35 + 0.45 * h + 1.6 * (dy - dx)).clamp(0.0, 1.0);
            pixels.push((shade * 255.0) as u8);
        }
    }
    (rows, pixels)
}
