//! A small picture of a mesh, drawn without a graphics card: for an AI to look at a result, and
//! for anything else that wants a picture where there is no window.

use crate::mesh::Mesh;
use crate::vec::{self, P3};

/// A picture: red, green and blue for every pixel, row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// From where the model is looked at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// The way the camera looks, from it towards the model.
    pub forward: P3,
}

impl Default for Camera {
    /// From the front left, a little from above, as models usually stand on the bed.
    fn default() -> Self {
        Camera { forward: [0.55, 0.65, -0.52] }
    }
}

const BACKGROUND: [f32; 3] = [246.0, 246.0, 248.0];
const MODEL: [f32; 3] = [196.0, 202.0, 214.0];
const MARKED: [f32; 3] = [240.0, 136.0, 62.0];

/// Draws the mesh into a square picture of `size` pixels, filling it but for a small margin,
/// without perspective. Triangles for which `marked` says so are drawn in orange.
pub fn render(mesh: &Mesh, size: u32, camera: &Camera, marked: Option<&[bool]>) -> Picture {
    let size = size.clamp(16, 2048);
    // Drawn at twice the size and shrunk, which smooths the edges.
    let big = (size * 2) as usize;
    let forward = vec::normalize(camera.forward);
    let up_hint = if forward[2].abs() > 0.99 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let right = vec::normalize(vec::cross(forward, up_hint));
    let up = vec::cross(right, forward);
    // The light stands above the camera's left shoulder.
    let light =
        vec::normalize(vec::add(vec::add(vec::scale(forward, -0.75), vec::scale(up, 0.55)), vec::scale(right, -0.35)));
    let projected: Vec<P3> =
        mesh.positions.iter().map(|p| [vec::dot(*p, right), vec::dot(*p, up), vec::dot(*p, forward)]).collect();
    let (mut low, mut high) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in &projected {
        for axis in 0..2 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    let mut colours = vec![BACKGROUND; big * big];
    let extent = (high[0] - low[0]).max(high[1] - low[1]);
    if mesh.triangles.is_empty() || !extent.is_finite() || extent <= 0.0 {
        return shrink(&colours, size);
    }
    let scale = big as f32 * 0.92 / extent;
    let centre = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
    let to_screen = |p: P3| -> P3 {
        [(p[0] - centre[0]) * scale + big as f32 / 2.0, big as f32 / 2.0 - (p[1] - centre[1]) * scale, p[2]]
    };
    let mut depth = vec![f32::INFINITY; big * big];
    for (t, corners) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = corners.map(|v| to_screen(projected[v as usize]));
        // Twice the triangle's area on the screen; its sign says which way it faces.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-9 {
            continue;
        }
        let normal = crate::normals::face_normal(mesh, t);
        // Lit from the front; a face seen from behind (an open or inside-out mesh) is darker.
        let facing = -vec::dot(normal, forward);
        let lit = if facing >= 0.0 { 0.28 + 0.72 * vec::dot(normal, light).max(0.0) } else { 0.22 };
        let base = if marked.is_some_and(|m| m.get(t).copied().unwrap_or(false)) { MARKED } else { MODEL };
        let colour = base.map(|c| c * lit);
        let (x0, x1) = (
            a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize,
            a[0].max(b[0]).max(c[0]).ceil().min(big as f32 - 1.0) as usize,
        );
        let (y0, y1) = (
            a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize,
            a[1].max(b[1]).max(c[1]).ceil().min(big as f32 - 1.0) as usize,
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let at = y * big + x;
                if z < depth[at] {
                    depth[at] = z;
                    colours[at] = colour;
                }
            }
        }
    }
    shrink(&colours, size)
}

/// Halves a picture drawn at twice the size.
fn shrink(colours: &[[f32; 3]], size: u32) -> Picture {
    let (size, big) = (size as usize, size as usize * 2);
    let mut rgb = Vec::with_capacity(size * size * 3);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                for (total, part) in sum.iter_mut().zip(colours[(2 * y + dy) * big + 2 * x + dx]) {
                    *total += part;
                }
            }
            rgb.extend(sum.map(|total| (total / 4.0).round().clamp(0.0, 255.0) as u8));
        }
    }
    Picture { width: size as u32, height: size as u32, rgb }
}

