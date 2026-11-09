//! Laying a flat texture onto a model: for a point of the surface, where on the height map is it?
//!
//! The preview and the export both ask [`Projector::height`], so they cannot disagree.

use crate::heightmap::{Heightmap, Tiling};
use crate::mesh::Bounds;
use crate::vec::P3;
use core::f32::consts::{PI, TAU};

/// The way a texture is laid on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mapping {
    /// From all three axes at once, each counting by how much the surface faces it. Works on
    /// any shape without setting anything up.
    Triplanar {
        /// How quickly one direction takes over from another: 1 blends widely, 8 and more
        /// nearly switches.
        sharpness: f32,
    },
    /// Like a box round the model: every point takes the texture from the side it faces most.
    Cubic,
    /// Wrapped round the vertical (z) axis like a label round a bottle; the top and bottom get
    /// it from above and below.
    Cylindrical {
        /// Surfaces facing up or down within this many degrees count as top and bottom.
        cap_degrees: f32,
    },
    /// Wrapped round the model's centre like a map on a globe.
    Spherical,
    /// Straight from above (and below).
    PlanarXY,
    /// Straight from the front (and back).
    PlanarXZ,
    /// Straight from the side.
    PlanarYZ,
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping::Triplanar { sharpness: 4.0 }
    }
}

/// Where and how large the texture lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The width of one tile in mm.
    pub tile: f32,
    /// Stretches the tile's height: 1 keeps the texture's own proportions.
    pub stretch: f32,
    /// Shifts the texture, in mm along its width and height.
    pub offset: [f32; 2],
    /// Turns the texture, in degrees counter-clockwise.
    pub rotation: f32,
    pub tiling: Tiling,
}

impl Default for Placement {
    fn default() -> Self {
        Placement { tile: 5.0, stretch: 1.0, offset: [0.0; 2], rotation: 0.0, tiling: Tiling::Repeat }
    }
}

/// A mapping set up for one model.
#[derive(Debug, Clone, Copy)]
pub struct Projector {
    mapping: Mapping,
    tiling: Tiling,
    /// The corner of the model's box the texture starts from.
    origin: P3,
    centre: P3,
    /// The radius the texture is wrapped at, for the round mappings.
    radius: f32,
    /// One over the tile's width and height in mm.
    per_mm: [f32; 2],
    /// The same for the way round: the width adjusted so a whole number of tiles goes round.
    per_mm_round: f32,
    offset: [f32; 2],
    sin: f32,
    cos: f32,
    cap_cos: f32,
}

impl Projector {
    /// Sets a mapping up for a model with these bounds and a texture whose tile is `aspect`
    /// times as tall as wide. Use the bounds of the model as loaded, not of a refined or moved
    /// copy, or the texture shifts.
    pub fn new(mapping: Mapping, placement: &Placement, bounds: &Bounds, aspect: f32) -> Projector {
        let tile = if placement.tile.is_finite() { placement.tile.max(1e-3) } else { 5.0 };
        let stretch = if placement.stretch.is_finite() { placement.stretch.max(1e-3) } else { 1.0 };
        let tile_height = tile * stretch * if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let size = bounds.size();
        let radius = match mapping {
            Mapping::Spherical => 0.5 * size[0].max(size[1]).max(size[2]),
            _ => 0.5 * size[0].max(size[1]),
        }
        .max(1e-3);
        // Round the model a whole number of tiles, so the texture meets itself.
        let around = TAU * radius;
        let tiles_round = libm::roundf(around / tile).max(1.0);
        let (sin, cos) = (libm::sinf(placement.rotation.to_radians()), libm::cosf(placement.rotation.to_radians()));
        let cap_degrees = match mapping {
            Mapping::Cylindrical { cap_degrees } => cap_degrees.clamp(0.0, 90.0),
            _ => 0.0,
        };
        Projector {
            mapping,
            tiling: placement.tiling,
            origin: if bounds.is_empty() { [0.0; 3] } else { bounds.min },
            centre: if bounds.is_empty() { [0.0; 3] } else { bounds.centre() },
            radius,
            per_mm: [1.0 / tile, 1.0 / tile_height],
            per_mm_round: tiles_round / around,
            offset: placement.offset,
            sin,
            cos,
            cap_cos: libm::cosf(cap_degrees.to_radians()),
        }
    }

    /// How many tiles go round the model with the round mappings.
    pub fn tiles_round(&self) -> u32 {
        libm::roundf(self.per_mm_round * TAU * self.radius) as u32
    }

