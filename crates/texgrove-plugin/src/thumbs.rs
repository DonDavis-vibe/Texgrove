//! Small pictures of the textures for the gallery, made from the height maps themselves.

use std::cell::RefCell;
use std::collections::HashMap;

use base64::Engine;
use texgrove_core::heightmap::Tiling;
use texgrove_core::pattern::Pattern;

/// Pixels a side.
const SIZE: u32 = 72;
/// Tiles across, so the picture shows how the texture repeats.
const TILES: f32 = 2.0;

thread_local! {
    static CACHE: RefCell<HashMap<Pattern, String>> = RefCell::new(HashMap::new());
}

/// The pattern as a base64 PNG, lit from the upper left so heights read as relief.
pub fn of(pattern: Pattern) -> String {
    CACHE.with(|cache| cache.borrow_mut().entry(pattern).or_insert_with(|| render(pattern)).clone())
}

fn render(pattern: Pattern) -> String {
    let map = pattern.render(128);
    let rows = ((SIZE as f32) * map.aspect.min(1.0)).round().max(8.0) as u32;
    let mut pixels = Vec::with_capacity((SIZE * rows) as usize);
    let step = 1.0 / SIZE as f32;
    for y in 0..rows {
        for x in 0..SIZE {
            let u = (x as f32 + 0.5) / SIZE as f32 * TILES;
            // The picture's top row shows the texture's highest v, as on a wall.
            let v = (1.0 - (y as f32 + 0.5) / SIZE as f32) * TILES / map.aspect;
            let h = map.sample(u, v, Tiling::Repeat);
            // A light from the upper left: the slope towards it brightens.
            let dx = map.sample(u + step, v, Tiling::Repeat) - map.sample(u - step, v, Tiling::Repeat);
            let dy = map.sample(u, v + step, Tiling::Repeat) - map.sample(u, v - step, Tiling::Repeat);
            let shade = (0.35 + 0.45 * h + 1.6 * (dy - dx)).clamp(0.0, 1.0);
            pixels.push((shade * 255.0) as u8);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, SIZE, rows);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(&pixels);
        }
    }
    base64::engine::general_purpose::STANDARD.encode(png)
}