impl Picture {
    /// Several pictures of one size side by side.
    pub fn beside(pictures: &[Picture]) -> Picture {
        let Some(first) = pictures.first() else { return Picture { width: 0, height: 0, rgb: Vec::new() } };
        let (width, height) = (first.width as usize, first.height as usize);
        let mut rgb = Vec::with_capacity(width * height * 3 * pictures.len());
        for y in 0..height {
            for picture in pictures {
                rgb.extend_from_slice(&picture.rgb[y * width * 3..(y + 1) * width * 3]);
            }
        }
        Picture { width: (width * pictures.len()) as u32, height: height as u32, rgb }
    }

    /// The picture as a PNG file.
    #[cfg(feature = "pictures")]
    pub fn to_png(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width.max(1), self.height.max(1));
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(&self.rgb);
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    fn pixel(picture: &Picture, x: u32, y: u32) -> [u8; 3] {
        let at = ((y * picture.width + x) * 3) as usize;
        [picture.rgb[at], picture.rgb[at + 1], picture.rgb[at + 2]]
    }

    #[test]
    fn a_cube_fills_the_middle_and_shows_three_sides() {
        let cube = shapes::cuboid([10.0; 3]);
        let picture = render(&cube, 128, &Camera::default(), None);
        assert_eq!((picture.width, picture.height, picture.rgb.len()), (128, 128, 128 * 128 * 3));
        let background = BACKGROUND.map(|c| c as u8);
        assert_eq!(pixel(&picture, 1, 1), background);
        assert_eq!(pixel(&picture, 126, 126), background);
        assert_ne!(pixel(&picture, 64, 64), background);
        // The top is lighter than the two walls, and these differ from each other.
        let (top, left, right) = (pixel(&picture, 64, 30)[0], pixel(&picture, 40, 80)[0], pixel(&picture, 90, 80)[0]);
        assert!(top > left && top > right && left != right, "{top} {left} {right}");
        // Nearly square in the picture: the cube reaches close to both edges.
        let row: Vec<bool> = (0..128).map(|x| pixel(&picture, x, 64) != background).collect();
        assert!(row.iter().filter(|p| **p).count() > 100);
    }

    #[test]
    fn marked_triangles_are_orange_and_the_nearer_one_wins() {
        let cube = shapes::cuboid([10.0; 3]);
        let mut marked = vec![false; 12];
        marked[2] = true;
        marked[3] = true; // the top
        let picture = render(&cube, 128, &Camera::default(), Some(&marked));
        let [r, g, b] = pixel(&picture, 64, 30);
        assert!(r > g && g > b, "{r} {g} {b}");
        // The bottom is marked instead: it is hidden behind the rest.
        let mut marked = vec![false; 12];
        marked[0] = true;
        marked[1] = true;
        let hidden = render(&cube, 128, &Camera::default(), Some(&marked));
        assert_eq!(hidden, render(&cube, 128, &Camera::default(), None));
    }

    #[test]
    fn nothing_to_draw_is_an_empty_page_and_pictures_stand_side_by_side() {
        let empty = render(&Mesh::default(), 32, &Camera::default(), None);
        assert!(empty.rgb.chunks(3).all(|p| p == BACKGROUND.map(|c| c as u8)));
        let both = Picture::beside(&[empty.clone(), render(&shapes::cuboid([1.0; 3]), 32, &Camera::default(), None)]);
        assert_eq!((both.width, both.height), (64, 32));
        assert_eq!(pixel(&both, 0, 0), pixel(&empty, 0, 0));
        assert_ne!(pixel(&both, 48, 16), pixel(&empty, 16, 16));
        assert!(both.to_png().starts_with(b"\x89PNG"));
    }
}