    /// The height of the texture (0 to 1) at a point of the surface with the given unit normal.
    pub fn height(&self, map: &Heightmap, p: P3, normal: P3) -> f32 {
        let [x, y, z] = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        let facing = |n: f32| if n < 0.0 { -1.0 } else { 1.0 };
        // Each side seen from outside, the texture upright and not mirrored.
        let from_x = |s: &Self| s.flat(map, facing(normal[0]) * y, z);
        let from_y = |s: &Self| s.flat(map, -facing(normal[1]) * x, z);
        let from_z = |s: &Self| s.flat(map, x, facing(normal[2]) * y);
        match self.mapping {
            Mapping::PlanarXY => from_z(self),
            Mapping::PlanarXZ => from_y(self),
            Mapping::PlanarYZ => from_x(self),
            Mapping::Cubic => {
                let [ax, ay, az] = normal.map(f32::abs);
                if az >= ax && az >= ay {
                    from_z(self)
                } else if ax >= ay {
                    from_x(self)
                } else {
                    from_y(self)
                }
            }
            Mapping::Triplanar { sharpness } => {
                let power = sharpness.clamp(1.0, 64.0);
                let weights = normal.map(|n| libm::powf(n.abs(), power));
                let total = weights[0] + weights[1] + weights[2];
                if total <= 0.0 {
                    return from_z(self); // no normal: any side is as good
                }
                let mut sum = 0.0;
                // A side that hardly counts is not worth looking up.
                if weights[0] > 1e-3 * total {
                    sum += weights[0] * from_x(self);
                }
                if weights[1] > 1e-3 * total {
                    sum += weights[1] * from_y(self);
                }
                if weights[2] > 1e-3 * total {
                    sum += weights[2] * from_z(self);
                }
                (sum / total).clamp(0.0, 1.0)
            }
            Mapping::Cylindrical { .. } => {
                if normal[2].abs() > self.cap_cos {
                    return from_z(self);
                }
                self.round(map, self.angle(p) * self.radius, z)
            }
            Mapping::Spherical => {
                let d = [p[0] - self.centre[0], p[1] - self.centre[1], p[2] - self.centre[2]];
                let reach = libm::sqrtf(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
                let latitude = if reach > 0.0 { libm::asinf((d[2] / reach).clamp(-1.0, 1.0)) } else { 0.0 };
                self.round(map, self.angle(p) * self.radius, (latitude + PI / 2.0) * self.radius)
            }
        }
    }

    /// The angle of a point round the vertical axis through the model's centre, from 0 to 2π.
    #[inline]
    fn angle(&self, p: P3) -> f32 {
        libm::atan2f(p[1] - self.centre[1], p[0] - self.centre[0]) + PI
    }

    /// The height at a place given in mm on a flat side.
    #[inline]
    fn flat(&self, map: &Heightmap, a: f32, b: f32) -> f32 {
        let (a, b) = self.turned(a, b);
        map.sample(a * self.per_mm[0], b * self.per_mm[1], self.tiling)
    }

    /// The height at a place given in mm round the model and along it.
    #[inline]
    fn round(&self, map: &Heightmap, along: f32, up: f32) -> f32 {
        let (a, b) = self.turned(along, up);
        // Only a quarter turn keeps the whole number of tiles round the model; any other
        // rotation shows a seam where the angle starts over.
        map.sample(a * self.per_mm_round, b * self.per_mm[1], self.tiling)
    }

    #[inline]
    fn turned(&self, a: f32, b: f32) -> (f32, f32) {
        let (a, b) = (a * self.cos + b * self.sin, -a * self.sin + b * self.cos);
        (a - self.offset[0], b - self.offset[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Pattern;

    fn bounds(size: P3) -> Bounds {
        Bounds { min: [0.0; 3], max: size }
    }

    /// Stripes along the first direction: the height tells where across a tile a place lies.
    fn stripes() -> Heightmap {
        Pattern::Grooves.render(256)
    }

    #[test]
    fn a_flat_side_measures_in_millimetres() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let top = Projector::new(Mapping::PlanarXY, &placement, &bounds([40.0; 3]), 1.0);
        let up = [0.0, 0.0, 1.0];
        // A groove every 4 mm along x, whatever y and z are: 0 at the tile's edge, 1 in its middle.
        assert!(top.height(&map, [0.0, 3.0, 9.0], up) < 0.02);
        assert!(top.height(&map, [2.0, 7.0, 1.0], up) > 0.98);
        assert!((top.height(&map, [1.0, 0.0, 0.0], up) - 0.5).abs() < 0.02);
        assert!((top.height(&map, [13.0, 0.0, 0.0], up) - 0.5).abs() < 0.02);
        // Shifted by 1 mm, the middle of the tile is at 3 mm.
        let shifted = Placement { offset: [1.0, 0.0], ..placement };
        let top = Projector::new(Mapping::PlanarXY, &shifted, &bounds([40.0; 3]), 1.0);
        assert!(top.height(&map, [3.0, 0.0, 0.0], up) > 0.98);
        // Turned a quarter, the grooves run along x and are counted along y.
        let turned = Placement { rotation: 90.0, ..placement };
        let top = Projector::new(Mapping::PlanarXY, &turned, &bounds([40.0; 3]), 1.0);
        assert!(top.height(&map, [17.3, 2.0, 0.0], up) > 0.98);
        assert!(top.height(&map, [17.3, 4.0, 0.0], up) < 0.02);
    }

    #[test]
    fn the_texture_starts_at_the_models_corner_wherever_the_model_is() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let here = Projector::new(Mapping::PlanarXY, &placement, &bounds([40.0; 3]), 1.0);
        let moved = Bounds { min: [101.3, -7.7, 3.0], max: [141.3, 32.3, 43.0] };
        let there = Projector::new(Mapping::PlanarXY, &placement, &moved, 1.0);
        let up = [0.0, 0.0, 1.0];
        for i in 0..40 {
            let x = i as f32 * 0.37;
            let (a, b) = (here.height(&map, [x, 1.0, 0.0], up), there.height(&map, [x + 101.3, -6.7, 3.0], up));
            assert!((a - b).abs() < 1e-3, "{a} {b}");
        }
    }

    #[test]
    fn a_box_takes_each_side_from_its_own_direction() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let cubic = Projector::new(Mapping::Cubic, &placement, &bounds([40.0; 3]), 1.0);
        // On the right side the grooves are counted along y, on the front along x.
        assert!(cubic.height(&map, [40.0, 2.0, 5.0], [1.0, 0.0, 0.0]) > 0.98);
        assert!(cubic.height(&map, [40.0, 4.0, 5.0], [1.0, 0.0, 0.0]) < 0.02);
        assert!(cubic.height(&map, [2.0, 0.0, 5.0], [0.0, -1.0, 0.0]) > 0.98);
        // Seen from outside the texture is not mirrored: on the left side it runs the other way.
        let ramp = Heightmap::from_fn(64, 1, |u, _| u);
        let left = cubic.height(&ramp, [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
        let right = cubic.height(&ramp, [40.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        assert!((right - 0.25).abs() < 0.02 && (left - 0.75).abs() < 0.02, "{left} {right}");

        // Triplanar agrees on a face and blends on a slope.
        let tri = Projector::new(Mapping::default(), &placement, &bounds([40.0; 3]), 1.0);
        assert!(tri.height(&map, [40.0, 2.0, 5.0], [1.0, 0.0, 0.0]) > 0.98);
        let d = core::f32::consts::FRAC_1_SQRT_2;
        let between = tri.height(&map, [2.0, 4.0, 5.0], [d, d, 0.0]);
        assert!((between - 0.5).abs() < 0.02, "{between}"); // 0 from the side, 1 from the front
    }

    #[test]
    fn round_a_cylinder_goes_a_whole_number_of_tiles() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        // 20 mm across: 62.83 mm round, so 16 tiles of 3.93 mm.
        let round =
            Projector::new(Mapping::Cylindrical { cap_degrees: 30.0 }, &placement, &bounds([20.0, 20.0, 50.0]), 1.0);
        assert_eq!(round.tiles_round(), 16);
        let at = |degrees: f32, z: f32| {
            let (s, c) = (libm::sinf(degrees.to_radians()), libm::cosf(degrees.to_radians()));
            round.height(&map, [10.0 + 10.0 * c, 10.0 + 10.0 * s, z], [c, s, 0.0])
        };
        // One tile is 22.5 degrees; the texture repeats with it and does not care about z.
        for degrees in [3.0f32, 77.0, 141.0, -100.0] {
            assert!((at(degrees, 5.0) - at(degrees + 22.5, 31.0)).abs() < 0.02);
        }
        // Where the angle starts over there is no step.
        assert!((at(179.99, 0.0) - at(-179.99, 0.0)).abs() < 0.02);
        // The top is a cap: flat from above.
        let top = Projector::new(Mapping::PlanarXY, &placement, &bounds([20.0, 20.0, 50.0]), 1.0);
        let up = [0.0, 0.0, 1.0];
        assert_eq!(round.height(&map, [6.0, 9.0, 50.0], up), top.height(&map, [6.0, 9.0, 50.0], up));
    }

    #[test]
    fn a_globe_has_no_step_where_the_angle_starts_over() {
        let map = Pattern::Dots.render(128);
        let placement = Placement { tile: 6.0, ..Placement::default() };
        let globe = Projector::new(Mapping::Spherical, &placement, &bounds([30.0; 3]), 1.0);
        for i in 1..20 {
            let latitude = (i as f32 / 20.0 - 0.5) * PI;
            let at = |degrees: f32| {
                let (s, c) = (libm::sinf(degrees.to_radians()), libm::cosf(degrees.to_radians()));
                let n = [c * libm::cosf(latitude), s * libm::cosf(latitude), libm::sinf(latitude)];
                globe.height(&map, [15.0 + 15.0 * n[0], 15.0 + 15.0 * n[1], 15.0 + 15.0 * n[2]], n)
            };
            assert!((at(179.99) - at(-179.99)).abs() < 0.05);
        }
    }

    #[test]
    fn odd_settings_do_not_break_it() {
        let map = stripes();
        let odd = Placement { tile: 0.0, stretch: f32::NAN, ..Placement::default() };
        let projector = Projector::new(Mapping::default(), &odd, &Bounds::EMPTY, f32::NAN);
        let h = projector.height(&map, [1.0, 2.0, 3.0], [0.0; 3]);
        assert!((0.0..=1.0).contains(&h));
    }
}
