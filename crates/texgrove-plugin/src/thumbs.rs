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
    let picture = render(&map, tiles);
    CACHE.with(|cache| cache.borrow_mut().insert(texture.to_owned(), picture.clone()));
    Some(picture)
}

fn render(map: &Heightmap, tiles: f32) -> String {
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
